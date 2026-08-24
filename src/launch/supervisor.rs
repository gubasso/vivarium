//! One cancellation tree owns every backend child and reader task.

use crate::launch::secure_fs::{self, SocketState};
use crate::launch::{
    BootMetadata, CommandSpec, ConfinementProfile, ConsoleReader, ConsoleSink, LaunchError,
    LaunchSpec,
};
use crate::protocol::validate_boot_identity;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const STARTUP_POLLS: usize = 400;
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long the guest is given to boot to the point where its agent answers on
/// the control socket — the rung a `viv start`'s wall clock is dominated by, and
/// the one that expires under host pressure. Expiry surfaces in the journal as
/// `timed out waiting for guest agent`.
///
/// 20s, and deliberately interim: it is sized as roughly 2.3x the ~8.5s
/// end-to-end boot observed on the development host (2026-08-14), not from a
/// per-rung distribution — the measurement that would settle it is Q-026 in
/// `docs/plan/open-questions.md`. Its predecessor was 10s, a ~1.2x margin that
/// a loaded host missed twice in one day. Two constraints bound it either way:
/// it MUST stay strictly below the launcher's `READINESS_TIMEOUT` (30s,
/// `src/main.rs`), with headroom for the failure report to cross the readiness
/// socket, so the supervisor is always the party that reports and the
/// launcher's own expiry keeps its distinct meaning (nothing reported at all);
/// and widening it further is not the reflex repair for readiness flakes under
/// load — `.config/nextest.toml`'s boot-width note records that pressure, which
/// no budget repairs.
const AGENT_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the credential relay legs are given to come up: host-local socket
/// work after the agent has already answered, so it shares no reason to grow
/// with the boot budget above.
const CREDENTIAL_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the namespace pair may take to appear in `/proc`: host-local
/// process creation, milliseconds in practice — this is generous slack rather
/// than a boot budget, and a pair this slow points at the host, not the guest.
const NAMESPACE_PAIR_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the guest is given to act on the ACPI power button before the VM is destroyed.
///
/// Sized against the bound above it rather than against a measurement of one guest: `viv stop`
/// defaults to a ten-second grace (spec/10), and this whole ladder has to finish inside it.
///
/// Six and not eight, which is the difference between fitting and exactly filling. The ladder also
/// issues two `ch-remote` calls, overshoots each wait by up to one poll interval, reaps every
/// child, and then removes the runtime directory — none of which is free, and all of which happens
/// after these two constants have spent their budget. At eight the total was exactly the ten the
/// host allows, so a guest that ignored the power button had the whole group killed out from under
/// the supervisor before `cleanup` ran, leaving the runtime directory populated and `viv stop`
/// reporting `teardown-incomplete` for a teardown that had in fact reached its last rung. Two
/// seconds of headroom is what keeps the escalation path reporting the outcome it actually had.
///
/// It is the whole grace and not the default one, which is a limitation rather than a design.
/// spec/10 gives the operator `viv stop --timeout` and says `-1` waits indefinitely, but that flag
/// is read by the CLI at stop time and this supervisor was started at `viv start` — nothing carries
/// the value across, so a longer request currently buys a longer wait before `systemctl kill`, not
/// a longer wait before the guest is destroyed. Tracked as `Q-018` in
/// `docs/plan/open-questions.md`; widening this constant is not the fix, because the default grace
/// is what it has to fit inside.
const GUEST_POWEROFF_TIMEOUT: Duration = Duration::from_secs(6);

/// How long the destroyed VM's processes are given to exit before they are killed.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Linux's `COMMAND_LINE_SIZE`, which is 2048 on both supported architectures.
///
/// A hard refusal rather than a warning, because the kernel copies what fits and drops the rest in
/// silence. Since ADR-0110 what rides here is one short parameter per declared mount — a kind, and
/// for a file plan an already-encoded basename — so the budget is no longer the thing that bounds
/// how many trees a sandbox may hold. It still bounds the mount count, and a truncated line still
/// reaches the guest as a shorter, valid, wrong plan rather than as an error.
const CMDLINE_LIMIT: usize = 2048;

/// Compose the guest kernel command line from the base, the boot identity, and each declared
/// mount's plan.
///
/// A free function rather than a method so the composition can be asserted directly. What crosses
/// here is the half of a mount plan that resolves only at launch: whether the declared source was
/// a directory or a regular file, and which entry of the share the file is. Where the share binds
/// is build output, carried in the contract's own bind table, which is what let ADR-0110 delete
/// the workspace's separate channel rather than move it.
fn compose_cmdline(
    base: &str,
    boot_identity: &str,
    shares: &[crate::launch::ShareSpec],
) -> Result<String, LaunchError> {
    use std::fmt::Write as _;
    let mut cmdline = format!("{base} vivarium.boot_identity={boot_identity}");
    // Matched by tag against the bind table the image carries (`mount-bind.sh`). The entry is
    // already percent-encoded — it goes verbatim, and the guest is the one decoder.
    for share in shares {
        match &share.mount_plan {
            None => {}
            Some(plan) => match (plan.kind, plan.entry.as_deref()) {
                // `Tree` rides as `dir`: the guest binds a mirrored tree and an ordinary
                // directory mount identically, so the distinction is the host's and stays there.
                (crate::launch::MountPlanKind::Dir | crate::launch::MountPlanKind::Tree, _) => {
                    let _ = write!(cmdline, " vivarium.mount.{}=dir", share.tag);
                }
                (crate::launch::MountPlanKind::File, Some(entry)) => {
                    let _ = write!(cmdline, " vivarium.mount.{}=file:{entry}", share.tag);
                }
                (crate::launch::MountPlanKind::File, None) => {
                    // `LaunchSpec::validate` refuses this shape; a spec that got here
                    // anyway must not boot a guest with half a plan.
                    return Err(LaunchError::InvalidSpec("a file mount plan lost its entry"));
                }
            },
        }
    }
    if cmdline.len() >= CMDLINE_LIMIT {
        return Err(LaunchError::InvalidSpec(
            "the launch parameters do not fit the guest kernel command line",
        ));
    }
    Ok(cmdline)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildKind {
    Virtiofsd(String),
    Vmm,
    /// Holds the per-VM user+net namespace pair open (spec/05).
    NetnsHolder,
    /// The unprivileged uplink process connecting the pair to the host's network.
    Uplink,
    /// The gating resolver, present only under allowlist mode.
    Resolver,
}

impl ChildKind {
    /// Whether this child ends with the guest. The namespace holder, the uplink,
    /// and the resolver outlive a guest poweroff by design and exit only when this
    /// supervisor kills them, so the shutdown ladder must not wait on them.
    const fn ends_with_guest(&self) -> bool {
        matches!(self, Self::Vmm | Self::Virtiofsd(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildExit {
    pub kind: ChildKind,
    pub status: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownReason {
    VmExit,
    ChildFailure(ChildExit),
    ExplicitStop,
    ProcessSignal,
    UnitStop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchReady {
    ProcessReady,
    /// The launch failed before readiness. Sent before the shutdown ladder and
    /// cleanup run, because cleanup unlinks the readiness socket — a report
    /// attempted after it can never connect, and the launcher would spend its
    /// whole handoff budget discovering nothing.
    Failed,
}

pub trait GuestReadiness: Send + Sync {
    fn wait<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), LaunchError>> + Send + 'a>>;
}

struct ManagedChild {
    kind: ChildKind,
    child: Child,
}

pub struct Supervisor {
    spec: LaunchSpec,
    cancellation: CancellationToken,
    tasks: JoinSet<Result<(), LaunchError>>,
    children: Vec<ManagedChild>,
}

impl Supervisor {
    #[must_use]
    pub fn new(spec: LaunchSpec) -> Self {
        Self {
            spec,
            cancellation: CancellationToken::new(),
            tasks: JoinSet::new(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Run create-before-boot launch and retain ownership until shutdown.
    ///
    /// # Errors
    ///
    /// Returns the first construction, readiness, child, task, or cleanup failure.
    pub async fn run(
        mut self,
        ready: mpsc::Sender<LaunchReady>,
    ) -> Result<ShutdownReason, LaunchError> {
        if let Err(error) = self.prepare_runtime().await {
            let _ = ready.send(LaunchReady::Failed).await;
            return Err(error);
        }
        if let Err(error) = self.run_inner(&ready).await {
            // Failure is reported before the ladder, not after, so the receiver
            // parked on this channel learns of it while the ladder (child
            // shutdown, then cleanup) is still spending its own budget. What the
            // report no longer depends on is winning that race: `cleanup` unlinks
            // the readiness socket, and `vivarium-supervisor` therefore holds an
            // already-open handoff connection rather than dialling the name after
            // this send. `let _` because a departed receiver must not turn a
            // launch failure into a different failure. The `monitor` arm below
            // deliberately sends nothing — readiness was already reported by then.
            let _ = ready.send(LaunchReady::Failed).await;
            self.cancellation.cancel();
            let _ = self.shutdown_children().await;
            let cleanup = self.cleanup().await;
            return cleanup.and(Err(error));
        }
        let reason = match self.monitor().await {
            Ok(reason) => reason,
            Err(error) => {
                self.cancellation.cancel();
                let _ = self.shutdown_children().await;
                let cleanup = self.cleanup().await;
                return cleanup.and(Err(error));
            }
        };
        if !matches!(reason, ShutdownReason::VmExit) {
            self.cancellation.cancel();
        }
        let shutdown = self.shutdown_children().await;
        let cleanup = self.cleanup().await;
        shutdown?;
        cleanup?;
        Ok(reason)
    }

    async fn run_inner(&mut self, ready: &mpsc::Sender<LaunchReady>) -> Result<(), LaunchError> {
        self.provision_volumes().await?;
        let profile = ConfinementProfile::new(&self.spec)?;
        let shares = profile.shares();
        let holder = profile.netns_holder();
        let drops_bounding_set = profile.drops_bounding_set();
        // Before the first daemon, not only with the VMM at the end: a share's confinement is
        // settled the moment its daemon opens its root, so the re-read that refuses a rendering
        // which lost a staging leg — or which points a staged daemon at the declared file's
        // parent — has to happen while there is still nothing serving (ADR-0105).
        ConfinementProfile::validate_shares(&shares, drops_bounding_set)?;
        let share_roots = self.open_share_roots()?;
        for (share, command) in self.spec.shares.clone().into_iter().zip(shares.iter()) {
            self.spawn_child(ChildKind::Virtiofsd(share.tag), command)?;
        }
        let share_sockets = self
            .spec
            .shares
            .iter()
            .map(|share| (share.socket.clone(), share.tag.clone()))
            .collect::<Vec<_>>();
        for (socket, tag) in share_sockets {
            self.wait_for_socket(&socket, Some(&tag)).await?;
        }
        // Every daemon has opened its root; the checked descriptors have done their pinning.
        drop(share_roots);
        let vmm = self.bring_up_network(&holder, drops_bounding_set).await?;
        ConfinementProfile::validate_rendered(&vmm, &shares, drops_bounding_set)?;
        self.spawn_child(ChildKind::Vmm, &vmm)?;
        self.write_vmm_pid().await?;
        let api_socket = self.spec.runtime_paths.api_socket.clone();
        self.wait_for_socket(&api_socket, None).await?;
        let metadata = self.prepare_boot_metadata().await?;
        self.write_vm_create_json().await?;
        self.write_boot_json(&metadata).await?;
        self.remote("create", Some(&self.spec.runtime_paths.vm_create_json))
            .await?;
        let console_socket = self.spec.runtime_paths.console_socket.clone();
        self.wait_for_socket(&console_socket, None).await?;
        let sink = ConsoleSink::open(
            &self.spec.runtime_paths.console_log,
            self.spec.console_log_enabled,
        )
        .await?;
        let reader = ConsoleReader::new();
        let (connected_tx, connected_rx) = oneshot::channel();
        let handle = reader.spawn(
            self.spec.runtime_paths.console_socket.clone(),
            sink,
            self.cancellation.clone(),
            connected_tx,
        );
        self.tasks
            .spawn(async move { handle.await.map_err(|_| LaunchError::Task)? });
        connected_rx
            .await
            .map_err(|_| LaunchError::Readiness("console connection"))?;
        self.remote("boot", None).await?;
        crate::launch::control::wait_for_agent(
            &self.spec.runtime_paths.control_socket,
            &metadata,
            AGENT_STARTUP_TIMEOUT,
        )
        .await?;
        let credential_tasks = crate::launch::credentials::start(
            self.spec.runtime_paths.control_socket.clone(),
            &self.spec.socket_legs.credentials,
            self.cancellation.clone(),
            CREDENTIAL_STARTUP_TIMEOUT,
        )
        .await?;
        for task in credential_tasks {
            self.tasks
                .spawn(async move { task.await.map_err(|_| LaunchError::Task)? });
        }
        ready
            .send(LaunchReady::ProcessReady)
            .await
            .map_err(|_| LaunchError::Readiness("initiator readiness channel"))?;
        Ok(())
    }

    async fn prepare_runtime(&self) -> Result<(), LaunchError> {
        self.spec.validate()?;
        secure_fs::private_dir(&self.spec.runtime_paths.root).await
    }

    /// Bring up the guest's network and return the VMM command joined into it.
    ///
    /// The order is the contract (spec/05): the holder makes the pair; the tap and
    /// the forwarding sysctl are configured inside it; under allowlist mode the
    /// default-deny ruleset and its literal destinations reach the kernel before
    /// the uplink or the VMM exist, so no window is open in which a guest packet
    /// could cross an unfiltered path; then the uplink, the resolver, and last the
    /// VMM. The pair, the tap, and the uplink exist in every mode; the filter and
    /// the resolver exist only under allowlist — absent, not inert.
    async fn bring_up_network(
        &mut self,
        holder: &CommandSpec,
        drops_bounding_set: bool,
    ) -> Result<CommandSpec, LaunchError> {
        self.spawn_child(ChildKind::NetnsHolder, holder)?;
        let holder_pid = self.child_pid(&ChildKind::NetnsHolder)?;
        crate::net::netns::await_pair(holder_pid, NAMESPACE_PAIR_TIMEOUT)
            .await
            .map_err(|_| LaunchError::Readiness("namespace pair"))?;
        let plan = {
            let profile =
                ConfinementProfile::with_bounding_set_drop(&self.spec, drops_bounding_set)?;
            let tap_steps: Vec<CommandSpec> = crate::net::tap::setup_sequences(
                &self.spec.network.tap_name,
                &self.spec.network.gateway_cidr(),
            )
            .iter()
            .map(|sequence| profile.in_namespace_ip(holder_pid, sequence))
            .collect();
            let link_show: Vec<String> = ["-j", "link", "show"]
                .iter()
                .map(ToString::to_string)
                .collect();
            (
                profile.net_init(holder_pid),
                tap_steps,
                profile.in_namespace_ip(holder_pid, &link_show),
                profile.uplink(holder_pid),
                profile.resolver(holder_pid),
                profile.vmm(holder_pid),
            )
        };
        let (net_init, tap_steps, link_show, uplink, resolver, vmm) = plan;
        run_checked(net_init).await?;
        for step in tap_steps {
            run_checked(step).await?;
        }
        let links = run_captured(&link_show).await?;
        // No carrier is the expected pre-attach state; what is asserted is that the
        // tap exists and is administratively up before a VMM is told to open it.
        crate::net::tap::assert_tap_up(&links, &self.spec.network.tap_name)
            .map_err(|_| LaunchError::Readiness("tap device"))?;
        if matches!(
            self.spec.egress.mode,
            crate::launch::LaunchEgressMode::Allowlist
        ) {
            let allowlist = crate::net::allowlist::Allowlist::parse(&self.spec.egress.allow)
                .map_err(|_| LaunchError::InvalidSpec("egress allowlist entry is malformed"))?;
            let runner = crate::net::nft::NftRunner::entered(
                &self.spec.backend_programs.nsenter,
                holder_pid,
                &self.spec.backend_programs.nft.display().to_string(),
            );
            runner
                .apply(&crate::net::nft::base_ruleset())
                .await
                .map_err(|_| LaunchError::Readiness("egress ruleset"))?;
            let literals = allowlist.literal_destinations();
            if !literals.is_empty() {
                runner
                    .apply(&crate::net::nft::literal_elements(&literals))
                    .await
                    .map_err(|_| LaunchError::Readiness("egress literal destinations"))?;
            }
        }
        self.spawn_child(ChildKind::Uplink, &uplink)?;
        if matches!(
            self.spec.egress.mode,
            crate::launch::LaunchEgressMode::Allowlist
        ) {
            self.spawn_child(ChildKind::Resolver, &resolver)?;
        }
        Ok(vmm)
    }

    fn child_pid(&self, kind: &ChildKind) -> Result<u32, LaunchError> {
        self.children
            .iter()
            .find(|managed| &managed.kind == kind)
            .and_then(|managed| managed.child.id())
            .ok_or(LaunchError::Task)
    }

    /// Creates any volume image that does not exist yet, formatted before it is reachable.
    ///
    /// Existence is the whole test, so what exists has to be complete. Truncating and formatting
    /// `volume.path` in place would break that: between the two steps the final name holds a bare
    /// sparse file with no filesystem, and a `mkfs.ext4` that fails — or a process that dies in the
    /// window — leaves it there. Every later boot would then find the path present, skip
    /// provisioning, and attach a disk the guest cannot mount, turning one transient failure into a
    /// permanent one no retry clears. Staging under a sibling name and publishing with `rename`
    /// makes the final path appear only once it is a filesystem. The staging suffix is deliberately
    /// not `.img`: `viv volume list` enumerates the directory by that extension, and a half-written
    /// image visible there would be reported as an orphan and offered to `viv volume prune`.
    async fn provision_volumes(&self) -> Result<(), LaunchError> {
        for volume in &self.spec.volumes {
            if fs::try_exists(&volume.path)
                .await
                .map_err(|error| LaunchError::io("inspect volume", error))?
            {
                continue;
            }
            let staging = volume.path.with_extension("img.staging");
            run_checked(CommandSpec::new(
                self.spec.backend_programs.truncate.clone(),
                vec![
                    "-s".into(),
                    format!("{}M", volume.size_mib),
                    staging.display().to_string(),
                ],
            ))
            .await?;
            let mut args = vec!["-q".into(), "-L".into(), volume.label.clone()];
            if let Some(ratio) = volume.inode_ratio {
                args.extend(["-i".into(), ratio.to_string()]);
            }
            args.push(staging.display().to_string());
            run_checked(CommandSpec::new(
                self.spec.backend_programs.mkfs_ext4.clone(),
                args,
            ))
            .await?;
            fs::rename(&staging, &volume.path)
                .await
                .map_err(|error| LaunchError::io("publish volume", error))?;
        }
        Ok(())
    }

    /// Type-check every share source through a descriptor, and hold the descriptors.
    ///
    /// spec/06 asks a source's type check to read through the descriptor the share is then served
    /// from. Measured on the target host (2026-08-18, virtiofsd 1.14.0): the daemon's own
    /// `--sandbox namespace` re-opens its root inside the new mount namespace, where a
    /// `/proc/self/fd/N` spelling no longer resolves (`Error entering sandbox: OpenNewRoot:
    /// NotFound`) — the same invocation with `--sandbox none` accepts it. The N20 sandbox wins, so
    /// the closest honest enactment is this: the type check reads through an `O_PATH` descriptor
    /// in the spawning process, the descriptors stay open (pinning each inode) until every daemon
    /// has opened its own root, and the daemon is handed the path. The residual window between
    /// this `fstat` and that open is bounded by the sandbox itself — the daemon pivots into
    /// whatever the path then names and can reach nothing else.
    ///
    /// What "the right type" means differs by share: a staged share's source is the declared file
    /// itself, which the staging step binds as the only entry of the export root (ADR-0105), while
    /// every other share is served as the directory it names.
    fn open_share_roots(&self) -> Result<Vec<rustix::fd::OwnedFd>, LaunchError> {
        self.spec
            .shares
            .iter()
            .map(|share| {
                let fd = rustix::fs::open(
                    &share.source,
                    rustix::fs::OFlags::PATH,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|errno| LaunchError::io("open share source", errno.into()))?;
                let stat = rustix::fs::fstat(&fd)
                    .map_err(|errno| LaunchError::io("inspect share source", errno.into()))?;
                let kind = rustix::fs::FileType::from_raw_mode(stat.st_mode);
                let staged = share.stage_dir(&self.spec.runtime_paths.root).is_some();
                if staged && !kind.is_file() {
                    return Err(LaunchError::InvalidSpec(
                        "a staged share source is not a regular file at spawn",
                    ));
                }
                if !staged && !kind.is_dir() {
                    return Err(LaunchError::InvalidSpec(
                        "share source is not a directory at spawn",
                    ));
                }
                Ok(fd)
            })
            .collect()
    }

    fn spawn_child(&mut self, kind: ChildKind, spec: &CommandSpec) -> Result<(), LaunchError> {
        let mut command = Command::new(spec.program());
        command
            .args(spec.args())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| LaunchError::io("spawn supervised child", error))?;
        if let Some(stdout) = child.stdout.take() {
            self.tasks.spawn(drain(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            self.tasks.spawn(drain(stderr));
        }
        self.children.push(ManagedChild { kind, child });
        Ok(())
    }

    async fn wait_for_socket(
        &mut self,
        path: &Path,
        share_tag: Option<&str>,
    ) -> Result<(), LaunchError> {
        for _ in 0..STARTUP_POLLS {
            if socket_exists(path).await? {
                return Ok(());
            }
            for managed in &mut self.children {
                if let Some(status) = managed
                    .child
                    .try_wait()
                    .map_err(|error| LaunchError::io("poll child", error))?
                {
                    // The pair dies with its holder and the guest's network with
                    // its uplink or resolver, so any of them exiting while a
                    // socket is awaited fails the launch rather than letting the
                    // wait run out its budget.
                    let relevant = match (&managed.kind, share_tag) {
                        (ChildKind::Virtiofsd(tag), Some(expected)) => tag == expected,
                        (
                            ChildKind::Vmm
                            | ChildKind::NetnsHolder
                            | ChildKind::Uplink
                            | ChildKind::Resolver,
                            _,
                        ) => true,
                        (ChildKind::Virtiofsd(_), None) => false,
                    };
                    if relevant {
                        return Err(LaunchError::ChildExit {
                            kind: "startup child",
                            status: status.code(),
                        });
                    }
                }
            }
            tokio::select! {
                () = self.cancellation.cancelled() => return Err(LaunchError::Cancelled),
                () = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
        Err(LaunchError::Readiness("backend socket"))
    }

    async fn remote(
        &self,
        operation: &'static str,
        payload: Option<&Path>,
    ) -> Result<(), LaunchError> {
        let mut args = vec![
            "--api-socket".into(),
            self.spec.runtime_paths.api_socket.display().to_string(),
            operation.into(),
        ];
        if let Some(path) = payload {
            args.push(path.display().to_string());
        }
        run_checked(CommandSpec::new(
            self.spec.backend_programs.ch_remote.clone(),
            args,
        ))
        .await
    }

    async fn prepare_boot_metadata(&mut self) -> Result<BootMetadata, LaunchError> {
        let boot_identity = fs::read_to_string("/proc/sys/kernel/random/uuid")
            .await
            .map_err(|error| LaunchError::io("read boot identity", error))?;
        let boot_identity = boot_identity.trim();
        validate_boot_identity(boot_identity)
            .map_err(|_| LaunchError::InvalidSpec("kernel UUID is malformed"))?;
        // Derived from the specification rather than carried, because since ADR-0110 a workspace
        // is an ordinary mount and there is no argument channel that would say which rows came
        // from `[[workspaces]]`. The equality below is the definition, not a guess at it.
        //
        // Through the same canonical form the resolution side uses, because `reusable()` compares
        // the two directly: the share order here is the build's and the resolved order there is
        // the manifest's, and two spellings of one set must not read as two sets.
        let mut workspace_paths = self
            .spec
            .workspace_shares()
            .into_iter()
            .map(|share| share.source.clone())
            .collect::<Vec<_>>();
        crate::launch::workspace::canonicalize_set(&mut workspace_paths);
        // An empty set is not refused here. A manifest declaring no workspace is refused at
        // resolution (ADR-0109), and a verification image boots by hand with mounts that land at
        // fixed targets rather than at their sources.
        let cmdline = self
            .spec
            .vm_create
            .get_mut("payload")
            .and_then(|payload| payload.get_mut("cmdline"))
            .and_then(|value| value.as_str())
            .ok_or(LaunchError::InvalidSpec("VM create cmdline is missing"))?;
        let cmdline = compose_cmdline(cmdline, boot_identity, &self.spec.shares)?;
        self.spec.vm_create["payload"]["cmdline"] = serde_json::Value::String(cmdline);
        Ok(BootMetadata {
            // The launch contract's number, not the guest-handshake protocol's: `launch.json`
            // and `boot.json` are one launch's record family, written and read by the host
            // side, and this is the version the reader's envelope compares against its own.
            schema_version: crate::launch::LAUNCH_SCHEMA_VERSION,
            boot_identity: boot_identity.to_owned(),
            sandbox_id: self.spec.sandbox_id.clone(),
            target: self.spec.target.clone(),
            backend: crate::launch::BACKEND.to_owned(),
            workspace_paths,
        })
    }

    async fn write_boot_json(&self, metadata: &BootMetadata) -> Result<(), LaunchError> {
        secure_fs::private_write(
            &self.spec.runtime_paths.boot_json,
            &serde_json::to_vec(metadata)
                .map_err(|_| LaunchError::InvalidSpec("boot metadata cannot be serialized"))?,
        )
        .await
    }

    async fn write_vm_create_json(&self) -> Result<(), LaunchError> {
        secure_fs::private_write(
            &self.spec.runtime_paths.vm_create_json,
            &serde_json::to_vec(&self.spec.vm_create)
                .map_err(|_| LaunchError::InvalidSpec("VM create JSON cannot be serialized"))?,
        )
        .await
    }

    async fn write_vmm_pid(&self) -> Result<(), LaunchError> {
        let pid = self
            .children
            .iter()
            .find(|child| matches!(child.kind, ChildKind::Vmm))
            .and_then(|child| child.child.id())
            .ok_or(LaunchError::Task)?;
        secure_fs::private_write(&self.spec.runtime_paths.vm_pid, pid.to_string().as_bytes()).await
    }

    async fn monitor(&mut self) -> Result<ShutdownReason, LaunchError> {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|error| LaunchError::io("install signal handler", error))?;
        loop {
            for managed in &mut self.children {
                if let Some(status) = managed
                    .child
                    .try_wait()
                    .map_err(|error| LaunchError::io("poll child", error))?
                {
                    let exit = ChildExit {
                        kind: managed.kind.clone(),
                        status: status.code(),
                    };
                    return Ok(if matches!(managed.kind, ChildKind::Vmm) {
                        ShutdownReason::VmExit
                    } else {
                        ShutdownReason::ChildFailure(exit)
                    });
                }
            }
            tokio::select! {
                () = self.cancellation.cancelled() => return Ok(ShutdownReason::ExplicitStop),
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| {
                        LaunchError::io("wait for process signal", error)
                    })?;
                    return Ok(ShutdownReason::ProcessSignal);
                },
                _ = terminate.recv() => return Ok(ShutdownReason::UnitStop),
                result = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    match result {
                        Some(Ok(Ok(()))) => {
                            return Ok(ShutdownReason::ChildFailure(ChildExit {
                                kind: ChildKind::Vmm,
                                status: None,
                            }));
                        }
                        Some(_) => return Err(LaunchError::Task),
                        None => {}
                    }
                }
                () = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
    }

    /// Whether any child that ends with the guest is still running.
    ///
    /// Deliberately not all children: the namespace holder, the uplink, and the
    /// resolver exit only when killed, so a ladder that waited on them would spend
    /// its whole poweroff budget on every stop for processes that were never going
    /// to comply.
    fn guest_children_alive(&mut self) -> Result<bool, LaunchError> {
        let mut alive = false;
        for managed in &mut self.children {
            if !managed.kind.ends_with_guest() {
                continue;
            }
            alive |= managed
                .child
                .try_wait()
                .map_err(|error| LaunchError::io("poll shutdown", error))?
                .is_none();
        }
        Ok(alive)
    }

    /// Wait until every guest-bound child has exited, or the budget expires.
    async fn await_children(&mut self, budget: Duration) -> Result<(), LaunchError> {
        let deadline = tokio::time::Instant::now() + budget;
        while self.guest_children_alive()? {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        Ok(())
    }

    async fn shutdown_children(&mut self) -> Result<(), LaunchError> {
        // spec/10's ladder, rungs two and three. `power-button` raises an ACPI event the guest
        // handles, so systemd inside it runs its own shutdown transaction and unmounts the volumes
        // — which is the whole of N18's "a stop removes nothing". `shutdown` does not do that: it
        // destroys the VM where it stands, and everything the guest had not yet committed is gone.
        //
        // Measured, and this is why the rung exists rather than being assumed: with `shutdown`
        // alone, a file written into the home volume and not explicitly `sync`ed was absent after
        // the next `viv start`, while a synced one survived. Nothing was wrong with the volume —
        // it was retained, reattached, and mounted from the same image — so the loss was the
        // shutdown path's, and no volume assertion could have found it.
        let _ = self.remote("power-button", None).await;
        self.await_children(GUEST_POWEROFF_TIMEOUT).await?;
        // Rung three, and it is reached only when the guest did not take the invitation. This
        // ladder sits inside spec/10's default ten-second grace, and inside that one only: a
        // `--timeout` larger than the default does not reach this process, so it does not move
        // the moment the guest is destroyed (`Q-018`, and the note on `GUEST_POWEROFF_TIMEOUT`).
        if self.guest_children_alive()? {
            let _ = self.remote("shutdown", None).await;
            self.await_children(SHUTDOWN_TIMEOUT).await?;
        }
        for managed in &mut self.children {
            if managed
                .child
                .try_wait()
                .map_err(|error| LaunchError::io("poll shutdown", error))?
                .is_none()
            {
                managed
                    .child
                    .start_kill()
                    .map_err(|error| LaunchError::io("kill child", error))?;
            }
            let _ = managed
                .child
                .wait()
                .await
                .map_err(|error| LaunchError::io("reap child", error))?;
        }
        self.cancellation.cancel();
        while let Some(result) = self.tasks.join_next().await {
            let _ = result;
        }
        Ok(())
    }

    /// Remove only artifacts declared by this launch.
    ///
    /// # Errors
    ///
    /// Returns an error rather than removing the directory when an unknown entry exists.
    pub async fn cleanup(&self) -> Result<(), LaunchError> {
        cleanup_runtime(&self.spec).await
    }
}

async fn run_checked(spec: CommandSpec) -> Result<(), LaunchError> {
    let status = Command::new(spec.program())
        .args(spec.args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|error| LaunchError::io("run launch helper", error))?;
    if status.success() {
        Ok(())
    } else {
        Err(LaunchError::ChildExit {
            kind: "launch helper",
            status: status.code(),
        })
    }
}

/// Like [`run_checked`], for the helpers whose stdout is the assertion input.
async fn run_captured(spec: &CommandSpec) -> Result<String, LaunchError> {
    let output = Command::new(spec.program())
        .args(spec.args())
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| LaunchError::io("run launch helper", error))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(LaunchError::ChildExit {
            kind: "launch helper",
            status: output.status.code(),
        })
    }
}

async fn drain<R: AsyncRead + Unpin>(mut reader: R) -> Result<(), LaunchError> {
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| LaunchError::io("drain child stream", error))?;
        if read == 0 {
            return Ok(());
        }
    }
}

async fn socket_exists(path: &Path) -> Result<bool, LaunchError> {
    Ok(secure_fs::socket_state(path).await? == SocketState::Socket)
}

async fn cleanup_runtime(spec: &LaunchSpec) -> Result<(), LaunchError> {
    // The per-target startup lock (spec/12) is tolerated by the scan below and removed by nothing.
    //
    // This sweep runs from teardowns a lock holder itself triggered: `viv start --rebuild` stops
    // the old VM while holding the lock, and a session's stale-record repair does the same. On
    // Unix, `flock` is held against an inode rather than a name, so unlinking the file here would
    // not release the holder's lock — it would end the mutual exclusion the name provides. The
    // next `viv` would create a fresh `lock`, take an uncontended lock on a different inode, and
    // walk into the boot decision beside a process that is still rebuilding. One boot per target
    // is exactly what the lock exists to guarantee, so the sweep leaves the name alone.
    //
    // It stays listed rather than unlisted because an unallowed entry aborts the sweep before
    // anything is removed, and every `stop` would then report an incomplete teardown.
    let lock = spec.runtime_paths.lock.clone();
    let mut allowed = vec![
        spec.runtime_paths.launch_spec.clone(),
        spec.runtime_paths.ready_socket.clone(),
        spec.runtime_paths.api_socket.clone(),
        // cloud-hypervisor takes an exclusive lock file beside its API socket and
        // leaves it behind, so a backend artefact appears in a directory vivarium
        // owns. Derived here rather than declared in `RuntimePaths` for the same
        // reason the rotated console logs are: the launcher never names this path,
        // the backend does. Confirmed at v53.0 by running the VMM with nothing but
        // `--api-socket`. Missing it was not a leaked file but a total cleanup
        // failure, because an unallowed entry aborts the sweep below before
        // anything is removed.
        PathBuf::from(format!("{}.lock", spec.runtime_paths.api_socket.display())),
        spec.runtime_paths.console_socket.clone(),
        spec.runtime_paths.control_socket.clone(),
        spec.runtime_paths.console_log.clone(),
        spec.runtime_paths.vm_pid.clone(),
        spec.runtime_paths.boot_json.clone(),
        spec.runtime_paths.vm_create_json.clone(),
        PathBuf::from(format!("{}.1", spec.runtime_paths.console_log.display())),
        PathBuf::from(format!("{}.2", spec.runtime_paths.console_log.display())),
    ];
    for share in &spec.shares {
        allowed.push(share.socket.clone());
        allowed.push(PathBuf::from(format!("{}.pid", share.socket.display())));
    }
    // A staged share's export root is a directory rather than a file, and it is empty on the host
    // by construction: the tmpfs and the bind that fill it exist only inside the daemon's own mount
    // namespace, which ends with the daemon (ADR-0105). It is swept separately for that reason —
    // `remove_file` cannot take a directory — and it is listed either way, because an entry nobody
    // named aborts the whole sweep and every `stop` would then report an incomplete teardown.
    let stages = spec
        .shares
        .iter()
        .filter_map(|share| share.stage_dir(&spec.runtime_paths.root))
        .collect::<Vec<_>>();
    let mut entries = match fs::read_dir(&spec.runtime_paths.root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(LaunchError::io("read runtime directory", error)),
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| LaunchError::io("read runtime entry", error))?
    {
        let path = entry.path();
        if path != lock && !allowed.contains(&path) && !stages.contains(&path) {
            return Err(LaunchError::UnknownRuntimeArtifact(path));
        }
    }
    for path in allowed {
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LaunchError::io("remove runtime artifact", error)),
        }
    }
    for path in stages {
        match fs::remove_dir(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LaunchError::io("remove share staging directory", error)),
        }
    }
    // The directory goes only when the retained lock is not in it. A `viv`-driven boot leaves the
    // lock behind and therefore the directory too, which costs one empty file under a runtime root
    // that does not outlive the session (spec/10); the diagnostic runner takes no lock, so the
    // paths that assert an empty runtime directory after shutdown still see one.
    match fs::metadata(&lock).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::remove_dir(&spec.runtime_paths.root)
                .await
                .map_err(|error| LaunchError::io("remove runtime directory", error))
        }
        Err(error) => Err(LaunchError::io("inspect startup lock", error)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::launch::{MountPlan, VolumeSpec};

    fn share(tag: &str, plan: Option<MountPlan>) -> crate::launch::ShareSpec {
        crate::launch::ShareSpec {
            tag: tag.to_owned(),
            source: PathBuf::from("/home/u/tree"),
            mount_point: PathBuf::from(format!("/run/vivarium-mounts/{tag}")),
            socket: PathBuf::from(format!("/run/user/1000/viv/{tag}.sock")),
            cache: "auto".to_owned(),
            read_only: false,
            mount_plan: plan,
            extra_args: Vec::new(),
        }
    }

    fn dir_plan() -> MountPlan {
        MountPlan {
            kind: crate::launch::MountPlanKind::Dir,
            entry: None,
            target: PathBuf::from("/home/u/tree"),
        }
    }

    fn file_plan(entry: &str) -> MountPlan {
        MountPlan {
            kind: crate::launch::MountPlanKind::File,
            entry: Some(entry.to_owned()),
            target: PathBuf::from("/home/u/.config/thing.toml"),
        }
    }

    /// Every plan reaches the guest, and the store share — which has none — contributes nothing.
    ///
    /// Since ADR-0110 this is the whole of what the command line carries beyond the boot identity:
    /// a workspace's host path stopped travelling here and became build output, so a parameter
    /// appearing for a share with no plan would be a leak of the channel this decision closed.
    #[test]
    fn each_declared_mount_plan_reaches_the_guest_as_its_own_parameter() {
        let shares = [
            share("store", None),
            share("mnt0", Some(dir_plan())),
            share("mnt1", Some(file_plan("thing.toml"))),
        ];
        let cmdline = compose_cmdline("console=ttyS0", "b7f0", &shares).unwrap();
        assert!(cmdline.starts_with("console=ttyS0 vivarium.boot_identity=b7f0"));
        assert!(cmdline.contains(" vivarium.mount.mnt0=dir"));
        assert!(cmdline.contains(" vivarium.mount.mnt1=file:thing.toml"));
        assert!(!cmdline.contains("vivarium.mount.store"));
        // The closed channel, asserted as absent rather than assumed: a workspace is an ordinary
        // mount now and nothing about it rides here.
        assert!(!cmdline.contains("vivarium.workspace"));
    }

    /// A file plan that lost its entry is refused rather than booted with half a plan.
    #[test]
    fn a_file_plan_without_its_entry_is_refused() {
        let mut plan = file_plan("thing.toml");
        plan.entry = None;
        let shares = [share("mnt0", Some(plan))];
        assert!(matches!(
            compose_cmdline("console=ttyS0", "b7f0", &shares),
            Err(LaunchError::InvalidSpec("a file mount plan lost its entry"))
        ));
    }

    #[test]
    fn the_command_line_budget_refuses_at_its_boundary() {
        let base = "console=ttyS0";
        let identity = "b7f0";
        // Grow one file plan's entry until the composition is refused, then check the last
        // accepted one sat just under the limit — which pins the comparison as `>=` rather
        // than `>`.
        let mut accepted = None;
        let mut refused = None;
        for length in 1..CMDLINE_LIMIT {
            let shares = [share("mnt0", Some(file_plan(&"a".repeat(length))))];
            match compose_cmdline(base, identity, &shares) {
                Ok(cmdline) => accepted = Some(cmdline.len()),
                // Any error ends the sweep; the assertions below are what decide whether it was
                // the refusal this test is about. Matching the reason rather than panicking on
                // the others keeps the failure a comparison a reader can see.
                Err(error) => {
                    refused = Some(format!("{error:?}"));
                    break;
                }
            }
        }
        assert_eq!(
            refused,
            Some(
                "InvalidSpec(\"the launch parameters do not fit the guest kernel command line\")"
                    .to_owned()
            )
        );
        assert_eq!(accepted, Some(CMDLINE_LIMIT - 1));
    }

    /// Many ordinary mounts fit, and enough of them do not.
    ///
    /// The bound moved with ADR-0110 and moved the right way: what rides here is now a short
    /// constant per share rather than a percent-encoded absolute path, so the budget stopped
    /// being what limits how many trees a sandbox may hold. Asserted rather than assumed, because
    /// "it got roomier" is exactly the kind of claim that rots quietly.
    #[test]
    fn an_ordinary_mount_set_fits_the_budget_and_an_extravagant_one_does_not() {
        let ordinary: Vec<_> = (0..40)
            .map(|index| share(&format!("mnt{index}"), Some(dir_plan())))
            .collect();
        assert!(compose_cmdline("console=ttyS0", "b7f0", &ordinary).is_ok());

        let extravagant: Vec<_> = (0..200)
            .map(|index| share(&format!("mnt{index}"), Some(dir_plan())))
            .collect();
        assert!(matches!(
            compose_cmdline("console=ttyS0", "b7f0", &extravagant),
            Err(LaunchError::InvalidSpec(
                "the launch parameters do not fit the guest kernel command line"
            ))
        ));
    }

    /// A `truncate`/`mkfs.ext4` stand-in that records its argv and creates the file it is given.
    ///
    /// Stubs rather than the real tools, and the reason is what this test is about. What
    /// `provision_volumes` decides is which images to create, which to leave alone, and which
    /// arguments each one gets — none of which is a fact about ext4. Calling the real `mkfs.ext4`
    /// would spend seconds formatting filesystems nothing mounts, in exchange for coverage the
    /// host lanes already have from a guest that boots off the result.
    fn recording_stub(path: &Path, log: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::write(
            path,
            format!(
                concat!(
                    "#!/bin/sh\n",
                    "printf '%s\\n' \"$*\" >> {}\n",
                    // The last argument is the path in both `truncate -s N PATH` and
                    // `mkfs.ext4 ... PATH`, so creating it is what makes the skip-if-present
                    // branch reachable on the next volume.
                    "for a in \"$@\"; do last=$a; done\n",
                    ": > \"$last\"\n",
                ),
                log.display()
            ),
        )?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }

    #[tokio::test]
    async fn provisions_every_declared_volume_and_leaves_an_existing_image_alone() {
        // `spec.volumes` was two fixed entries named on the command line until named volumes
        // landed, and nothing exercised this loop at any length — `tests/launch_supervision.rs`
        // builds its specification with `volumes: vec![]`. So "already generic over N volumes"
        // was a reading of the code rather than an observation of it.
        let scratch = crate::test_support::ScratchDirectory::new().unwrap();
        let root = scratch.path().to_path_buf();
        let log = root.join("argv.log");
        let truncate = root.join("truncate");
        let mkfs = root.join("mkfs.ext4");
        recording_stub(&truncate, &log).unwrap();
        recording_stub(&mkfs, &log).unwrap();

        // The warm case: an image that already exists is skipped whole, which is what makes a
        // restart reattach the user's data instead of formatting over it.
        let warm = root.join("default.img");
        std::fs::write(&warm, b"existing").unwrap();

        let mut spec = crate::launch::spec::tests::fixture();
        spec.backend_programs.truncate = truncate;
        spec.backend_programs.mkfs_ext4 = mkfs;
        spec.volumes = vec![
            VolumeSpec {
                label: "vivarium-default".into(),
                path: warm.clone(),
                size_mib: 32768,
                image_type: "raw".into(),
                inode_ratio: None,
            },
            VolumeSpec {
                label: "vivarium-store".into(),
                path: root.join("store.img"),
                size_mib: 32768,
                image_type: "raw".into(),
                inode_ratio: Some(8192),
            },
            VolumeSpec {
                label: "viv-cache".into(),
                path: root.join("cache.img"),
                size_mib: 4096,
                image_type: "raw".into(),
                inode_ratio: None,
            },
        ];

        Supervisor::new(spec).provision_volumes().await.unwrap();

        let calls = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = calls.lines().collect();
        // Two volumes created, four calls; the warm one contributes none and still holds its
        // bytes, so nothing reformatted it.
        assert_eq!(lines.len(), 4, "{calls}");
        assert_eq!(std::fs::read(&warm).unwrap(), b"existing");
        // Both steps address the staging name, never the final one. That is the whole guard: the
        // path the warm branch above tests for must not exist until it holds a filesystem, or a
        // `mkfs.ext4` that fails leaves a bare sparse file every later boot skips and attaches.
        assert!(
            lines[0].contains("-s 32768M") && lines[0].ends_with("store.img.staging"),
            "{calls}"
        );
        // ADR-0091 gives the store volume an inode ratio and every other volume the filesystem
        // default, so this is the one argument that may not appear twice.
        assert!(
            lines[1].contains("-L vivarium-store")
                && lines[1].contains("-i 8192")
                && lines[1].ends_with("store.img.staging"),
            "{calls}"
        );
        assert!(
            lines[2].contains("-s 4096M") && lines[2].ends_with("cache.img.staging"),
            "{calls}"
        );
        assert!(
            lines[3].contains("-L viv-cache")
                && !lines[3].contains("-i ")
                && lines[3].ends_with("cache.img.staging"),
            "{calls}"
        );

        // And publication happened: each image is at its final name and no staging file survives
        // to be enumerated by `viv volume list` or offered to `viv volume prune`.
        for name in ["store", "cache"] {
            assert!(root.join(format!("{name}.img")).exists(), "{name}");
            assert!(
                !root.join(format!("{name}.img.staging")).exists(),
                "{name} left staging residue"
            );
        }
    }
}
