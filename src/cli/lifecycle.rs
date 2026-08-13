//! The three lifecycle verbs: bring the VM up, say what it is doing, bring it down.
//!
//! What is new here is small on purpose. The supervisor, the confinement policy, and the transient
//! unit already exist and are host-proven; the generated flake already publishes the same runner
//! the diagnostic path executes. So `start` resolves, builds, mints an identity, and executes that
//! runner, which execs `viv start --spec` back into the handoff slice 002 built. Nothing here
//! supervises anything.
//!
//! The division of labour with `mod.rs` is that this file owns the lifecycle and that one owns the
//! readers. They share the `Context`, the failure vocabulary, and the resolution front half.

use std::fs::{self, File};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::{Context, Failure, ResolvedForLaunch, Success, diagnosed};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::launch::{BootMetadata, GuestSession, LaunchSpec, secure_fs};

/// The only target a project has today (spec/15).
pub(super) const DEFAULT_TARGET: &str = "default";

/// How long a reuse decision waits for the agent to answer a `Ping`.
///
/// Short, because this is a question about a VM that is supposed to be up already: if the socket is
/// there and the agent is healthy, one round trip answers it. A longer budget here would turn every
/// stale socket into a pause.
const PING_TIMEOUT: Duration = Duration::from_millis(500);

/// How long a live VM whose agent has not answered yet is given before it is called unavailable.
///
/// This is the boot-timeout arm of spec/12 step 4: the VM's own process is alive, so something is
/// coming up, and the answer is to wait rather than to boot a second one — but bounded, because
/// spec/12 requires a stated reason rather than a hang.
const AGENT_TIMEOUT: Duration = Duration::from_secs(30);

/// The reserved name of the home volume every project has (spec/06, spec/02).
const DEFAULT_VOLUME: &str = "default";

/// The undeclared volume backing the guest store's writable layer (spec/06, ADR-0087).
const STORE_VOLUME: &str = "store";

/// The lifecycle states ADR-0030 fixes. Closed, because `status` reports against it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// Nothing built and nothing running.
    Absent,
    /// A build output exists and no VM is up. Also the resting state a clean `stop` leaves.
    Built,
    /// Another process's boot, observed while it is in flight.
    Starting,
    /// The VM is up and its runtime records are live.
    Running,
    /// A stop in flight.
    Stopping,
    /// Runtime records present and the process behind them is gone.
    Failed,
}

impl State {
    /// The spelling spec/01's `state` field carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Built => "built",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
        }
    }
}

/// Everything `status` reports, resolved without writing anything.
pub struct Report {
    pub manifest: Option<String>,
    pub state: State,
    /// Why a `failed` VM failed, and `None` in every other state (spec/01, ADR-0030).
    ///
    /// ADR-0030 makes `failed` the state and the cause a reason, so the field is the only place
    /// the published causes — `crashed`, `boot-timeout`, `socket-lost` — can be told apart. spec/01
    /// fixes the set as open and obliges a consumer to tolerate a value it does not recognize, so
    /// vivarium names one cause per condition it can actually tell apart and adds to the set as it
    /// learns to tell more apart. Today only a dead recorded process is named; the other conditions
    /// `classify` reaches `failed` from share that name and are the next ones to earn their own.
    pub reason: Option<&'static str>,
    /// Meaningful only while `running` (ADR-0030): freshness derives from the store output path.
    pub stale: bool,
    pub store_path: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub resources: Option<Resources>,
}

/// The declared ceiling, never the measured use. spec/01 keeps the two in separate objects so a
/// consumer never has to guess which it is holding.
#[derive(Clone, Copy)]
pub struct Resources {
    pub mem_mib: u64,
    pub vcpu: u64,
}

/// Where one target's runtime files live, and the unit that owns them.
pub(super) struct Runtime {
    pub directory: PathBuf,
    pub unit: String,
}

impl Runtime {
    pub(super) fn locate(runtime_root: &Path, project_id: &str, target: &str) -> Self {
        Self {
            directory: runtime_root.join(project_id).join(target),
            // The same name `nix/runner.sh` hands `systemd-run --unit`, because `stop` and
            // `status` have to name the unit `start` created and neither can ask it.
            unit: format!("vivarium-{project_id}-{target}.service"),
        }
    }

    fn boot_json(&self) -> PathBuf {
        self.directory.join("boot.json")
    }

    fn vm_pid(&self) -> PathBuf {
        self.directory.join("vm.pid")
    }

    fn launch_spec(&self) -> PathBuf {
        self.directory.join("launch.json")
    }

    fn control_socket(&self) -> PathBuf {
        self.directory.join("control.sock")
    }

    /// The per-target startup lock (spec/12 step 1).
    ///
    /// Spelled here and declared in the launch specification's runtime paths, because the
    /// supervisor's teardown sweep has to recognize it. The two spellings are compared by the
    /// launcher's own validation, which requires every runtime path to be an exact child of this
    /// directory.
    fn lock(&self) -> PathBuf {
        self.directory.join("lock")
    }
}

/// The per-target `flock`, held across a reuse-or-boot decision and dropped before the session.
///
/// ADR-0053 fixes one total order for every lock vivarium takes — registry, identity index, this
/// one, then the Nix profile. It is taken after `mint_identity` has released the first two, which
/// is that order, and it is the reason a second `viv exec` racing a cold start waits for the boot
/// rather than starting a second one.
struct TargetLock(File);

impl TargetLock {
    fn acquire(runtime: &Runtime) -> Result<Self, Failure> {
        secure_fs::private_dir_blocking(&runtime.directory).map_err(|error| {
            diagnosed(
                Namespace::Vm,
                "runtime-directory-unusable",
                "the runtime directory for this target cannot be prepared",
                Locus::File(runtime.directory.clone()),
                format!("{error}"),
                ExitKind::IoErr,
            )
        })?;
        let path = runtime.lock();
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(secure_fs::PRIVATE_FILE_MODE)
            .open(&path)
            .map_err(|error| {
                diagnosed(
                    Namespace::Vm,
                    "lock-unusable",
                    "the startup lock for this target cannot be opened",
                    Locus::File(path.clone()),
                    format!("{error}"),
                    ExitKind::IoErr,
                )
            })?;
        // `try_lock` rather than `lock`, for the reason spec/12 gives it a code of its own: a
        // startup race is transient, and telling the caller to retry is an answer, while blocking
        // behind another process's build is a command that appears to have hung.
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(fs::TryLockError::WouldBlock) => Err(diagnosed(
                Namespace::Vm,
                "startup-contended",
                "another vivarium process is starting this project's VM",
                Locus::File(path),
                "the per-target startup lock could not be taken promptly",
                ExitKind::TempFail,
            )
            .with_hint("retry once the other process has finished starting the VM")),
            Err(fs::TryLockError::Error(error)) => Err(diagnosed(
                Namespace::Vm,
                "lock-unusable",
                "the startup lock for this target cannot be taken",
                Locus::File(path),
                format!("{error}"),
                ExitKind::IoErr,
            )),
        }
    }
}

impl Drop for TargetLock {
    fn drop(&mut self) {
        // `flock` releases on close and on process death, which is what stops a crashed `viv` from
        // wedging a project. Unlocking explicitly only makes the ordinary path explicit.
        let _ = self.0.unlock();
    }
}

/// The file `start` writes so a later `status` or `--no-rebuild` can name the last build.
///
/// Deliberately not a generation: spec/11's generation record, its retention, and its GC roots are
/// out of this slice's scope. This is one path, written after a successful build, and it is what
/// makes `built` distinguishable from `absent` after a clean stop.
fn last_build_path(roots: &config::XdgRoots, project_id: &str, target: &str) -> PathBuf {
    build_record(roots, project_id, target, "last-build")
}

/// The build the currently running VM was launched from, written just before the launcher runs.
///
/// Under the data root rather than in the runtime directory, and deliberately: the supervisor's
/// cleanup sweep removes only its own allowlist and aborts on anything else, so a freshness record
/// written beside the sockets would make every teardown fail with an unknown-artifact refusal.
fn running_build_path(roots: &config::XdgRoots, project_id: &str, target: &str) -> PathBuf {
    build_record(roots, project_id, target, "running-build")
}

fn build_record(roots: &config::XdgRoots, project_id: &str, target: &str, name: &str) -> PathBuf {
    roots
        .data
        .join("projects")
        .join(project_id)
        .join(target)
        .join(name)
}

/// Reads the recorded build output, and only if it still exists in the store.
///
/// The existence check is the point: a recorded path whose output has been collected is not a
/// build a `--no-rebuild` could boot, and reporting `built` for one would be a state that cannot
/// be acted on.
fn last_build(roots: &config::XdgRoots, project_id: &str, target: &str) -> Option<String> {
    read_build_record(&last_build_path(roots, project_id, target))
}

/// The same read for the running VM's own build, used only to answer freshness.
fn running_build(roots: &config::XdgRoots, project_id: &str, target: &str) -> Option<String> {
    read_build_record(&running_build_path(roots, project_id, target))
}

fn read_build_record(path: &Path) -> Option<String> {
    let recorded = std::fs::read_to_string(path).ok()?;
    let trimmed = recorded.trim();
    (!trimmed.is_empty() && Path::new(trimmed).exists()).then(|| trimmed.to_owned())
}

/// Whether the VM's own process is alive.
///
/// Read from the pid the supervisor recorded rather than from the unit, because the unit is the
/// lifetime owner and the pid is the VM: a unit that is still activating has no VM yet, and that
/// difference is exactly what separates `starting` from `running`.
fn vm_is_alive(runtime: &Runtime) -> bool {
    let Ok(raw) = std::fs::read_to_string(runtime.vm_pid()) else {
        return false;
    };
    let Ok(pid) = raw.trim().parse::<i32>() else {
        return false;
    };
    // A record that parses but names no process is a broken record, not a live VM, and it is
    // rejected here rather than resolved to something. Substituting `Pid::INIT` for it — which is
    // what `unwrap_or` did — asked the kernel about pid 1, which always exists and is never ours,
    // so a `vm.pid` holding `0` reported every VM as alive forever. The sign is filtered before
    // `Pid::from_raw` rather than by it, because that constructor debug-asserts on a negative.
    let Some(pid) = (pid > 0)
        .then(|| rustix::process::Pid::from_raw(pid))
        .flatten()
    else {
        return false;
    };
    // Signal 0 asks the kernel whether the process exists and whether we may signal it, and sends
    // nothing. `Errno::PERM` means it exists and belongs to someone else, which for a per-user
    // runtime directory should not happen — but it is still alive, so it counts as alive.
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(errno) => errno == rustix::io::Errno::PERM,
    }
}

/// What is behind this target's records, for spec/12 step 4's one decision.
///
/// Three answers rather than two, because step 4's two actions are both destructive against a VM
/// that is in fact running: cold-starting boots a second VM into a directory the first still owns,
/// and repairing stale records stops it. "I could not tell" therefore has to be sayable. Reading it
/// as `Dead` — which a boolean forces — spends an unreachable service manager on a teardown of
/// something that was working.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Presence {
    /// A process is there and its owning unit says the VM is its.
    Live,
    /// Nothing is there, or the unit that would own it says it is not running.
    Dead,
    /// A process is there and the unit could not be asked, so neither answer is established.
    Indeterminate,
}

/// Which of the three [`Presence`] answers this target's records support.
///
/// A live pid is necessary and not sufficient, for the reason [`classify`] already gives: pids are
/// reused, so records a crashed VM left behind can name a live process that is not this VM at all.
/// A pid with no process is the one answer the pid alone can give, and it is `Dead`.
fn vm_presence(runtime: &Runtime) -> Presence {
    if !vm_is_alive(runtime) {
        return Presence::Dead;
    }
    match unit_owns_a_vm(&runtime.unit) {
        Some(true) => Presence::Live,
        Some(false) => Presence::Dead,
        None => Presence::Indeterminate,
    }
}

/// Whether the owning unit is in a state that owns a VM, when the manager could be asked at all.
///
/// `None` is "the question could not be put" — no `systemctl`, a non-zero exit, no value, or a
/// state this version does not recognize — and it is deliberately not the same answer as
/// `inactive`.
/// [`unit_active_state`] collapses the two because [`classify`]'s worst case is a status reported
/// wrongly; step 4's worst case is a running VM stopped, so it needs them apart.
fn unit_owns_a_vm(unit: &str) -> Option<bool> {
    let output = Command::new("systemctl")
        .args(["--user", "show", "-p", "ActiveState", "--value", unit])
        .output()
        .ok()?;
    // A non-zero exit is a question that was refused, not a unit that is inactive. `show` answers
    // `inactive` for a unit it has never heard of, so the only way to reach here with a failure is
    // for the manager itself to be unreachable — exactly the case that must not read as dead.
    if !output.status.success() {
        return None;
    }
    ownership_of(String::from_utf8_lossy(&output.stdout).trim())
}

/// The state-name half of [`unit_owns_a_vm`], as a total function of what `ActiveState` said.
///
/// Separated for the reason [`classify`] is: every row is then testable without a systemd user
/// manager. The third answer is the one worth the separation — an empty value or a state name this
/// version does not know is `None`, because guessing which side of the boundary an unrecognized
/// state falls on is exactly the guess that costs a running VM.
fn ownership_of(state: &str) -> Option<bool> {
    match state {
        "active" | "reloading" | "refreshing" | "activating" | "deactivating" => Some(true),
        "inactive" | "failed" => Some(false),
        _ => None,
    }
}

/// Whether the systemd user unit that owns the VM's lifetime is loaded and doing something.
fn unit_active_state(unit: &str) -> Option<String> {
    let output = Command::new("systemctl")
        .args(["--user", "show", "-p", "ActiveState", "--value", unit])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// ADR-0030's discriminator, as a pure function of what the runtime directory and the store say.
///
/// The marker-teardown invariant is what makes it work: a clean `stop` removes the runtime records,
/// so "records present and the process is dead" is `failed` rather than `built`. ADR-0030 names
/// that dependency as the cost of this design, and `stop` below is the half that has to hold it.
pub(super) fn discriminate(runtime: &Runtime, has_build: bool) -> State {
    let has_records = runtime.boot_json().exists() || runtime.vm_pid().exists();
    classify(
        has_records,
        has_build,
        vm_is_alive(runtime),
        unit_active_state(&runtime.unit).as_deref(),
    )
}

/// The discriminator itself, as a total function of the four facts [`discriminate`] gathers.
///
/// Separated so every row is testable without a systemd user manager, a VM, or a live pid — the
/// three things a unit test of a state model should not need.
pub(super) fn classify(
    has_records: bool,
    has_build: bool,
    alive: bool,
    unit: Option<&str>,
) -> State {
    if !has_records {
        // A unit in transition outranks the resting states even with nothing on disk, because the
        // records are the supervisor's and the unit is submitted before the supervisor exists.
        // That window is precisely when spec/10 says a concurrent `status` observes `starting`,
        // and reading it as `built` would make the transitional state unobservable exactly where
        // it is defined to be visible.
        return match unit {
            Some("activating") => State::Starting,
            Some("deactivating") => State::Stopping,
            _ if has_build => State::Built,
            _ => State::Absent,
        };
    }
    if alive {
        // The unit is the lifetime owner, so a unit on its way out with a live VM is `stopping`
        // rather than `running` — the transitional state a concurrent `status` can observe.
        //
        // A live pid is necessary and not sufficient: pids are reused, so records left behind by a
        // crashed VM can name a live process that is not this VM at all. Reporting `running` for
        // one would send `exec` and `stop` at a VM that does not exist, so `running` requires the
        // owning unit to say so too, and anything else is the broken record ADR-0030 calls
        // `failed`.
        return match unit {
            Some("deactivating") => State::Stopping,
            Some("activating") => State::Starting,
            Some("active" | "reloading" | "refreshing") => State::Running,
            _ => State::Failed,
        };
    }
    // Records without a process. A unit still activating has not produced a VM yet, which is a
    // boot in flight rather than one that failed.
    match unit {
        Some("activating") => State::Starting,
        Some("deactivating") => State::Stopping,
        _ => State::Failed,
    }
}

/// The hard preflight subset spec/10 step 2 requires, cut to what this slice actually needs.
///
/// Deliberately not the whole `viv doctor` catalog: this checks the three things that decide
/// whether a boot can be attempted at all, and refuses before any build so a host that cannot hold
/// a VM never pays for one. Admission control — spec/10 step 3, owned by spec/17 and slice 005 —
/// is not implemented, and the slice `Revisions` records that gap rather than leaving it implied.
fn preflight<E: Environment>(context: &Context<'_, E>) -> Result<PathBuf, Failure> {
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;

    if !Path::new("/dev/kvm").exists() {
        return Err(diagnosed(
            Namespace::Host,
            "no-kvm",
            "this host has no /dev/kvm",
            Locus::Named("host preflight"),
            "vivarium boots a guest kernel behind hardware virtualization, which needs KVM",
            ExitKind::Unavailable,
        )
        .with_hint(
            "load the kvm module for your processor and check group membership on /dev/kvm",
        ));
    }

    Ok(runtime_root)
}

/// Bring the project's VM up and return once it is running (spec/10).
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), an unmet preflight, a content defect in the
/// merge (`65`), a build that fails, or a launcher that does not come up.
pub fn start<E: Environment>(
    context: &Context<'_, E>,
    rebuild: bool,
    no_rebuild: bool,
    attach: bool,
) -> Result<Success, Failure> {
    // Refused by name rather than accepted and dropped. spec/10 makes `--attach` a console-stream
    // whose post-condition is the opposite of the detached form's — it returns when the stream
    // ends, not when the guest answers — so a `start --attach` that quietly detached would report
    // success for something the user did not ask for. Slice 013 owns reaching into a running guest.
    if attach {
        return Err(diagnosed(
            Namespace::Internal,
            "not-implemented",
            "`viv start --attach` is not implemented yet",
            Locus::Named("command surface"),
            "the detached form is this slice's; the console stream and \
            its own post-condition are not",
            ExitKind::Software,
        )
        .with_hint("run `viv start` for the detached form"));
    }

    // spec/10 step 1: the bound manifest is resolved before anything else, so an unbound project
    // answers `78` on every host — including one that would fail preflight — and so nothing is
    // written for a project that never reaches step 5.
    let resolved = super::resolve_manifest_for_launch(context)?;
    // The manifest's own table, kept for the one path that cannot read the merged
    // one: `--no-rebuild`
    // evaluates nothing by definition (spec/10), so a piece's `vivarium.resources` is not available
    // to it. The evaluating path below overrides this with the merged value spec/04 makes
    // authoritative.
    let leaf_resources = resolved.resources;
    // Filled by the evaluating path below, and left empty by `--no-rebuild`.
    let mut merged = None;

    // Step 2.
    let runtime_root = preflight(context)?;

    // Minted here, and once. spec/10 places identity persistence in ensure-running, but every
    // artifact from this point on is keyed by the id — the generated tree and its lock, the build
    // records, the runtime directory, the volumes, the unit name — and spec/15 is explicit that a
    // resolved-but-unpersisted suffix is deterministic rather than stable: a concurrent first start
    // can take the suffix this one resolved. Minting before the first keyed write is what makes all
    // of them name one project. It is still after steps 1 and 2, so an unbound project or an
    // unusable host writes nothing.
    //
    // Minting also repairs a marker the user deleted, which is why it precedes the already-running
    // return below rather than sitting behind it.
    let project_id = config::mint_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;
    let runtime = Runtime::locate(&runtime_root, &project_id, DEFAULT_TARGET);
    // The same per-target lock a session takes (spec/12 step 1), because the two verbs perform the
    // same routine and a lock only one of them respects would guard nothing. Held to the end of
    // this function, which is the end of the boot.
    let _lock = TargetLock::acquire(&runtime)?;
    let running = matches!(
        discriminate(
            &runtime,
            last_build(&context.roots, &project_id, DEFAULT_TARGET).is_some()
        ),
        State::Running
    );

    // Step 4.
    let store_path = if no_rebuild {
        // The fast path spec/10 fixes: no evaluation at all, boot the last build as it stands. A
        // live VM short-circuits here because there is nothing to compare it against — asking for
        // no evaluation is asking not to learn whether it drifted.
        if running {
            return Ok(Success {
                stdout: String::new(),
                notes: format!("`{project_id}` is already running\n"),
            });
        }
        last_build(&context.roots, &project_id, DEFAULT_TARGET).ok_or_else(|| {
            diagnosed(
                Namespace::State,
                "no-build",
                "this project has no build to boot",
                Locus::Named("build history"),
                "`--no-rebuild` skips evaluation, and no previous build output is \
                recorded or still in the store",
                ExitKind::DataErr,
            )
            .with_hint("run `viv start` without `--no-rebuild` to evaluate and build first")
        })?
    } else {
        let (built, evaluated) = evaluate_and_build(context, &project_id, resolved)?;
        // spec/04: the launch channel is read by pure evaluation of the *merged* configuration, so
        // a `vivarium.resources` a piece proposes is as binding as the manifest's own table. Taken
        // only on this path, because it is the only one that evaluated anything.
        merged = evaluated;
        built
    };

    // N15 and spec/10 "Freshness and staleness", which are one rule read from two sides: an
    // already-running VM is a no-op when it is fresh, and is preserved with a warning when it is
    // not. Both answers need the fresh build in hand, which is why this sits after step 4 rather
    // than before it — a `start` that returned early would never produce the output it is meant to
    // build, and would leave the drift it is meant to report invisible to the next `status`.
    if running && !rebuild {
        let launched = running_build(&context.roots, &project_id, DEFAULT_TARGET);
        let stale = launched.is_some_and(|launched| launched != store_path);
        return Ok(Success {
            stdout: String::new(),
            notes: if stale {
                format!(
                    "`{project_id}` is running an older build and was left alone\n\
                    the new build is ready; `viv start --rebuild` replaces the \
                    running VM with it\n"
                )
            } else {
                format!("`{project_id}` is already running\n")
            },
        });
    }

    // A rebuild replaces a running VM rather than leaving it (spec/10). Stopping first is what
    // makes "replace" true; without it the launcher would meet a runtime directory that is in use.
    // The failure is propagated rather than dropped: a stop that did not happen leaves the old VM
    // alive, and continuing would overwrite its freshness record with a build it was not launched
    // from — making a still-stale VM report fresh.
    if rebuild {
        stop_unit(&runtime, false, None)?;
    }

    // Step 5, ensure running. The identity minted above is the one every artifact below names.
    launch(
        context,
        &project_id,
        &runtime,
        &store_path,
        effective_resources(merged.or(leaf_resources).as_ref()),
    )?;

    Ok(Success::plain(String::new()))
}

/// spec/10 step 4: evaluate the merged configuration and build the runner it publishes.
///
/// Shared rather than repeated, because `viv start` and a session that must cold-start are the same
/// step of the same routine, and a second spelling of it would be a second product.
fn evaluate_and_build<E: Environment>(
    context: &Context<'_, E>,
    project_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<(String, Option<config::Resources>), Failure> {
    let evaluated = super::evaluate_resolved_for_launch(context, project_id, resolved)?;
    let built = build_runner(&evaluated.flake_directory)?;
    write_build_record(
        &last_build_path(&context.roots, project_id, DEFAULT_TARGET),
        &built,
    )?;
    Ok((built, evaluated.resources))
}

/// spec/10 step 5: record what this boot is from, then launch and wait for the guest to answer.
///
/// The record is written first, so a `status` racing this boot never reports a VM as fresh against
/// a build it was not launched from.
fn launch<E: Environment>(
    context: &Context<'_, E>,
    project_id: &str,
    runtime: &Runtime,
    store_path: &str,
    resources: Resources,
) -> Result<(), Failure> {
    write_build_record(
        &running_build_path(&context.roots, project_id, DEFAULT_TARGET),
        store_path,
    )?;
    execute_runner(context, store_path, runtime, project_id, resources)
}

/// What a session needs to reach into a VM that is now known to be running.
pub(super) struct Prepared {
    pub control_socket: PathBuf,
    pub boot: BootMetadata,
    /// The workspace share's guest mount point, which spec/12 makes a session's cwd.
    pub workspace_cwd: PathBuf,
    /// What the guest itself supplies to every process started through the control socket.
    pub session: GuestSession,
}

/// spec/12's ensure-running: reuse the project's VM, or bring one up, and answer where it is.
///
/// This is the routine `viv exec` and `viv shell` share with `viv start` rather than a second boot
/// path beside it. The steps are spec/12's own, in its order, and the lock spans exactly the
/// decision: from before the socket is looked at, to after the guest has answered — a session opens
/// its connection with the lock already released, because sessions multiplex over the socket and
/// only startup is exclusive.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), a contended startup (`75`), a live VM whose
/// agent never answers (`69`), a boot record that names another project (`69`), and everything the
/// cold path can fail at.
pub(super) async fn ensure_running<E: Environment + Sync>(
    context: &Context<'_, E>,
) -> Result<Prepared, Failure> {
    // spec/10 step 1, before anything else and on every host: an unbound project answers `78`
    // whether or not this machine could have booted a VM.
    let resolved = super::resolve_manifest_for_launch(context)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let project_id = config::mint_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;
    let runtime = Runtime::locate(&runtime_root, &project_id, DEFAULT_TARGET);

    // Step 1. Everything from here to the end of the boot is exclusive per target.
    let _lock = TargetLock::acquire(&runtime)?;

    // Steps 2 and 3: when the socket is there, the agent answered for this boot, and the record
    // names this project, nothing is preflighted, evaluated, built, or booted.
    let boot = if let Some(boot) = reusable(&runtime, &project_id, &context.project).await? {
        boot
    } else {
        // Step 5, which is `viv start`'s own steps 2, 4, and 5 — the same preflight subset, the
        // same evaluation, the same launcher. The preflight runs only here, because spec/12 step 3
        // says a reused VM skips it: the host already held one.
        preflight(context)?;
        let (store_path, merged) = evaluate_and_build(context, &project_id, resolved)?;
        launch(
            context,
            &project_id,
            &runtime,
            &store_path,
            effective_resources(merged.as_ref()),
        )?;
        // The launcher returns only once the supervisor has awaited the guest agent's own readiness
        // handshake, so spec/12 step 5's "wait for readiness before releasing the lock" holds by
        // construction rather than by a second wait here.
        read_boot_metadata(&runtime).ok_or_else(|| {
            diagnosed(
                Namespace::Vm,
                "boot-record-unreadable",
                "the VM started but left no readable boot record",
                Locus::File(runtime.boot_json()),
                "a session is authorized by comparing the agent's answer against this record",
                ExitKind::Unavailable,
            )
        })?
    };
    prepared(&runtime, boot)
    // `_lock` falls out of scope here, which is what puts the release before the session's own
    // connection rather than after it.
}

/// Steps 2 through 4: whether the VM that is there can be reused, after repairing what is stale.
///
/// `None` means "cold start", and it is returned only for a target with nothing live behind it —
/// never for a VM that is running and merely confusing, because tearing one of those down is not a
/// repair.
async fn reusable(
    runtime: &Runtime,
    project_id: &str,
    workspace: &Path,
) -> Result<Option<BootMetadata>, Failure> {
    // Step 2. A socket that is not there at all is the ordinary cold case — but only once step 4's
    // question has been asked, because "no socket" and "no VM" are not the same fact. A live VM
    // whose control socket has gone is unreachable, not absent, and cold-starting on it would boot
    // a second VM into a runtime directory the first one still owns.
    if !runtime.control_socket().exists() {
        return match vm_presence(runtime) {
            Presence::Dead => Ok(None),
            Presence::Live => Err(unreachable(
                runtime,
                "the VM's own process is alive under its unit and its control socket is missing",
            )),
            Presence::Indeterminate => Err(undecided(runtime, "its control socket is missing")),
        };
    }
    let Some(boot) = read_boot_metadata(runtime) else {
        // A socket with no readable record behind it cannot authorize anything: the agent's answer
        // is checked against this file, so without it there is no way to know whose agent replied.
        // Step 4 still decides what to do about it — records are removed for a dead process, and a
        // live one is reported unavailable rather than stopped, because a VM that is running is not
        // a stale record and tearing it down is not a repair.
        return match vm_presence(runtime) {
            Presence::Dead => stale(runtime).map(|()| None),
            Presence::Live => Err(unreachable(
                runtime,
                "the VM's own process is alive under its unit and its boot record cannot be \
                read, so no connection to it can be authorized",
            )),
            Presence::Indeterminate => Err(undecided(runtime, "its boot record cannot be read")),
        };
    };
    // Step 3, the half `Ping` cannot answer. The identity comparison in the handshake proves which
    // *boot* replied; this proves the boot is the one this invocation meant.
    //
    // A record naming another project or target refuses outright, live or dead: this invocation
    // has no standing to repair another project's records, and the hint says who does.
    if boot.project_id != project_id || boot.target != DEFAULT_TARGET {
        return Err(foreign(runtime, "it names another project or target"));
    }
    // The other two fields spec/12 step 3 can decide. These name the same project, so a mismatch is
    // this project's own record to act on, and which action depends on step 4's question rather
    // than on the mismatch: a live VM is refused because replacing it is not a repair, and a dead
    // one is stale state that must be cleared or the project could never start again. Project
    // identity deliberately survives a directory move (spec/15), so the workspace path is the field
    // that notices one — and a moved project meeting its own crashed VM's record has to be able to
    // cold-start at the new path.
    let mismatch = if boot.workspace_host_path != workspace {
        Some("it names another workspace host path")
    } else if boot.backend != crate::launch::BACKEND {
        Some("it names another backend")
    } else {
        None
    };
    if let Some(why) = mismatch {
        return match vm_presence(runtime) {
            Presence::Dead => stale(runtime).map(|()| None),
            Presence::Live => Err(foreign(runtime, why)),
            Presence::Indeterminate => Err(undecided(runtime, why)),
        };
    }

    // Step 3's cheap `Ping`, over a connection the agent authorizes against `boot.json`.
    if ping(runtime, &boot, PING_TIMEOUT).await {
        return Ok(Some(boot));
    }

    // Step 4. The records are diagnostic evidence only — they say whether anything is still there
    // to wait for, and never by themselves that a VM is reusable.
    match vm_presence(runtime) {
        Presence::Dead => return stale(runtime).map(|()| None),
        Presence::Indeterminate => {
            return Err(undecided(runtime, "its guest agent did not answer a ping"));
        }
        Presence::Live => {}
    }
    // A live VM whose agent has not answered yet is a boot in flight, so this waits rather than
    // booting a second VM into the same runtime directory.
    if ping(runtime, &boot, AGENT_TIMEOUT).await {
        return Ok(Some(boot));
    }
    Err(unreachable(
        runtime,
        &format!(
            "the control socket did not complete a handshake within {}s, and the VM's \
            own process is still alive under its unit",
            AGENT_TIMEOUT.as_secs()
        ),
    ))
}

/// Step 4 with the question unanswered: a live process whose owning unit could not be consulted.
///
/// Refusing is the whole point of the third answer. Both actions this stands in front of —
/// cold-starting and repairing stale records — destroy a VM that is in fact running, and a pid
/// alone cannot tell a recycled number from this VM's own. So a service manager that cannot be
/// reached costs a stated `69` and a hint, rather than a teardown of something that was working.
fn undecided(runtime: &Runtime, what: &str) -> Failure {
    diagnosed(
        Namespace::Vm,
        "vm-presence-undecided",
        "whether this project's VM is still running could not be determined",
        Locus::File(runtime.vm_pid()),
        format!(
            "{what}, the recorded process is alive, and the unit that owns its lifetime \
            could not be asked — so neither reusing this VM nor replacing it is safe"
        ),
        ExitKind::Unavailable,
    )
    .with_hint("check `systemctl --user status <unit>`, then run `viv stop` if the VM is gone")
}

/// Step 3's refusal: a record that describes a VM this invocation did not ask for.
///
/// Never a teardown, whatever the mismatch: the caller is told which record is in the way and who
/// may clear it, because a `boot.json` naming something else is the one case where acting on it
/// could destroy a VM that is doing its job.
fn foreign(runtime: &Runtime, why: &str) -> Failure {
    diagnosed(
        Namespace::Vm,
        "foreign-boot-record",
        "this target's runtime directory holds a boot record for another VM",
        Locus::File(runtime.boot_json()),
        format!(
            "{why}, so reusing it would attach a session to the wrong VM, and \
            replacing it would tear down a VM this invocation does not own"
        ),
        ExitKind::Unavailable,
    )
    .with_hint("run `viv stop` for the project that owns it, or remove the stale record")
}

/// Step 4's other answer: a VM that is alive and cannot be reached is `69`, never a teardown.
///
/// Shared by the three ways a session can meet one — no socket, no readable boot record, and no
/// handshake — because spec/12 gives them one answer, and a second spelling of it would be a second
/// place for the "live process means wait or fail" rule to be forgotten.
fn unreachable(runtime: &Runtime, why: &str) -> Failure {
    diagnosed(
        Namespace::Vm,
        "agent-unreachable",
        "the VM is running but a session could not reach its guest agent",
        Locus::File(runtime.control_socket()),
        why.to_owned(),
        ExitKind::Unavailable,
    )
    .with_hint("run `viv stop` and start again, or check `journalctl --user -u <unit>`")
}

/// Step 4's repair: a dead VM's records are removed so the state model stops reading them as live.
fn stale(runtime: &Runtime) -> Result<(), Failure> {
    // The unit first, because a failed unit that is never reset keeps the name the next boot needs.
    // Its own teardown removes the runtime artifacts when its supervisor is still alive to do it.
    stop_unit(runtime, true, None)?;
    // What remains is what a supervisor that died could not sweep. Only the two records
    // `discriminate` reads are removed: they are what make a dead VM look present, and removing
    // more would be inventing a sweep the supervisor already owns.
    for path in [runtime.boot_json(), runtime.vm_pid()] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(diagnosed(
                    Namespace::Vm,
                    "stale-record",
                    "a stale runtime record could not be removed",
                    Locus::File(path),
                    format!("{error}"),
                    ExitKind::IoErr,
                ));
            }
        }
    }
    Ok(())
}

/// One authorized handshake against the current boot, with no opinion about why it failed.
async fn ping(runtime: &Runtime, boot: &BootMetadata, timeout: Duration) -> bool {
    crate::launch::control::wait_for_agent(&runtime.control_socket(), boot, timeout)
        .await
        .is_ok()
}

fn read_boot_metadata(runtime: &Runtime) -> Option<BootMetadata> {
    let bytes = fs::read(runtime.boot_json()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Reads out of the launch specification what a session needs and the boot record does not carry.
fn prepared(runtime: &Runtime, boot: BootMetadata) -> Result<Prepared, Failure> {
    let path = runtime.launch_spec();
    let unreadable = |why: &str| {
        diagnosed(
            Namespace::Vm,
            "launch-record-unreadable",
            "the running VM's launch specification cannot be read",
            Locus::File(path.clone()),
            why,
            ExitKind::Unavailable,
        )
    };
    let bytes = fs::read(&path).map_err(|error| unreadable(&format!("{error}")))?;
    // Deserialized rather than validated: `LaunchSpec::from_json` also checks the host facts that
    // were true when the VM was launched, and a session has no business re-litigating them.
    let spec: LaunchSpec = serde_json::from_slice(&bytes)
        .map_err(|_| unreadable("it does not match the launch schema this version understands"))?;
    let workspace_cwd = spec
        .workspace_share()
        .map(|share| share.mount_point.clone())
        .ok_or_else(|| unreadable("it declares no workspace share to start a session in"))?;
    Ok(Prepared {
        control_socket: runtime.control_socket(),
        boot,
        workspace_cwd,
        session: spec.guest_session,
    })
}

/// Builds the runner the generated flake publishes.
fn build_runner(flake_directory: &Path) -> Result<String, Failure> {
    let attribute = format!("{}#runner.{}", flake_directory.display(), nix_system());
    let output = Command::new("nix")
        .args([
            "build",
            "--no-link",
            "--print-out-paths",
            "--extra-experimental-features",
            "nix-command flakes",
            // The pin the first evaluation installed is the one that decides this build. Letting
            // `nix` update it here would be an unannounced input jump (N3).
            "--no-update-lock-file",
            &attribute,
        ])
        .output()
        .map_err(|source| {
            diagnosed(
                Namespace::Store,
                "nix-unavailable",
                "could not run `nix`",
                Locus::Named("guest build"),
                source.to_string(),
                ExitKind::Software,
            )
        })?;
    if !output.status.success() {
        return Err(diagnosed(
            Namespace::Store,
            "build-failed",
            "the guest could not be built",
            Locus::Named("guest build"),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ExitKind::Software,
        ));
    }
    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if path.is_empty() {
        return Err(diagnosed(
            Namespace::Store,
            "build-empty",
            "the guest build produced no output path",
            Locus::Named("guest build"),
            "`nix build --print-out-paths` printed nothing",
            ExitKind::Software,
        ));
    }
    Ok(path)
}

/// The double-dashed system name the generated flake's attributes are keyed by.
const fn nix_system() -> &'static str {
    // Only the two the product flake resolves. An unsupported architecture reaches the flake and
    // is refused there by name rather than being guessed at here.
    if cfg!(target_arch = "aarch64") {
        "aarch64-linux"
    } else {
        "x86_64-linux"
    }
}

/// Publishes one build record, creating the per-target directory the first time.
fn write_build_record(path: &Path, store_path: &str) -> Result<(), Failure> {
    let fault = |source: std::io::Error| {
        diagnosed(
            Namespace::State,
            "build-record",
            "could not record the build output",
            Locus::File(path.to_path_buf()),
            source.to_string(),
            ExitKind::IoErr,
        )
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fault)?;
    }
    std::fs::write(path, format!("{store_path}\n")).map_err(fault)
}

/// Runs the built runner, which resolves the host-side tokens and execs `viv start --spec`.
fn execute_runner<E: Environment>(
    context: &Context<'_, E>,
    store_path: &str,
    runtime: &Runtime,
    project_id: &str,
    resources: Resources,
) -> Result<(), Failure> {
    // spec/02 and ADR-0019 fix the location: a project's volumes are part of its per-project state
    // at `projects/<project-id>/<target>/volumes/<name>.img`, under the same two components the
    // runtime root mirrors. Anywhere else and `viv volume list`, `viv volume rm`, and `viv destroy`
    // could not find the user's own data, because each of them looks under the project's subtree.
    let volumes = volume_directory(&context.roots, project_id, DEFAULT_TARGET);
    std::fs::create_dir_all(&volumes).map_err(|source| {
        diagnosed(
            Namespace::State,
            "volume-directory",
            "could not create the volume directory",
            Locus::File(volumes.clone()),
            source.to_string(),
            ExitKind::IoErr,
        )
    })?;

    let program = Path::new(store_path)
        .join("bin")
        .join("vivarium-first-microvm");
    let output = Command::new(&program)
        .arg("--workspace")
        .arg(&context.project)
        .arg("--runtime-dir")
        .arg(&runtime.directory)
        .arg("--volume")
        .arg(volumes.join(format!("{DEFAULT_VOLUME}.img")))
        .arg("--store-volume")
        .arg(volumes.join(format!("{STORE_VOLUME}.img")))
        .arg("--uid")
        .arg(config::effective_uid().to_string())
        .arg("--gid")
        .arg(config::effective_gid().to_string())
        .arg("--memory-mib")
        .arg(resources.mem_mib.to_string())
        .arg("--vcpu")
        .arg(resources.vcpu.to_string())
        .arg("--project-id")
        .arg(project_id)
        .arg("--target")
        .arg(DEFAULT_TARGET)
        .output()
        .map_err(|source| {
            diagnosed(
                Namespace::Vm,
                "launcher-unavailable",
                "could not run the guest launcher",
                Locus::File(program.clone()),
                source.to_string(),
                ExitKind::Software,
            )
        })?;

    if !output.status.success() {
        // The launcher's own stderr is the account of which child died; the readiness socket
        // carries a status and nothing more (see docs/explanation/launch-and-supervision.md).
        return Err(diagnosed(
            Namespace::Vm,
            "start-failed",
            "the VM did not come up",
            Locus::Named("guest launch"),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ExitKind::Unavailable,
        )
        .with_hint(format!(
            "`journalctl --user -u {}` carries the supervisor's own diagnostic",
            runtime.unit
        )));
    }
    Ok(())
}

/// Where one project's persistent volumes live (spec/02, ADR-0019).
pub(super) fn volume_directory(
    roots: &config::XdgRoots,
    project_id: &str,
    target: &str,
) -> PathBuf {
    roots
        .state
        .join("projects")
        .join(project_id)
        .join(target)
        .join("volumes")
}

/// The ceilings in force for one launch: declared wins, undeclared resolves from the host.
///
/// spec/10 step 5 puts this moment at ensure-running and spec/17 fixes the arithmetic, both for
/// the same reason: a host-derived ceiling is launch-channel, never a build input (N3, N19), so
/// two hosts building one manifest boot it with different ceilings and the same store path.
fn effective_resources(declared: Option<&config::Resources>) -> Resources {
    Resources {
        mem_mib: declared
            .and_then(|declared| declared.mem_mib)
            .map_or_else(host_mem_mib, u64::from),
        vcpu: declared
            .and_then(|declared| declared.vcpu)
            .map_or_else(host_vcpu, u64::from),
    }
}

/// spec/17: half of host physical memory, rounded down to a whole GiB, clamped to 4-16 GiB.
fn host_mem_mib() -> u64 {
    /// The clamp spec/17 fixes, in MiB.
    const FLOOR: u64 = 4 * 1024;
    const CEILING: u64 = 16 * 1024;

    // `MemTotal` is physical memory in kibibytes, which is the only reading this needs — the
    // available figure is what admission control asks about, and that gate is slice 005's.
    let total_mib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .map(|kibibytes| kibibytes / 1024);
    // A host that will not say how much memory it has gets the floor rather than a guess: the
    // clamp's lower bound is the one value spec/17 already calls safe for a real toolchain.
    let Some(total_mib) = total_mib else {
        return FLOOR;
    };
    ((total_mib / 2) / 1024 * 1024).clamp(FLOOR, CEILING)
}

/// spec/17: the host's CPU count, capped at 8.
fn host_vcpu() -> u64 {
    /// The cap spec/17 fixes.
    const CAP: u64 = 8;

    std::thread::available_parallelism().map_or(1, |count| {
        u64::try_from(count.get()).unwrap_or(CAP).min(CAP)
    })
}

/// The resources a launch put in force, read back from its own specification.
///
/// This is `status`'s reader, never a launch's: spec/01 requires the `resources` object to report
/// what is in force for the running VM, and the launch specification is the one artifact that
/// records it. The fallback covers a VM whose specification cannot be read; it is operational data
/// derived from no running VM, which spec/01's "never `null` while it runs" leaves no better answer
/// to today. Q-015 owns the choice between this floor and admitting `null` for the one case where
/// the record is unreadable; a `failed` state is not the alternative, because this VM is running.
fn declared_resources(runtime: &Runtime) -> Resources {
    let fallback = Resources {
        mem_mib: 4096,
        vcpu: 4,
    };
    let Ok(raw) = std::fs::read_to_string(runtime.launch_spec()) else {
        return fallback;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return fallback;
    };
    let resources = &value["resources"];
    Resources {
        mem_mib: resources["memoryMiB"].as_u64().unwrap_or(fallback.mem_mib),
        vcpu: resources["vcpus"].as_u64().unwrap_or(fallback.vcpu),
    }
}

/// What the project's VM is doing. Read-only, and any reported state exits `0`.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`) or an unusable runtime root.
pub fn status<E: Environment>(context: &Context<'_, E>, global: bool) -> Result<Report, Failure> {
    // `-g` enumerates the registry (spec/01). It has no trial in this slice and is the first
    // thing the appetite would cut, so it is refused by name rather than half-answered.
    if global {
        return Err(diagnosed(
            Namespace::Internal,
            "not-implemented",
            "`viv status --global` is not implemented yet",
            Locus::Named("command surface"),
            "the project-local form is this slice's; the cross-project enumeration is not",
            ExitKind::Software,
        ));
    }

    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let registry = config::registry::read(&context.roots.state)
        .map_err(|error| super::registry_failure(&error))?;
    let binding = config::resolve_binding(None, context.environment, &registry, &context.project);
    let Some(binding) = binding else {
        return Err(super::unbound(context));
    };

    // Resolved, never minted: spec/15 makes persistence the minting commands' alone, which is what
    // keeps spec/14's read-only guarantee literally true.
    let project_id = config::resolve_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;
    let runtime = Runtime::locate(&runtime_root, &project_id, DEFAULT_TARGET);
    let recorded = last_build(&context.roots, &project_id, DEFAULT_TARGET);
    let state = discriminate(&runtime, recorded.is_some());

    let running = matches!(state, State::Running | State::Stopping);
    Ok(Report {
        manifest: Some(binding.manifest),
        state,
        // The one cause this slice can distinguish: `discriminate` reaches `failed` from runtime
        // records whose process is gone, which is spec/10's `crashed`. `boot-timeout` needs the
        // readiness handshake's own verdict and `socket-lost` needs a live control socket, and
        // neither is observable from here — so neither is guessed at.
        reason: matches!(state, State::Failed).then_some("crashed"),
        // Freshness derives from the store output path (N4): the VM is stale when the build now
        // recorded is not the one it was launched from. Both halves are the same kind of path —
        // the runner output — because comparing a runner against anything else answers a different
        // question and always says "drifted". An unknown pairing reads as not stale rather than as
        // drifted, since the flag carries a remedy the user would act on.
        stale: matches!(state, State::Running)
            && running_build(&context.roots, &project_id, DEFAULT_TARGET).is_some_and(|launched| {
                recorded
                    .as_ref()
                    .is_some_and(|current| *current != launched)
            }),
        store_path: recorded,
        uptime_seconds: running.then(|| uptime_seconds(&runtime)).flatten(),
        resources: running.then(|| declared_resources(&runtime)),
    })
}

/// How long the VM has been up, from the age of the record the supervisor writes at boot.
fn uptime_seconds(runtime: &Runtime) -> Option<u64> {
    let metadata = std::fs::metadata(runtime.boot_json()).ok()?;
    let modified = metadata.modified().ok()?;
    modified.elapsed().ok().map(|elapsed| elapsed.as_secs())
}

/// Bring the VM down, stopping at the teardown boundary ADR-0018 fixes.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), or a stop that cannot be confirmed (`69`).
pub fn stop<E: Environment>(
    context: &Context<'_, E>,
    all: bool,
    force: bool,
    timeout: Option<i64>,
) -> Result<Success, Failure> {
    if all {
        return Err(diagnosed(
            Namespace::Internal,
            "not-implemented",
            "`viv stop --all` is not implemented yet",
            Locus::Named("command surface"),
            "the cross-project sweep is not this slice's; the project-local stop is",
            ExitKind::Software,
        ));
    }

    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let registry = config::registry::read(&context.roots.state)
        .map_err(|error| super::registry_failure(&error))?;
    if config::resolve_binding(None, context.environment, &registry, &context.project).is_none() {
        return Err(super::unbound(context));
    }
    let project_id = config::resolve_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;
    let runtime = Runtime::locate(&runtime_root, &project_id, DEFAULT_TARGET);

    // Idempotent (spec/10): nothing running is a `0` no-op. Checked against the discriminator
    // rather than against the unit, so a `failed` VM with dead records still reaches the sweep.
    let has_build = last_build(&context.roots, &project_id, DEFAULT_TARGET).is_some();
    if matches!(
        discriminate(&runtime, has_build),
        State::Absent | State::Built
    ) {
        return Ok(Success::plain(String::new()));
    }

    stop_unit(&runtime, force, timeout)?;

    // The post-condition, asserted in the product rather than only in a trial: after a completed
    // stop the runtime directory holds no entry the allowlisted cleanup is required to remove. The
    // supervisor removes the directory itself, so the honest test is that it is gone, or holds
    // nothing but the startup lock. The lock is excluded because the sweep is required *not* to
    // remove it: it is the per-target mutex (spec/12 step 1) rather than an artifact of the VM, a
    // caller can be holding it across this very teardown, and unlinking it would end the exclusion
    // its pathname provides. Counting it here would make every `viv`-driven stop report a teardown
    // it completed correctly as incomplete.
    //
    // An unreadable directory is not an empty one. Counting a failed inspection as zero would make
    // `stop` claim its post-condition at the one moment it cannot be established, so anything but
    // "gone" or "readable and holding at most the lock" is the unconfirmed teardown spec/10 assigns
    // the unavailable code to.
    let lock = runtime.lock();
    let residue = match std::fs::read_dir(&runtime.directory) {
        Ok(entries) => {
            let mut counted = 0;
            for entry in entries {
                let entry =
                    entry.map_err(|source| unconfirmed_teardown(&runtime.directory, &source))?;
                if entry.path() != lock {
                    counted += 1;
                }
            }
            counted
        }
        // The supervisor removes the directory itself, so absent is the clean outcome.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(source) => return Err(unconfirmed_teardown(&runtime.directory, &source)),
    };
    if residue > 0 {
        return Err(diagnosed(
            Namespace::Vm,
            "teardown-incomplete",
            "the runtime directory still holds entries after a completed stop",
            Locus::File(runtime.directory),
            format!("{residue} entries survived the allowlisted cleanup"),
            ExitKind::Unavailable,
        ));
    }

    Ok(Success::plain(String::new()))
}

/// A teardown whose post-condition could not be established, because the directory would not read.
fn unconfirmed_teardown(directory: &Path, source: &std::io::Error) -> Failure {
    diagnosed(
        Namespace::Vm,
        "teardown-unconfirmed",
        "the runtime directory could not be inspected after a completed stop",
        Locus::File(directory.to_path_buf()),
        source.to_string(),
        ExitKind::Unavailable,
    )
}

/// The escalation ladder of spec/10 "Stopping", expressed against the transient unit.
///
/// Stopping the unit converges every exit route through the supervisor's single cancellation path
/// (ADR-0097), and `KillMode=control-group` makes it a whole-group operation — so the VMM, every
/// per-share daemon, and any launch helper go away together. The ladder's first two rungs are the
/// supervisor's own; what is decided here is how long to wait before the last one.
fn stop_unit(runtime: &Runtime, force: bool, timeout: Option<i64>) -> Result<(), Failure> {
    // The ladder's first two rungs are the supervisor's own: stopping the unit runs its single
    // cancellation path, which signals the guest agent and then the backend. What is decided here
    // is the third — how long to wait before pulling the power — so the stop is issued without
    // waiting and the deadline is enforced here.
    let grace = grace_seconds(force, timeout);
    let mut child = Command::new("systemctl")
        .args(["--user", "--no-block", "stop", &runtime.unit])
        .output()
        .map_err(|source| {
            diagnosed(
                Namespace::Host,
                "systemctl-unavailable",
                "could not run `systemctl --user`",
                Locus::Named("user service manager"),
                source.to_string(),
                ExitKind::Unavailable,
            )
        })?;
    if !child.status.success() {
        let stderr = String::from_utf8_lossy(&child.stderr);
        // A unit that is already gone is the idempotent case, not a failure.
        if stderr.contains("not loaded") || stderr.contains("not found") {
            return Ok(());
        }
        return Err(diagnosed(
            Namespace::Vm,
            "stop-failed",
            "the VM could not be confirmed stopped",
            Locus::Named("guest teardown"),
            stderr.trim().to_owned(),
            ExitKind::Unavailable,
        ));
    }
    child.stderr.clear();

    // Poll rather than block, because the deadline is the point. `None` waits indefinitely, which
    // is what `--timeout -1` asks for.
    let deadline =
        grace.map(|seconds| std::time::Instant::now() + std::time::Duration::from_secs(seconds));
    loop {
        match unit_active_state(&runtime.unit).as_deref() {
            // Gone, or never loaded: the unit's lifetime is over and with it the VM's.
            None | Some("inactive" | "failed") => return Ok(()),
            _ => {}
        }
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // The last rung. `KillMode=control-group` makes this a whole-group operation, so the VMM,
    // every per-share daemon, and any launch helper go away together (ADR-0097, spec/17).
    let killed = Command::new("systemctl")
        .args(["--user", "kill", "--signal=SIGKILL", &runtime.unit])
        .output();
    if killed.is_ok_and(|output| output.status.success()) {
        // Give the group the moment it takes to reap before reporting.
        for _ in 0..50 {
            match unit_active_state(&runtime.unit).as_deref() {
                None | Some("inactive" | "failed") => return Ok(()),
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    }

    // spec/10: a stop that cannot be confirmed even by hard poweroff reports the actual state and
    // exits with the unavailable code rather than pretending success.
    Err(diagnosed(
        Namespace::Vm,
        "stop-unconfirmed",
        "the VM could not be confirmed stopped",
        Locus::Named("guest teardown"),
        format!(
            "`{}` is still {} after a hard poweroff",
            runtime.unit,
            unit_active_state(&runtime.unit).unwrap_or_else(|| "in an unknown state".to_owned())
        ),
        ExitKind::Unavailable,
    ))
}

/// The grace period before a hard poweroff: `--force` is zero, the default is ten seconds, and
/// `-1` waits indefinitely (spec/10).
///
/// A pure function of the two flags, so the ladder's one decision is testable without a VM.
#[must_use]
pub(super) const fn grace_seconds(force: bool, timeout: Option<i64>) -> Option<u64> {
    if force {
        return Some(0);
    }
    match timeout {
        Some(seconds) if seconds < 0 => None,
        Some(seconds) => Some(seconds.unsigned_abs()),
        None => Some(10),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Runtime, State, classify, config, discriminate, effective_resources, grace_seconds,
        host_mem_mib, host_vcpu, ownership_of, vm_is_alive, volume_directory,
    };
    use std::fs;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("vivarium-lifecycle-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).ok();
            Self(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The resting states, each reachable from what the filesystem says and nothing else.
    #[test]
    fn the_resting_states_are_decided_by_records_and_build_output() {
        let scratch = Scratch::new("resting");
        let runtime = Runtime::locate(&scratch.0, "api", "default");
        fs::create_dir_all(&runtime.directory).ok();

        // Neither records nor a build.
        assert_eq!(discriminate(&runtime, false), State::Absent);
        // A build and no records: the resting state a clean stop leaves.
        assert_eq!(discriminate(&runtime, true), State::Built);

        // Records whose process is gone. `stop` removing the records is what makes this `failed`
        // rather than `built`, which is the invariant ADR-0030 names as this design's cost.
        fs::write(runtime.directory.join("vm.pid"), "2147483647\n").ok();
        assert_eq!(discriminate(&runtime, true), State::Failed);
        assert_eq!(discriminate(&runtime, false), State::Failed);
    }

    /// The owning unit's three answers, including the one a boolean cannot carry.
    ///
    /// spec/12 step 4 acts destructively on `false` — it cold-starts or removes records — so the
    /// rows that matter most here are the ones that must NOT be `false`: a manager that could not
    /// be reached and a state name this version does not recognize. Both are `None`, and the caller
    /// turns that into a stated refusal rather than a teardown.
    #[test]
    fn an_unaskable_unit_is_not_a_stopped_one() {
        // Owning a VM: the unit is running it, or is on its way in or out of running it.
        for state in [
            "active",
            "reloading",
            "refreshing",
            "activating",
            "deactivating",
        ] {
            assert_eq!(ownership_of(state), Some(true), "{state} should own a VM");
        }
        // Definitively not owning one. These are the only two answers that may license a repair.
        for state in ["inactive", "failed"] {
            assert_eq!(ownership_of(state), Some(false), "{state} should be dead");
        }
        // No answer at all. An empty value is what an unreachable manager leaves behind, and the
        // last two stand for a state name a future systemd could return.
        for state in ["", "   ", "maintenance", "something-new"] {
            assert_eq!(ownership_of(state), None, "{state:?} should be undecided");
        }
    }

    /// A `vm.pid` that parses but names no process is a broken record, never a live VM.
    ///
    /// The values below are the ones a truncated or partially written record actually produces, and
    /// they are the ones that used to resolve to pid 1 — which always exists, so every one of them
    /// reported the VM as alive. spec/12 step 4 reads that answer to decide between repairing stale
    /// records and refusing `69`, so a record reading alive forever wedges the project rather than
    /// merely misreporting it.
    #[test]
    fn a_pid_record_that_names_no_process_is_not_alive() {
        let scratch = Scratch::new("pid-record");
        let runtime = Runtime::locate(&scratch.0, "api", "default");
        fs::create_dir_all(&runtime.directory).ok();

        for raw in ["0", "0\n", "-1", "", "  ", "not-a-pid"] {
            fs::write(runtime.directory.join("vm.pid"), raw).ok();
            assert!(
                !vm_is_alive(&runtime),
                "a vm.pid of {raw:?} was read as a live VM"
            );
        }

        // The control: this test's own process is unambiguously alive, so the check still says yes
        // when the record names something real. Without this the assertions above would pass just
        // as well against a function that always answered `false`.
        fs::write(
            runtime.directory.join("vm.pid"),
            format!("{}\n", std::process::id()),
        )
        .ok();
        assert!(vm_is_alive(&runtime));
    }

    /// Every row of ADR-0030's discriminator that needs a process or a unit to reach.
    #[test]
    fn records_are_discriminated_by_the_process_and_the_owning_unit() {
        // A live VM: records, a live process, and the unit that owns its lifetime saying so.
        assert_eq!(classify(true, true, true, Some("active")), State::Running);

        // The transitional pair, which the unit alone decides. A live process under a unit on its
        // way out is `stopping`, and one under a unit still coming up is another process's boot.
        assert_eq!(
            classify(true, true, true, Some("deactivating")),
            State::Stopping
        );
        assert_eq!(
            classify(true, true, true, Some("activating")),
            State::Starting
        );
        assert_eq!(
            classify(true, false, true, Some("activating")),
            State::Starting
        );

        // A live pid with no owning unit is the pid-reuse case: records a crashed VM left behind
        // can name a process that is not this VM, so `running` needs the unit's word too.
        assert_eq!(classify(true, true, true, None), State::Failed);
        assert_eq!(classify(true, true, true, Some("inactive")), State::Failed);
        assert_eq!(classify(true, true, true, Some("failed")), State::Failed);

        // Records without a process, and no boot in flight to explain them.
        assert_eq!(classify(true, true, false, Some("inactive")), State::Failed);
        assert_eq!(classify(true, false, false, None), State::Failed);
    }

    /// The window before the supervisor publishes anything: the unit exists, the records do not.
    ///
    /// spec/10 makes `starting` observable to a concurrent `status` racing another process's boot,
    /// and the unit is submitted before the supervisor that writes `boot.json` and `vm.pid` runs —
    /// so the no-records case has to consult the unit before it answers with a resting state.
    #[test]
    fn a_unit_in_transition_outranks_the_resting_states() {
        for has_build in [true, false] {
            assert_eq!(
                classify(false, has_build, false, Some("activating")),
                State::Starting
            );
            assert_eq!(
                classify(false, has_build, false, Some("deactivating")),
                State::Stopping
            );
        }
        // A clean stop removes the records and leaves no active unit, which is the resting state.
        assert_eq!(classify(false, true, false, Some("inactive")), State::Built);
        assert_eq!(classify(false, true, false, None), State::Built);
        assert_eq!(classify(false, false, false, None), State::Absent);
        assert_eq!(classify(false, false, false, Some("failed")), State::Absent);
    }

    /// The gathering half still reads the two files the pure half is told about.
    #[test]
    fn the_records_the_discriminator_reads_are_the_ones_the_supervisor_writes() {
        let scratch = Scratch::new("records");
        let runtime = Runtime::locate(&scratch.0, "api", "default");
        fs::create_dir_all(&runtime.directory).ok();

        // No unit exists for this scratch id, so a live pid can only reach `failed` — which is
        // exactly the pid-reuse row above, reached through the filesystem rather than by argument.
        fs::write(
            runtime.directory.join("vm.pid"),
            format!("{}\n", std::process::id()),
        )
        .ok();
        assert_eq!(discriminate(&runtime, true), State::Failed);

        // Either record is enough to make the directory "has records".
        fs::remove_file(runtime.directory.join("vm.pid")).ok();
        assert_eq!(discriminate(&runtime, true), State::Built);
        fs::write(runtime.directory.join("boot.json"), "{}\n").ok();
        assert_eq!(discriminate(&runtime, true), State::Failed);
    }

    /// Every state has a spelling, and they are the ones spec/01 publishes.
    #[test]
    fn every_state_spells_itself_as_the_specification_does() {
        let rows = [
            (State::Absent, "absent"),
            (State::Built, "built"),
            (State::Starting, "starting"),
            (State::Running, "running"),
            (State::Stopping, "stopping"),
            (State::Failed, "failed"),
        ];
        for (state, spelling) in rows {
            assert_eq!(state.as_str(), spelling);
        }
    }

    /// The ladder's one timing decision, as spec/10 fixes it.
    #[test]
    fn the_grace_period_follows_the_flags() {
        assert_eq!(grace_seconds(false, None), Some(10));
        assert_eq!(grace_seconds(true, None), Some(0));
        assert_eq!(grace_seconds(true, Some(0)), Some(0));
        assert_eq!(grace_seconds(false, Some(30)), Some(30));
        // `-1` waits indefinitely, which is the absence of a deadline rather than a long one.
        assert_eq!(grace_seconds(false, Some(-1)), None);
    }

    /// The unit name is the one the runner hands `systemd-run`, because nothing can ask it.
    #[test]
    fn the_unit_name_matches_the_launcher() {
        let runtime = Runtime::locate(
            std::path::Path::new("/run/user/1000/vivarium"),
            "api",
            "default",
        );
        assert_eq!(runtime.unit, "vivarium-api-default.service");
        assert_eq!(
            runtime.directory,
            std::path::Path::new("/run/user/1000/vivarium/api/default")
        );
    }

    /// spec/17: a declared ceiling always wins, and an undeclared one resolves from the host.
    #[test]
    fn declared_ceilings_win_and_undeclared_ones_come_from_the_host() {
        let both = config::Resources {
            mem_mib: Some(8192),
            vcpu: Some(6),
        };
        let resolved = effective_resources(Some(&both));
        assert_eq!(resolved.mem_mib, 8192);
        assert_eq!(resolved.vcpu, 6);

        // Each knob is independent: declaring one leaves the other to the host.
        let memory_only = config::Resources {
            mem_mib: Some(8192),
            vcpu: None,
        };
        assert_eq!(effective_resources(Some(&memory_only)).vcpu, host_vcpu());
        assert_eq!(effective_resources(None).mem_mib, host_mem_mib());
    }

    /// The host halves stay inside the clamps spec/17 fixes, whatever this host reports.
    #[test]
    fn host_resolved_ceilings_stay_inside_the_specified_clamps() {
        let memory = host_mem_mib();
        assert!(
            (4 * 1024..=16 * 1024).contains(&memory),
            "{memory} MiB is outside spec/17's 4-16 GiB clamp"
        );
        // Rounded down to a whole GiB, so the clamp's ends are reachable and nothing between them
        // is a fraction of a gibibyte.
        assert_eq!(memory % 1024, 0);

        let vcpu = host_vcpu();
        assert!(
            (1..=8).contains(&vcpu),
            "{vcpu} vcpu is outside spec/17's cap"
        );
    }

    /// The volume layout spec/02 and ADR-0019 fix, which the harness reads by the same shape.
    #[test]
    fn volumes_live_under_the_project_subtree() {
        let roots = config::XdgRoots {
            config: PathBuf::from("/c"),
            data: PathBuf::from("/d"),
            state: PathBuf::from("/s"),
            cache: PathBuf::from("/k"),
        };
        assert_eq!(
            volume_directory(&roots, "api", "default"),
            std::path::Path::new("/s/projects/api/default/volumes")
        );
    }
}
