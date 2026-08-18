//! Closed N20 command construction. Callers supply resources, never policy switches.

use crate::launch::{CommandSpec, LaunchError, LaunchSpec, ShareSpec};
use std::path::Path;

/// The legs of the wrapper every child carries, whatever the launcher's own privilege.
///
/// Each of these is refused by `setpriv` only for a reason that would also make the launch
/// wrong, so a missing one is a policy failure rather than a host fact.
const SETPRIV_POLICY: [&str; 3] = ["--no-new-privs", "--ambient-caps=-all", "--inh-caps=-all"];

/// Emptying the bounding set needs `CAP_SETPCAP`, so it is a host capability rather than a
/// policy switch.
///
/// `PR_CAPBSET_DROP` is privileged, and `setpriv` treats the refusal as fatal — it never
/// execs the child. An unprivileged launcher that asked for this unconditionally could
/// therefore never start a guest at all, which is what
/// [`ADR-0099`](../../docs/decisions/ADR-0099-the-bounding-set-drop-is-best-effort.md)
/// records. Omitting it costs nothing against the threat this profile closes: with
/// `--no-new-privs` set, a `setuid` binary cannot raise privilege, and the launcher's own
/// permitted and effective sets are already empty, so the bounding set bounds a capability
/// no descendant can acquire.
const SETPRIV_BOUNDING_SET: &str = "--bounding-set=-all";

/// `CAP_SETPCAP`, as a bit index into the capability masks `/proc/self/status` reports.
const CAP_SETPCAP: u32 = 8;

#[derive(Clone, Debug)]
pub struct ConfinementProfile<'a> {
    spec: &'a LaunchSpec,
    drop_bounding_set: bool,
}

impl<'a> ConfinementProfile<'a> {
    /// Validate a launch contract and bind it to the only policy constructors.
    ///
    /// # Errors
    ///
    /// Returns an error when the launch contract is unsafe or malformed.
    pub fn new(spec: &'a LaunchSpec) -> Result<Self, LaunchError> {
        Self::with_bounding_set_drop(spec, holds_cap_setpcap())
    }

    /// The constructor with the host observation supplied rather than made.
    ///
    /// Tests assert both renderings, and neither should depend on the privilege of
    /// whatever process happens to run them.
    pub(crate) fn with_bounding_set_drop(
        spec: &'a LaunchSpec,
        drop_bounding_set: bool,
    ) -> Result<Self, LaunchError> {
        spec.validate()?;
        Ok(Self {
            spec,
            drop_bounding_set,
        })
    }

    /// Whether this profile's commands empty their bounding set.
    #[must_use]
    pub const fn drops_bounding_set(&self) -> bool {
        self.drop_bounding_set
    }

    /// The API-only startup command, which carries no guest configuration, joined
    /// into the pair held by `holder_pid` so the VMM can open the tap that lives
    /// there.
    ///
    /// Landlock is deliberately absent here even when the host supports it. The pinned
    /// hypervisor's `--landlock` flag makes `--kernel` or `--firmware` mandatory on the
    /// command line — it has to know the paths it will allow — so passing it to a process
    /// that is started empty and configured later over the API makes the process refuse to
    /// start at all. The launch profile's path allowlist is requested through `vm.create`'s
    /// own `landlock_enable` and `landlock_rules`, which is the sequence
    /// `../../docs/reference/backend-capabilities.md` records for this version, and where
    /// the guest's paths are actually known.
    ///
    /// The bounding set is never emptied here, and by decision rather than by host
    /// fact: after the join, an exec by the pair's mapped root re-derives its
    /// capabilities from the bounding set, and the VMM must hold the pair's own
    /// `CAP_NET_ADMIN` to open the tap by name. Emptying it on a privileged host
    /// would make the same launch boot a guest without a NIC there and with one
    /// everywhere else. Nothing host-scoped is given up: the capabilities the join
    /// grants exist only inside the pair.
    #[must_use]
    pub fn vmm(&self, holder_pid: u32) -> CommandSpec {
        let child = vec![
            "--api-socket".into(),
            format!("path={}", self.spec.runtime_paths.api_socket.display()),
            "--seccomp".into(),
            "true".into(),
        ];
        let inner = wrapped(
            &self.spec.backend_programs.setpriv,
            &self.spec.backend_programs.cloud_hypervisor,
            child,
            false,
        );
        self.entered(holder_pid, &inner)
    }

    /// The holder that creates the pair and keeps it referenced: the pinned
    /// `unshare` around the pinned `sleep`, unwrapped because the pair's own
    /// mapped-root capabilities are the point of the process.
    #[must_use]
    pub fn netns_holder(&self) -> CommandSpec {
        CommandSpec::new(
            self.spec.backend_programs.unshare.clone(),
            crate::net::netns::create_pair_args(&crate::net::netns::holder_program(
                &self.spec.backend_programs.sleep.display().to_string(),
            )),
        )
    }

    /// One in-namespace `ip` invocation, joined by the holder's pid.
    #[must_use]
    pub fn in_namespace_ip(&self, holder_pid: u32, args: &[String]) -> CommandSpec {
        let mut program = vec![self.spec.backend_programs.ip.display().to_string()];
        program.extend_from_slice(args);
        CommandSpec::new(
            self.spec.backend_programs.nsenter.clone(),
            crate::net::netns::enter_pair_args(holder_pid, &program),
        )
    }

    /// The supervisor's own `net-init` entrypoint, run inside the pair to raise the
    /// forwarding sysctl no pinned tool writes.
    #[must_use]
    pub fn net_init(&self, holder_pid: u32) -> CommandSpec {
        let program = vec![
            self.spec.backend_programs.supervisor.display().to_string(),
            "net-init".to_owned(),
        ];
        CommandSpec::new(
            self.spec.backend_programs.nsenter.clone(),
            crate::net::netns::enter_pair_args(holder_pid, &program),
        )
    }

    /// The uplink: one unprivileged `pasta` per VM (spec/05). The process stays on
    /// the host side and opens its own device inside the pair, which is why it takes
    /// the namespace references rather than an `nsenter` wrap.
    #[must_use]
    pub fn uplink(&self, holder_pid: u32) -> CommandSpec {
        CommandSpec::new(
            self.spec.backend_programs.pasta.clone(),
            vec![
                "--foreground".into(),
                "--config-net".into(),
                "--dns-forward".into(),
                self.spec.network.dns_forward_address.to_string(),
                "--netns".into(),
                format!("/proc/{holder_pid}/ns/net"),
                "--userns".into(),
                format!("/proc/{holder_pid}/ns/user"),
            ],
        )
    }

    /// The gating resolver, joined into the pair: its socket is the only DNS the
    /// guest is given, and its filter programmer runs `nft` from inside.
    #[must_use]
    pub fn resolver(&self, holder_pid: u32) -> CommandSpec {
        let program = vec![
            self.spec.backend_programs.supervisor.display().to_string(),
            "resolver".to_owned(),
            "--spec".to_owned(),
            self.spec.runtime_paths.launch_spec.display().to_string(),
        ];
        CommandSpec::new(
            self.spec.backend_programs.nsenter.clone(),
            crate::net::netns::enter_pair_args(holder_pid, &program),
        )
    }

    fn entered(&self, holder_pid: u32, inner: &CommandSpec) -> CommandSpec {
        let mut program = vec![inner.program().display().to_string()];
        program.extend_from_slice(inner.args());
        CommandSpec::new(
            self.spec.backend_programs.nsenter.clone(),
            crate::net::netns::enter_pair_args(holder_pid, &program),
        )
    }

    #[must_use]
    pub fn shares(&self) -> Vec<CommandSpec> {
        self.spec
            .shares
            .iter()
            .map(|share| self.virtiofsd(share))
            .collect()
    }

    fn virtiofsd(&self, share: &ShareSpec) -> CommandSpec {
        let ids = self.spec.identity_translation;
        let stage = share.stage_dir(&self.spec.runtime_paths.root);
        // A staged share's daemon is pointed at the export root the staging step builds, never at
        // the declared file's parent (ADR-0105). Every other share keeps its own source.
        let shared_dir = stage
            .clone()
            .unwrap_or_else(|| share.source.clone())
            .display()
            .to_string();
        let mut child = vec![
            "--socket-path".into(),
            share.socket.display().to_string(),
            "--shared-dir".into(),
            shared_dir,
            "--sandbox".into(),
            "namespace".into(),
            "--seccomp".into(),
            "kill".into(),
            "--inode-file-handles=never".into(),
            "--thread-pool-size".into(),
            self.spec.descriptor_budget.worker_pool_size.to_string(),
            "--cache".into(),
            share.cache.clone(),
            format!("--rlimit-nofile={}", self.spec.descriptor_budget.limit),
        ];
        add_translation(
            &mut child,
            "--translate-uid",
            ids.guest_uid,
            ids.host_uid,
            ids.overflow_uid,
            ids.id_max,
        );
        add_translation(
            &mut child,
            "--translate-gid",
            ids.guest_gid,
            ids.host_gid,
            ids.overflow_gid,
            ids.id_max,
        );
        if share.read_only {
            child.push("--readonly".into());
        }
        child.extend(share.extra_args.clone());
        let daemon = wrapped(
            &self.spec.backend_programs.setpriv,
            &self.spec.backend_programs.virtiofsd,
            child,
            self.drop_bounding_set,
        );
        match stage {
            None => daemon,
            Some(stage) => staged(
                &self.spec.backend_programs.unshare,
                &self.spec.backend_programs.supervisor,
                share,
                &stage,
                &daemon,
            ),
        }
    }

    /// Re-read the rendered commands and refuse any that lost a mandatory leg.
    ///
    /// `bounding_set_dropped` is passed rather than inferred: the check has to know what
    /// the profile intended, or a rendering that silently lost the flag would look the same
    /// as one that never asked for it.
    ///
    /// The share half is separately callable, and the supervisor calls it before it spawns a
    /// single daemon: a check that a share serves the right directory is worth nothing once that
    /// share is already serving.
    pub(crate) fn validate_rendered(
        vmm: &CommandSpec,
        shares: &[CommandSpec],
        bounding_set_dropped: bool,
    ) -> Result<(), LaunchError> {
        // The VMM's rendering never drops the bounding set — see `vmm` — so its
        // wrapper is checked against that intent while the shares keep the host's.
        validate_wrapper(vmm, false)?;
        require_pair(vmm.args(), "--seccomp", "true")?;
        Self::validate_shares(shares, bounding_set_dropped)
    }

    /// The share half of [`Self::validate_rendered`], which runs before any daemon is spawned.
    ///
    /// The VMM's rendering is not in hand until the network is up, which is why the full check
    /// runs late. The shares' renderings are in hand immediately, and the confinement they carry
    /// is enforced by the daemon's own startup rather than by anything afterwards — a daemon
    /// pointed at the wrong directory has served it by the time a late check could object. So the
    /// share legs are re-read here, at the only moment refusing them still prevents anything.
    pub(crate) fn validate_shares(
        shares: &[CommandSpec],
        bounding_set_dropped: bool,
    ) -> Result<(), LaunchError> {
        for command in shares {
            validate_wrapper(command, bounding_set_dropped)?;
            validate_staging(command)?;
            require_pair(command.args(), "--sandbox", "namespace")?;
            require_pair(command.args(), "--seccomp", "kill")?;
            require_pair(command.args(), "--shared-dir", "")?;
            require_pair(command.args(), "--socket-path", "")?;
            if !command
                .args()
                .iter()
                .any(|arg| arg == "--inode-file-handles=never")
            {
                return Err(LaunchError::Policy("virtiofsd inode handle policy missing"));
            }
            if !command
                .args()
                .iter()
                .any(|arg| arg.starts_with("--rlimit-nofile="))
            {
                return Err(LaunchError::Policy("virtiofsd descriptor limit missing"));
            }
        }
        Ok(())
    }
}

fn wrapped(
    setpriv: &Path,
    child: &Path,
    child_args: Vec<String>,
    drop_bounding_set: bool,
) -> CommandSpec {
    let mut args = SETPRIV_POLICY
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if drop_bounding_set {
        args.push(SETPRIV_BOUNDING_SET.to_owned());
    }
    args.push("--".into());
    args.push(child.display().to_string());
    args.extend(child_args);
    CommandSpec::new(setpriv.to_path_buf(), args)
}

/// The legs that create the staging namespace, in the order `unshare` takes them.
///
/// `--keep-caps` is the load-bearing one and the easy one to lose. A user namespace grants its
/// creator every capability inside it, but a process that is not mapped to root drops them at
/// `execve`, so without this the staging program would run with no authority to mount at all —
/// measured on the target host, where every mount then fails with `must be superuser`. Keeping
/// them ambient carries them across the exec; `setpriv --ambient-caps=-all` inside the wrapper
/// drops them again before the daemon starts, so the authority exists for exactly the stretch
/// that builds the export root. `--map-current-user` rather than `--map-root-user` because
/// nothing here needs the daemon to believe it is root, and the host uid it runs as is the
/// identity translation's whole premise.
const STAGING_NAMESPACE: [&str; 6] = [
    "--user",
    "--map-current-user",
    "--keep-caps",
    "--mount",
    "--propagation",
    "private",
];

/// The staging subcommand's own name, shared by the renderer and its belt.
const STAGE_SHARE: &str = "stage-share";

/// Wrap a share's daemon in the namespace that gives it an export root holding one file.
///
/// The composition is three programs and one direction of travel: `unshare` makes the namespace,
/// the supervisor's own `stage-share` builds the export root inside it and hands over, and the
/// existing `setpriv`-wrapped daemon runs with the capabilities dropped again. Nothing the staging
/// does is visible on the host — the tmpfs and the bind live in a namespace that dies with the
/// daemon — which is why the confinement needs no privileged helper and leaves nothing to clean up
/// beyond an empty directory (ADR-0105).
fn staged(
    unshare: &Path,
    supervisor: &Path,
    share: &ShareSpec,
    stage: &Path,
    daemon: &CommandSpec,
) -> CommandSpec {
    let mut args = STAGING_NAMESPACE
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    args.extend([
        "--".into(),
        supervisor.display().to_string(),
        STAGE_SHARE.into(),
        "--source".into(),
        share.source.display().to_string(),
        "--stage".into(),
        stage.display().to_string(),
    ]);
    if share.read_only {
        args.push("--readonly".into());
    }
    args.push("--".into());
    args.push(daemon.program().display().to_string());
    args.extend(daemon.args().iter().cloned());
    CommandSpec::new(unshare.to_path_buf(), args)
}

/// Whether this process may empty its own capability bounding set.
///
/// Read from the kernel's own report rather than assumed from the effective user id: a
/// launcher can be given the capability without being root, and root in a user namespace
/// can lack it. A status file that cannot be read or parsed answers no, because the only
/// use of a wrong yes is a `setpriv` that refuses to exec the child at all.
fn holds_cap_setpcap() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
        .is_some_and(|capabilities| capabilities & (1 << CAP_SETPCAP) != 0)
}

fn add_translation(
    args: &mut Vec<String>,
    flag: &str,
    guest: u32,
    host: u32,
    overflow: u32,
    id_max: u32,
) {
    args.extend([flag.into(), format!("map:{guest}:{host}:1")]);
    if guest > 0 {
        args.extend([flag.into(), format!("forbid-guest:0:{guest}")]);
    }
    if guest < id_max {
        args.extend([
            flag.into(),
            format!("forbid-guest:{}:{}", guest + 1, id_max - guest),
        ]);
    }
    if host > 0 {
        args.extend([flag.into(), format!("squash-host:0:{overflow}:{host}")]);
    }
    if host < id_max {
        args.extend([
            flag.into(),
            format!("squash-host:{}:{overflow}:{}", host + 1, id_max - host),
        ]);
    }
}

/// Re-read a staged share's rendering: a staging step that lost a leg must not reach a daemon.
///
/// A share is staged or it is not, and the tell is `stage-share` in the argument list. When it is
/// there, the namespace legs have to be there too — a staging step that ran without `--keep-caps`
/// would fail to mount, and one that ran without `--mount` would mount on the host — and the
/// daemon's `--shared-dir` has to be the very directory the staging built. That last check is the
/// one that matters most: a rendering where the two disagree would serve the declared file's
/// parent again, silently, which is exactly the residual this construction exists to close.
fn validate_staging(command: &CommandSpec) -> Result<(), LaunchError> {
    if !command.args().iter().any(|arg| arg == STAGE_SHARE) {
        return Ok(());
    }
    for leg in STAGING_NAMESPACE {
        if !command.args().iter().any(|arg| arg == leg) {
            return Err(LaunchError::Policy("share staging namespace incomplete"));
        }
    }
    let value = |flag: &str| {
        command
            .args()
            .windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].clone())
    };
    let (Some(stage), Some(shared_dir), Some(source)) =
        (value("--stage"), value("--shared-dir"), value("--source"))
    else {
        return Err(LaunchError::Policy("share staging arguments incomplete"));
    };
    if stage != shared_dir || stage == source {
        return Err(LaunchError::Policy(
            "a staged share must serve its staging directory",
        ));
    }
    Ok(())
}

fn validate_wrapper(command: &CommandSpec, bounding_set_dropped: bool) -> Result<(), LaunchError> {
    let mandatory = SETPRIV_POLICY
        .iter()
        .copied()
        .chain(bounding_set_dropped.then_some(SETPRIV_BOUNDING_SET));
    for leg in mandatory {
        if !command.args().iter().any(|arg| arg == leg) {
            return Err(LaunchError::Policy(
                "capability/no-new-privileges wrapper incomplete",
            ));
        }
    }
    Ok(())
}

fn require_pair(args: &[String], flag: &str, value: &str) -> Result<(), LaunchError> {
    let found = args
        .windows(2)
        .any(|pair| pair[0] == flag && (value.is_empty() || pair[1] == value));
    if found {
        Ok(())
    } else {
        Err(LaunchError::Policy(
            "mandatory confinement argument missing",
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn command(args: &[&str]) -> CommandSpec {
        CommandSpec::new(
            std::path::PathBuf::from("/nix/store/setpriv"),
            args.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn mutation_table_rejects_every_mandatory_leg() {
        let base_vmm = [
            "--no-new-privs",
            "--bounding-set=-all",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--",
            "/nix/store/ch",
            "--seccomp",
            "true",
        ];
        let base_share = [
            "--no-new-privs",
            "--bounding-set=-all",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--",
            "/nix/store/virtiofsd",
            "--sandbox",
            "namespace",
            "--seccomp",
            "kill",
            "--shared-dir",
            "/work",
            "--socket-path",
            "/run/s",
            "--inode-file-handles=never",
            "--rlimit-nofile=524288",
        ];
        let vmm = command(&base_vmm);
        let share = command(&base_share);
        assert_eq!(1, [share.clone()].len());
        ConfinementProfile::validate_rendered(&vmm, &[share], true).unwrap();
        // `--bounding-set=-all` is deliberately absent from the vmm rows: the VMM's
        // rendering never drops the bounding set (see `vmm`), so its wrapper check
        // must not require it — asserted positively right below.
        for removed in [
            "--no-new-privs",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--seccomp",
        ] {
            let mutated = command(
                &base_vmm
                    .iter()
                    .copied()
                    .filter(|arg| *arg != removed)
                    .collect::<Vec<_>>(),
            );
            assert!(
                ConfinementProfile::validate_rendered(&mutated, &[command(&base_share)], true)
                    .is_err()
            );
        }
        let vmm_without_bounding = command(
            &base_vmm
                .iter()
                .copied()
                .filter(|arg| *arg != "--bounding-set=-all")
                .collect::<Vec<_>>(),
        );
        ConfinementProfile::validate_rendered(&vmm_without_bounding, &[command(&base_share)], true)
            .unwrap();
        // The shares keep requiring the drop when the host could make it.
        let share_without_bounding = command(
            &base_share
                .iter()
                .copied()
                .filter(|arg| *arg != "--bounding-set=-all")
                .collect::<Vec<_>>(),
        );
        assert!(
            ConfinementProfile::validate_rendered(&vmm, &[share_without_bounding], true).is_err()
        );
        let mut false_seccomp = base_vmm.map(str::to_owned);
        false_seccomp[7] = "false".into();
        assert!(
            ConfinementProfile::validate_rendered(
                &CommandSpec::new(
                    std::path::PathBuf::from("/nix/store/setpriv"),
                    false_seccomp.into_iter().collect(),
                ),
                &[command(&base_share)],
                true,
            )
            .is_err()
        );
        for (flag, replacement) in [
            ("--seccomp", "none"),
            ("--seccomp", "log"),
            ("--sandbox", "none"),
        ] {
            let mut args = base_share.map(str::to_owned);
            let index = args.iter().position(|arg| arg == flag).unwrap();
            args[index + 1] = replacement.into();
            let mutated = CommandSpec::new(
                std::path::PathBuf::from("/nix/store/setpriv"),
                args.into_iter().collect(),
            );
            assert!(ConfinementProfile::validate_rendered(&vmm, &[mutated], true).is_err());
        }
        for removed in ["--seccomp", "--ambient-caps=-all"] {
            let mutated = command(
                &base_share
                    .iter()
                    .copied()
                    .filter(|arg| *arg != removed)
                    .collect::<Vec<_>>(),
            );
            assert!(ConfinementProfile::validate_rendered(&vmm, &[mutated], true).is_err());
        }
    }

    /// A file mount's daemon is pointed at its staging directory, never at the file's parent.
    ///
    /// Rendered through the profile from the shared fixture rather than assembled by hand, or the
    /// assertion would compare one hand-written argument list against another and pass however the
    /// renderer changes. The negative half is the point: the parent directory must appear nowhere
    /// in the command, because a single argument naming it is the whole residual coming back.
    #[test]
    fn a_file_mount_is_served_from_its_staging_directory() {
        use crate::launch::spec::{MountPlan, MountPlanKind, tests::fixture};
        let mut spec = fixture();
        let root = spec.runtime_paths.root.clone();
        let source = std::env::current_dir().unwrap().join("Cargo.toml");
        spec.shares.push(ShareSpec {
            tag: "mnt0".into(),
            source: source.clone(),
            mount_point: "/run/vivarium-mounts/mnt0".into(),
            socket: root.join("mnt0.sock"),
            cache: "auto".into(),
            read_only: true,
            mount_plan: Some(MountPlan {
                kind: MountPlanKind::File,
                entry: Some("Cargo.toml".into()),
            }),
            extra_args: vec![],
        });
        let profile = ConfinementProfile::with_bounding_set_drop(&spec, true).unwrap();
        let shares = profile.shares();
        let staged = shares.last().unwrap();
        let stage = root.join("mnt0.stage").display().to_string();

        assert_eq!(staged.program(), spec.backend_programs.unshare);
        for leg in STAGING_NAMESPACE {
            assert!(staged.args().iter().any(|arg| arg == leg), "lost {leg}");
        }
        assert!(staged.args().contains(&stage));
        assert!(
            staged
                .args()
                .windows(2)
                .any(|pair| pair[0] == "--shared-dir" && pair[1] == stage),
            "the daemon was not pointed at the staging directory"
        );
        let parent = source.parent().unwrap().display().to_string();
        assert!(
            !staged.args().contains(&parent),
            "the declared file's parent directory reached the daemon's command line"
        );
        // The workspace share keeps the unstaged rendering, so the new construction is confined
        // to the share kind that needs it.
        assert_eq!(shares[0].program(), spec.backend_programs.setpriv);
        validate_staging(&shares[0]).unwrap();
        validate_staging(staged).unwrap();

        // The belt, against the two ways a rendering could quietly serve the parent again.
        let mutate = |edit: &dyn Fn(&mut Vec<String>)| {
            let mut args = staged.args().to_vec();
            edit(&mut args);
            CommandSpec::new(staged.program().to_path_buf(), args)
        };
        let served_elsewhere = mutate(&|args| {
            let index = args.iter().position(|arg| arg == "--shared-dir").unwrap();
            args[index + 1] = parent.clone();
        });
        assert!(validate_staging(&served_elsewhere).is_err());
        let without_keep_caps = mutate(&|args| args.retain(|arg| arg != "--keep-caps"));
        assert!(validate_staging(&without_keep_caps).is_err());
    }

    /// An unprivileged launcher renders a wrapper it can actually exec.
    ///
    /// `setpriv` refuses to run the child at all when it cannot empty the bounding set, so
    /// asking for it unconditionally is not a stricter profile — it is no guest. The three
    /// legs that need no privilege stay mandatory in both renderings, which is what keeps
    /// the fallback a narrowing of the wrapper rather than an abandonment of it.
    #[test]
    fn the_bounding_set_drop_is_the_only_leg_a_host_can_withhold() {
        let setpriv = std::path::PathBuf::from("/nix/store/setpriv");
        let child = std::path::PathBuf::from("/nix/store/ch");
        let dropped = wrapped(&setpriv, &child, Vec::new(), true);
        let kept = wrapped(&setpriv, &child, Vec::new(), false);

        assert!(dropped.args().iter().any(|arg| arg == SETPRIV_BOUNDING_SET));
        assert!(
            !kept.args().iter().any(|arg| arg == SETPRIV_BOUNDING_SET),
            "a launcher without CAP_SETPCAP still asked to empty the bounding set"
        );
        // The three unprivileged legs stay mandatory in both renderings, which is what
        // makes the fallback a narrowing of the wrapper rather than an abandonment of it.
        for leg in SETPRIV_POLICY {
            assert!(
                kept.args().iter().any(|arg| arg == leg),
                "the unprivileged rendering dropped {leg}, which needs no privilege"
            );
        }

        // Each rendering is checked against its own intent and against nothing else: a
        // command that lost the flag must not satisfy a check that expected it.
        validate_wrapper(&kept, false).unwrap();
        validate_wrapper(&dropped, true).unwrap();
        assert!(
            validate_wrapper(&kept, true).is_err(),
            "a rendering that omitted the drop passed a check that required it"
        );
        for leg in SETPRIV_POLICY {
            let mutated = command(
                &kept
                    .args()
                    .iter()
                    .map(String::as_str)
                    .filter(|arg| *arg != leg)
                    .collect::<Vec<_>>(),
            );
            assert!(
                validate_wrapper(&mutated, false).is_err(),
                "the unprivileged check accepted a wrapper missing {leg}"
            );
        }
    }
}
