//! The three lifecycle verbs: bring the VM up, say what it is doing, bring it down.
//!
//! What is new here is small on purpose. The supervisor, the confinement policy, and the transient
//! unit already exist and are host-proven; the generated flake already publishes the same runner
//! the diagnostic path executes. So `start` resolves, builds, and executes that
//! runner to render the specification, then re-invokes this binary as `start --spec` — the
//! handoff slice 002 built, now driven from the running installation (ADR-0102). Nothing here
//! supervises anything.
//!
//! The division of labour with `mod.rs` is that this file owns the lifecycle and that one owns the
//! readers. They share the `Context`, the failure vocabulary, and the resolution front half.

use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::grammar::Output;
use super::{Context, Failure, ResolvedForLaunch, Success, diagnosed};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::launch::{
    BootMetadata, GuestSession, LAUNCH_SCHEMA_VERSION, LaunchSpec, MountPlanKind, VersionEnvelope,
    agent_source, mounts, secure_fs,
};
use crate::protocol::CredentialId;
use crate::ui::{Ui, watch};

/// The only target a project has today (spec/15).
pub(super) const DEFAULT_TARGET: &str = "default";

/// How long a reuse decision waits for the agent to answer a `Ping`.
///
/// Short, because this is a question about a VM that is supposed to be up already: if the socket is
/// there and the agent is healthy, one round trip answers it. A longer budget here would turn every
/// stale socket into a pause.
const PING_TIMEOUT: Duration = Duration::from_millis(500);

/// The bounded ask of the ladder's first rung: connect, greet, request the shutdown, read the
/// acknowledgement (spec/10, spec/12).
///
/// Generous for four frames against a healthy agent, and capped at what remains of the
/// operator's grace at the call site, so a short deadline is never eaten whole by the ask.
const SHUTDOWN_ASK_TIMEOUT: Duration = Duration::from_secs(2);

/// The window the backend's power path gets when it is engaged after an acknowledged first rung
/// outlived the grace.
///
/// The supervisor raises the power button, waits its six-second window, destroys the VM and
/// waits two seconds more (the compiled budgets in `src/launch/supervisor.rs`); this figure is
/// those two plus slop. Only
/// a guest that acknowledged an orderly shutdown and then wedged inside it is given this bounded
/// overshoot past the operator's deadline — the alternative is a group kill at the deadline,
/// which skips the supervisor's cleanup and turns every acked-but-slow guest into an unconfirmed
/// teardown where the power path delivers a clean one ten seconds later (spec/10).
const POWER_SIGNAL_WINDOW_SECONDS: i64 = 10;

/// How long a live VM whose agent has not answered yet is given before it is called unavailable.
///
/// This is the boot-timeout arm of spec/12 step 4: the VM's own process is alive, so something is
/// coming up, and the answer is to wait rather than to boot a second one — but bounded, because
/// spec/12 requires a stated reason rather than a hang.
const AGENT_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Which rung of spec/10's ladder ended a stop — the `rung` field of spec/01's stop record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StopRung {
    /// The guest agent acknowledged the shutdown request and the guest powered itself off.
    Agent,
    /// The manager's stop reached the supervisor, whose power signal brought the guest down.
    PowerSignal,
    /// The group was killed when the ladder ran out.
    HardPoweroff,
}

impl StopRung {
    /// The spelling spec/01's `rung` field carries.
    #[must_use]
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::PowerSignal => "power-signal",
            Self::HardPoweroff => "hard-poweroff",
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
    /// The retained generation `current` points at, `None` for a project never built (spec/11).
    pub generation: Option<u64>,
    pub uptime_seconds: Option<u64>,
    pub resources: Option<Resources>,
    pub runtime: RuntimeReadings,
    /// The launch schema a running VM's boot record carries when it is not this binary's own.
    ///
    /// Reported beside `running` rather than folded into a refusal, because `status` is the one
    /// verb that must keep answering when half the tool cannot act: a VM booted by another
    /// vivarium version is running, and saying only `running` would hand the user a truth every
    /// session verb then refuses with `78`.
    pub record_skew: Option<u32>,
}

/// The declared ceiling, never the measured use. spec/01 keeps the two in separate objects so a
/// consumer never has to guess which it is holding.
#[derive(Clone, Copy)]
pub struct Resources {
    pub mem_mib: u64,
    pub vcpu: u64,
}

/// What is measured right now — the `runtime` object, opposite [`Resources`] in spec/01's split.
///
/// Every field is `None` where the reading is honestly unavailable: no delegated memory
/// controller, an agent that cannot answer, a host without PSI, a VM that is not running, or
/// volume state that cannot be read. The disk pair survives a stop because volumes do (N18),
/// and `0` there is a reading — a sandbox whose images occupy nothing — not an absence. Nothing
/// here refuses: `status` is the verb that keeps answering, and spec/14's status rows admit no
/// code for a reading that failed, so a failed reading is a `null` rather than an exit.
#[derive(Clone, Copy, Default)]
pub struct RuntimeReadings {
    pub mem_used_bytes: Option<u64>,
    pub disk_allocated_bytes: Option<u64>,
    pub disk_virtual_bytes: Option<u64>,
    pub sessions: Option<u64>,
    pub pressure_some_avg60: Option<f64>,
}

/// Where one target's runtime files live, and the unit that owns them.
#[derive(Clone)]
pub(super) struct Runtime {
    pub directory: PathBuf,
    pub unit: String,
}

impl Runtime {
    pub(super) fn locate(
        runtime_root: &Path,
        sandbox_id: &str,
        target: &str,
    ) -> Result<Self, Failure> {
        let runtime = Self {
            directory: runtime_root.join(sandbox_id).join(target),
            // The same name `nix/runner.sh` hands `systemd-run --unit`, because `stop` and
            // `status` have to name the unit `start` created and neither can ask it.
            unit: format!("vivarium-{sandbox_id}-{target}.service"),
        };
        let control = runtime.control_socket();
        if control.as_os_str().as_bytes().len() >= 108 {
            return Err(diagnosed(
                Namespace::Host,
                "runtime-socket-path-too-long",
                "the sandbox control socket does not fit in a Unix socket path",
                Locus::File(control.clone()),
                format!(
                    concat!(
                        "the rendered path is {} bytes; Linux `sun_path` permits at most ",
                        "107 path bytes"
                    ),
                    control.as_os_str().as_bytes().len()
                ),
                ExitKind::Config,
            )
            .with_hint("use a shorter `XDG_RUNTIME_DIR` or a shorter manifest name"));
        }
        Ok(runtime)
    }

    /// [`Runtime::locate`] without the socket-length refusal, for enumeration only.
    ///
    /// The 108-byte check exists to refuse before a launch binds the socket; an enumeration
    /// never binds, and refusing the whole fleet because one manifest name renders an over-long
    /// path would un-enumerate a sandbox that exists. Such a row simply cannot be asked for its
    /// session count — a `null`, not a fault.
    pub(super) fn for_report(runtime_root: &Path, sandbox_id: &str, target: &str) -> Self {
        Self {
            directory: runtime_root.join(sandbox_id).join(target),
            unit: format!("vivarium-{sandbox_id}-{target}.service"),
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

    /// The supervisor-owned console capture `--attach` follows (spec/16 fixes the path).
    pub(super) fn console_log(&self) -> PathBuf {
        self.directory.join("console.log")
    }

    /// The per-target startup lock (spec/12 step 1).
    ///
    /// Spelled here and declared in the launch specification's runtime paths, because the
    /// supervisor's teardown sweep has to recognize it. The two spellings are compared by the
    /// launcher's own validation, which requires every runtime path to be an exact child of this
    /// directory.
    pub(super) fn lock(&self) -> PathBuf {
        self.directory.join("lock")
    }
}

/// The per-target `flock`, held across a reuse-or-boot decision and dropped before the session.
///
/// ADR-0053's surviving lock order starts here, before the not-yet-implemented Nix profile lock.
/// It is the reason a second `viv exec` racing a cold start waits for the boot rather than starting
/// a second one.
pub(super) struct TargetLock(File);

impl TargetLock {
    pub(super) fn acquire(runtime: &Runtime) -> Result<Self, Failure> {
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

/// The build the currently running VM was launched from, written just before the launcher runs.
///
/// Under the state root — what a run produces — beside the generation profile it may diverge
/// from, and not in the runtime directory, deliberately: the supervisor's cleanup sweep removes
/// only its own allowlist and aborts on anything else, so a record written beside the sockets
/// would make every teardown fail with an unknown-artifact refusal. Slice 032 moved it here from
/// the data root, which spec/02 reserves for pinned inputs.
fn running_build_path(roots: &config::XdgRoots, sandbox_id: &str, target: &str) -> PathBuf {
    roots
        .state
        .join("projects")
        .join(sandbox_id)
        .join(target)
        .join("running-build")
}

/// What this project last built: the current generation of its spec/11 profile.
///
/// A reader of the profile rather than a second record — slice 032 retired the `last-build` text
/// file so exactly one answer to "what did this project last build" exists, and made that answer
/// a registered garbage-collector root. The collected-output guard lives in the profile reader:
/// a pinned path can still be gone on a store wiped by hand, and reporting `built` for one would
/// be a state that cannot be acted on.
pub(super) fn last_build(
    roots: &config::XdgRoots,
    sandbox_id: &str,
    target: &str,
) -> Option<String> {
    config::generations::current_store_path(&generation_paths(roots, sandbox_id, target))
}

/// The one spelling of where a project target's generation profile lives.
pub(super) fn generation_paths(
    roots: &config::XdgRoots,
    sandbox_id: &str,
    target: &str,
) -> config::generations::GenerationPaths {
    config::generations::GenerationPaths::new(&roots.state, sandbox_id, target)
}

/// The same read for the running VM's own build: freshness for `status`, and the boot record
/// the prune guard reads (ADR-0085) — `boot.json` carries no store path.
pub(super) fn running_build(
    roots: &config::XdgRoots,
    sandbox_id: &str,
    target: &str,
) -> Option<String> {
    read_build_record(&running_build_path(roots, sandbox_id, target))
}

/// Reads a recorded store path, and only if that output still exists — the same guard the
/// profile reader applies, for the same reason.
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
    recorded_pid_liveness(runtime) == Some(true)
}

/// What the recorded pid actually establishes, with no-evidence kept apart from dead.
///
/// `Some(false)` is the one positive death: a pid that parsed and whose process the kernel says
/// is gone. A missing, unreadable, malformed, or non-positive record is `None` — no evidence
/// either way — because a live guest does not stop existing when a file goes unreadable, and
/// [`vm_presence`]'s destructive consumers must not read an absence of evidence as a death.
fn recorded_pid_liveness(runtime: &Runtime) -> Option<bool> {
    let raw = std::fs::read_to_string(runtime.vm_pid()).ok()?;
    let pid = raw.trim().parse::<i32>().ok()?;
    // A record that parses but names no process is a broken record, not a live VM, and it is
    // rejected here rather than resolved to something. Substituting `Pid::INIT` for it — which is
    // what `unwrap_or` did — asked the kernel about pid 1, which always exists and is never ours,
    // so a `vm.pid` holding `0` reported every VM as alive forever. The sign is filtered before
    // `Pid::from_raw` rather than by it, because that constructor debug-asserts on a negative.
    let pid = (pid > 0)
        .then(|| rustix::process::Pid::from_raw(pid))
        .flatten()?;
    // Signal 0 asks the kernel whether the process exists and whether we may signal it, and sends
    // nothing. `PERM` means it exists and belongs to someone else, which for a per-user runtime
    // directory should not happen — but it is still alive, so it counts as alive. `SRCH` is the
    // one errno the kernel gives the meaning "no such process", so it alone is a positive death;
    // any other answer is a failed probe, which establishes nothing.
    match rustix::process::test_kill_process(pid) {
        Ok(()) | Err(rustix::io::Errno::PERM) => Some(true),
        Err(rustix::io::Errno::SRCH) => Some(false),
        Err(_) => None,
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
pub(super) enum Presence {
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
/// A pid whose process is positively gone is the one answer the pid alone can give, and it is
/// `Dead`; a pid record that yields no evidence defers to the unit, and two non-answers together
/// are `Indeterminate` — never `Dead`, because destructive consumers gate on that word.
pub(super) fn vm_presence(runtime: &Runtime) -> Presence {
    let pid = recorded_pid_liveness(runtime);
    if pid == Some(false) {
        return Presence::Dead;
    }
    presence_of(pid, unit_owns_a_vm(&runtime.unit))
}

/// The presence judgment as a total function of the two facts, testable without a manager.
pub(super) const fn presence_of(pid_alive: Option<bool>, unit_owns: Option<bool>) -> Presence {
    match (pid_alive, unit_owns) {
        // Positive death from either side wins: a recorded process the kernel says is gone (the
        // pid is the VM, whatever the unit says), or a manager that positively disowns the
        // records (a live pid beside that answer is a reused pid).
        (Some(false), _) | (_, Some(false)) => Presence::Dead,
        (_, Some(true)) => Presence::Live,
        // No positive answer from either side. `(None, None)` — no pid evidence and an
        // unaskable manager — must not read as dead: it is exactly the state a live guest is
        // in when a record went unreadable at the same moment the manager did.
        (_, None) => Presence::Indeterminate,
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

/// The unit's measured memory and its cgroup path, from one `systemctl show` round trip.
///
/// spec/17 fixes what the figure is: the scope's own current memory charge — the monitor plus
/// every filesystem daemon — from the accounting `MemoryAccounting=yes` turns on. The manager is
/// asked rather than `/sys/fs/cgroup` walked because unit-name escaping and slice placement are
/// the manager's own, and `ControlGroup` is its answer to where the scope lives.
pub(super) fn unit_memory_and_cgroup(unit: &str) -> (Option<u64>, Option<String>) {
    let Ok(output) = Command::new("systemctl")
        .args([
            "--user",
            "show",
            "-p",
            "MemoryCurrent",
            "-p",
            "ControlGroup",
            unit,
        ])
        .output()
    else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }
    parse_unit_memory_show(&String::from_utf8_lossy(&output.stdout))
}

/// The property-line half of [`unit_memory_and_cgroup`], as a total function of what `show` said.
///
/// Key-prefixed lines rather than `--value`, because with two properties the value form leaves
/// line order as the only binding between number and name.
fn parse_unit_memory_show(shown: &str) -> (Option<u64>, Option<String>) {
    let mut memory = None;
    let mut cgroup = None;
    for line in shown.lines() {
        if let Some(value) = line.strip_prefix("MemoryCurrent=") {
            memory = parse_memory_current(value);
        } else if let Some(value) = line.strip_prefix("ControlGroup=") {
            let value = value.trim();
            if !value.is_empty() {
                cgroup = Some(value.to_owned());
            }
        }
    }
    (memory, cgroup)
}

/// One `MemoryCurrent` value, with every unavailable spelling collapsed to `None`.
///
/// spec/17 requires unavailable rather than zero where the controller is not delegated, and the
/// manager has three spellings for unavailable: an empty value, `[not set]`, and the `u64`
/// ceiling it uses as infinity.
fn parse_memory_current(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || value == "[not set]" {
        return None;
    }
    let parsed = value.parse::<u64>().ok()?;
    (parsed != u64::MAX).then_some(parsed)
}

/// The `some avg60` share of a pressure-stall file, host-wide and per-scope alike.
fn parse_pressure_some_avg60(pressure: &str) -> Option<f64> {
    pressure
        .lines()
        .find(|line| line.starts_with("some"))?
        .split_whitespace()
        .find_map(|token| token.strip_prefix("avg60="))?
        .parse::<f64>()
        .ok()
}

/// The scope's own memory-pressure reading, from the cgroup path the manager named.
pub(super) fn scope_pressure_some_avg60(cgroup: &str) -> Option<f64> {
    let path = format!("/sys/fs/cgroup{cgroup}/memory.pressure");
    parse_pressure_some_avg60(&std::fs::read_to_string(path).ok()?)
}

/// The host's own memory-pressure reading; absent where the kernel does not expose PSI.
pub(super) fn host_pressure_some_avg60() -> Option<f64> {
    parse_pressure_some_avg60(&std::fs::read_to_string("/proc/pressure/memory").ok()?)
}

/// ADR-0030's discriminator, as a pure function of what the runtime directory and the store say.
///
/// The record-teardown invariant is what makes it work: a clean `stop` removes the runtime records,
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

/// The hard preflight subset spec/10 step 2 requires.
///
/// Deliberately not the whole `viv doctor` catalog: the hard subset decides whether a boot can be
/// attempted at all, and refuses before any build so a host that cannot hold a VM never pays for
/// one. Admission — spec/10 step 3, owned by spec/17 — is [`admission`]'s separate gate, run by
/// `start` right after this one: preflight asks whether this host can run a VM at all, admission
/// whether it can run another one right now, and spec/10 keeps the two apart by name.
fn preflight<E: Environment>(context: &Context<'_, E>) -> Result<PathBuf, Failure> {
    // The hard subset of the shared probe catalog, in catalog order — the same probes, the same
    // messages, the same codes `viv doctor` reports, so the guard and the report cannot drift
    // (spec/13, ADR-0023). The probe id is the diagnostic id (spec/01).
    let inputs = crate::doctor::Inputs {
        environment: context.environment,
        roots: &context.roots,
        project: None,
        // The launch path reached here only by resolving, so ownership was unambiguous by
        // construction; the two probes this feeds are soft and outside the hard subset anyway.
        ownership_ambiguity: None,
        online: false,
        runtime_root: None,
    };
    if let Some(finding) = crate::doctor::first_hard_failure(&inputs) {
        let mut failure = diagnosed(
            Namespace::Host,
            finding.probe.id,
            finding.message,
            Locus::Named("host preflight"),
            "a launch needs every hard check in the doctor catalog, and this one refused \
            before any side effect",
            finding.probe.code.unwrap_or(ExitKind::Software),
        );
        if let Some(hint) = finding.hint {
            failure = failure.with_hint(hint);
        }
        return Err(failure);
    }

    // The value the caller needs from the fact `runtime-dir-usable` just proved.
    config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))
}

/// spec/17's admission table, total over the readings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Admission {
    /// Available host memory is below the reserve: refuse before any build (N23).
    Refuse,
    /// The fleet's measured use plus the reserve exceeds what is available: warn and proceed,
    /// because the user decides which sandbox matters.
    Warn,
    /// Nothing to say.
    Proceed,
}

/// The decision itself, as a total function of the two readings [`admission`] gathers.
///
/// Separated so every row of spec/17's table is testable without a fleet, a systemd user
/// manager, or a crafted `/proc/meminfo`. A reading that does not exist decides nothing: an
/// absent `available` proceeds (the same honest skip the `host-memory-headroom` probe takes),
/// and an absent fleet term cannot trip the warn tier — host-level readings still reached the
/// decision, which is spec/17's degraded mode, never a skipped check.
pub(super) const fn admit(available: Option<u64>, fleet_used: Option<u64>) -> Admission {
    let Some(available) = available else {
        return Admission::Proceed;
    };
    if available < crate::doctor::MEMORY_RESERVE_BYTES {
        return Admission::Refuse;
    }
    // Spelled as a subtraction so the comparison is exact at every magnitude: `used + reserve`
    // can overflow where `available - reserve` cannot, because the refuse arm above already
    // guaranteed `available >= reserve`.
    match fleet_used {
        Some(used) if used > available - crate::doctor::MEMORY_RESERVE_BYTES => Admission::Warn,
        _ => Admission::Proceed,
    }
}

/// spec/17's warning, minus the `viv trim` line — the reclaim verb does not exist yet and is
/// withheld rather than printed dead (slice 028 owns it).
///
/// A partial fleet sum is printed as the lower bound it is, and an unknown host total drops its
/// clause rather than fabricating a figure.
fn admission_warning(
    fleet: &super::fleet::FleetUse,
    total: Option<u64>,
    sandbox_id: &str,
) -> String {
    let used = fleet.measured_bytes.unwrap_or(0);
    // A partial measurement and an incomplete scan are the same honesty problem: the figure in
    // hand is a floor, and printing it bare would present it as the fleet.
    let at_least = if fleet.measured < fleet.running || !fleet.complete {
        "at least "
    } else {
        ""
    };
    let total = total.map_or_else(String::new, |total| {
        format!("{} ", super::render::bytes(total))
    });
    let (verb, plural) = if fleet.running == 1 {
        ("is", "")
    } else {
        ("are", "s")
    };
    format!(
        concat!(
            "warning: {count} project VM{plural} {verb} running and using {at_least}{used} ",
            "of {total}host memory.\n",
            "         Starting `{sandbox}` may push the host into swap.\n",
            "         viv status -g     see what is running\n",
            "         viv stop          stop a project you are done with"
        ),
        count = fleet.running,
        plural = plural,
        verb = verb,
        at_least = at_least,
        used = super::render::bytes(used),
        total = total,
        sandbox = sandbox_id,
    )
}

/// spec/10 step 3: the admission gate, between preflight and anything that creates state (N23).
///
/// Refusal comes from the host reading alone, before the fleet is even enumerated, so a start
/// the host cannot serve touches nothing — not the index, not the target lock. The fleet term
/// then consumes slice 025's readers; an enumeration that fails degrades to an unavailable
/// fleet term rather than minting a failure mode for a launch whose resolution already read the
/// same index.
///
/// # Errors
///
/// Returns [`Failure`] at `69` (`host.memory-reserve`) when available host memory is below the
/// admission reserve.
fn admission<E: Environment>(
    context: &Context<'_, E>,
    runtime_root: &Path,
    sandbox_id: &str,
) -> Result<(), Failure> {
    let (available, total) = super::fleet::host_memory();
    if admit(available, None) == Admission::Refuse {
        // `available` is present by construction: `admit` cannot refuse an absent reading.
        let available = available.unwrap_or_default();
        return Err(diagnosed(
            Namespace::Host,
            "memory-reserve",
            format!(
                "{} of host memory is available, below the {} admission reserve",
                super::render::bytes(available),
                super::render::bytes(crate::doctor::MEMORY_RESERVE_BYTES),
            ),
            Locus::Named("admission"),
            "spec/17's admission check refuses before any build or boot, so a host below \
            its reserve never pays for a VM it cannot serve",
            ExitKind::Unavailable,
        )
        .with_hint(
            "`viv status -g` shows what is running; `viv stop` a project you are done with",
        ));
    }
    let fleet = super::fleet::measured_use(context, runtime_root).ok();
    let fleet_used = fleet.as_ref().and_then(|fleet| fleet.measured_bytes);
    if let (Admission::Warn, Some(fleet)) = (admit(available, fleet_used), fleet) {
        context
            .ui
            .warn(&admission_warning(&fleet, total, sandbox_id));
    }
    Ok(())
}

/// What a completed start pipeline did, so no caller has to parse the notes to find out.
///
/// The two faces read it differently: the detached form turns both arms into its `Success`,
/// while `--attach` streams only after [`StartOutcome::Booted`] — the launch-time stream
/// attaches to a boot this command performed, never to a VM that was already up (that reach is
/// the slice's recorded out-of-scope).
pub(super) enum StartOutcome {
    /// One of spec/10's short-circuits: nothing was booted, and the note says why.
    AlreadyRunning {
        /// The stderr note the detached form delivers verbatim.
        notes: String,
    },
    /// A boot this command performed, concluded by the supervisor's readiness answer. The
    /// runtime it booted travels through `on_boot`, which fired before the launch.
    Booted {
        /// The ceilings the VM was launched with.
        resources: Resources,
    },
}

/// Bring the project's VM up and return once it is running (spec/10).
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), an unmet preflight, a refused admission
/// (`69`), a content defect in the merge (`65`), a build that fails, or a launcher that does not
/// come up.
pub fn start<E: Environment>(
    context: &Context<'_, E>,
    rebuild: bool,
    no_rebuild: bool,
    generation: Option<u64>,
) -> Result<Success, Failure> {
    match perform_start(context, rebuild, no_rebuild, generation, None)? {
        StartOutcome::AlreadyRunning { notes } => Ok(Success {
            stdout: String::new(),
            notes,
        }),
        StartOutcome::Booted { resources, .. } => {
            context.ui.outro(&format!(
                "running · {} MiB · {} vcpu",
                resources.mem_mib, resources.vcpu
            ));
            Ok(Success::plain(String::new()))
        }
    }
}

/// The start pipeline itself, shared by the detached and attached forms (spec/10 steps 1-5).
///
/// `on_boot` fires exactly when a boot is certain — after every short-circuit and the
/// replace-stop, before the launch — with the runtime whose console the attached form follows.
///
/// # Errors
///
/// The detached form's: [`start`] documents the codes.
pub(super) fn perform_start<E: Environment>(
    context: &Context<'_, E>,
    rebuild: bool,
    no_rebuild: bool,
    generation: Option<u64>,
    on_boot: Option<&dyn Fn(&Runtime)>,
) -> Result<StartOutcome, Failure> {
    // spec/10 step 1: the bound manifest is resolved before anything else, so an unbound project
    // answers `78` on every host — including one that would fail preflight — and so nothing is
    // written for a project that never reaches step 5.
    let resolved = super::resolve_manifest_for_launch(context)?;
    debug_assert!(
        resolved
            .matched_workspace
            .as_ref()
            .is_some_and(|workspace| context.project.starts_with(workspace))
    );
    // The manifest's own table, kept for the one path that cannot read the merged
    // one: `--no-rebuild`
    // evaluates nothing by definition (spec/10), so a piece's `vivarium.resources` is not available
    // to it. The evaluating path below overrides this with the merged value spec/04 makes
    // authoritative.
    let leaf_resources = resolved.resources;
    // Cloned before `resolved` is consumed by the evaluating path: the launch below relays which
    // rows came from `[[workspaces]]`, and only this side ever knew.
    let workspace_paths = resolved.workspace_paths.clone();
    // Filled by the evaluating path below, and left empty by `--no-rebuild`.
    let mut merged = None;

    // Step 2.
    let runtime_root = preflight(context)?;
    // Step 3: admission, before anything is created — a refusal here leaves nothing behind, not
    // even the target lock (spec/10, spec/17, N23).
    admission(context, &runtime_root, &resolved.selected.name)?;

    let sandbox_id = resolved.selected.name.clone();
    let runtime = Runtime::locate(&runtime_root, &sandbox_id, DEFAULT_TARGET)?;
    // The same per-target lock a session takes (spec/12 step 1), because the two verbs perform the
    // same routine and a lock only one of them respects would guard nothing. Held to the end of
    // this function, which is the end of the boot.
    let _lock = TargetLock::acquire(&runtime)?;
    // The gutter opens once the boot is really this project's to run: target lock held.
    // Every return below either reaches the outro or carries a note, and a note closes an open
    // gutter itself — so the frame never ends mid-air.
    context.ui.intro(&sandbox_id);
    let running = matches!(
        discriminate(
            &runtime,
            last_build(&context.roots, &sandbox_id, DEFAULT_TARGET).is_some()
        ),
        State::Running
    );

    // Step 4.
    let store_path = if let Some(number) = generation {
        retained_boot(&context.roots, &sandbox_id, number, running)?
    } else if no_rebuild {
        // The fast path spec/10 fixes: no evaluation at all, boot the last build as it stands. A
        // live VM short-circuits here because there is nothing to compare it against — asking for
        // no evaluation is asking not to learn whether it drifted.
        if running {
            return Ok(StartOutcome::AlreadyRunning {
                notes: format!("`{sandbox_id}` is already running\n"),
            });
        }
        last_build(&context.roots, &sandbox_id, DEFAULT_TARGET).ok_or_else(|| {
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
        let (built, evaluated) = evaluate_and_build(context, &sandbox_id, resolved)?;
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
        let launched = running_build(&context.roots, &sandbox_id, DEFAULT_TARGET);
        let stale = launched.is_some_and(|launched| launched != store_path);
        return Ok(StartOutcome::AlreadyRunning {
            notes: if stale {
                format!(
                    "`{sandbox_id}` is running an older build and was left alone\n\
                    the new build is ready; `viv start --rebuild` replaces the \
                    running VM with it\n"
                )
            } else {
                format!("`{sandbox_id}` is already running\n")
            },
        });
    }

    // A rebuild replaces a running VM rather than leaving it (spec/10). Stopping first is what
    // makes "replace" true; without it the launcher would meet a runtime directory that is in use.
    // The failure is propagated rather than dropped: a stop that did not happen leaves the old VM
    // alive, and continuing would overwrite its freshness record with a build it was not launched
    // from — making a still-stale VM report fresh.
    if rebuild {
        stop_unit(&runtime, &sandbox_id, false, None, context.ui)?;
    }

    // The boot is certain from here: the attached form's follower starts on this runtime's
    // console capture before the launch establishes it, so the stream misses nothing.
    if let Some(on_boot) = on_boot {
        on_boot(&runtime);
    }

    // Step 5, ensure running. The selected manifest name keys every artifact below.
    let resources = effective_resources(merged.or(leaf_resources).as_ref());
    launch(
        context,
        &sandbox_id,
        &runtime,
        &store_path,
        resources,
        &workspace_paths,
    )?;

    Ok(StartOutcome::Booted { resources })
}

/// The retained-boot source `--generation <n>` selects (spec/11): no evaluation, one named
/// generation.
///
/// A running VM refuses rather than short-circuits, because unlike `--no-rebuild` the user named
/// a build that may not be the one running, and a `start` that quietly kept the other would
/// report success for something it did not do. `75`: clearable in one step.
fn retained_boot(
    roots: &config::XdgRoots,
    sandbox_id: &str,
    number: u64,
    running: bool,
) -> Result<String, Failure> {
    if running {
        return Err(diagnosed(
            Namespace::Vm,
            "generation-swap",
            "this project's VM is already running",
            Locus::Named("build history"),
            "booting a named generation replaces the VM, and `start` never \
            replaces a running one",
            ExitKind::TempFail,
        )
        .with_hint(format!(
            "`viv stop` first, then `viv start --generation {number}`"
        )));
    }
    let paths = generation_paths(roots, sandbox_id, DEFAULT_TARGET);
    match fs::read_link(paths.link(number)) {
        Err(_) => Err(diagnosed(
            Namespace::State,
            "no-build",
            format!("generation {number} is not retained"),
            Locus::Named("build history"),
            "`--generation` boots a retained build, and no generation \
            with this number exists for this project",
            ExitKind::DataErr,
        )
        .with_hint("`viv generations list` names what is retained")),
        // spec/11: a missing pin is a legible error, not a silent rebuild.
        Ok(target) if !target.exists() => Err(diagnosed(
            Namespace::State,
            "no-build",
            format!("generation {number}'s build output is gone from the store"),
            Locus::File(target),
            "the recorded output was collected out from under its root, \
            which no ordinary `nix-collect-garbage` does",
            ExitKind::DataErr,
        )
        .with_hint("run `viv start` to evaluate and build a fresh generation")),
        Ok(target) => Ok(target.to_string_lossy().into_owned()),
    }
}

/// spec/10 step 4: evaluate the merged configuration and build the runner it publishes.
///
/// Shared rather than repeated, because `viv start` and a session that must cold-start are the same
/// step of the same routine, and a second spelling of it would be a second product.
fn evaluate_and_build<E: Environment>(
    context: &Context<'_, E>,
    sandbox_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<(String, Option<config::Resources>), Failure> {
    let composition = config::volumes::Composition::of(&resolved.manifest);
    let evaluated = super::evaluate_resolved_for_launch(context, sandbox_id, resolved)?;
    let built = build_runner(&evaluated.flake_directory, context.ui)?;
    // The result is recorded as a generation (spec/10 step 4): a numbered symlink registered as
    // a garbage-collector root, with the record and the lock in force retained beside it. Under
    // the per-target lock this function already runs beneath, which is what makes the number
    // allocation safe. An output the current generation already pins is not appended again — a
    // build's freshness key is its store path (N4), so an unchanged evaluation is the same build,
    // and a history entry per `viv start` of it would be rows that all name one thing.
    let paths = generation_paths(&context.roots, sandbox_id, DEFAULT_TARGET);
    if config::generations::current_store_path(&paths).as_deref() != Some(built.as_str()) {
        let record = config::generations::GenerationRecord {
            store_path: built.clone(),
            // Filled by `append` from the published snapshot, so the digest and the retained
            // bytes cannot disagree.
            lock_digest: String::new(),
            manifest: sandbox_id.to_owned(),
            backend: crate::launch::BACKEND.to_owned(),
            built_at: config::generations::rfc3339_utc(epoch_now()),
        };
        config::generations::append(
            &paths,
            &record,
            &evaluated.lock_snapshot,
            lock_digest,
            register_root,
        )
        .map_err(|error| super::registry_failure(&error))?;
    }
    // Written on this path only. `--no-rebuild` evaluates nothing, so it has no provenance to
    // record and must leave the previous answer standing rather than publish an empty one — an
    // empty record would orphan every piece-declared volume the moment someone skipped a rebuild.
    // Under the per-target lock this function already runs beneath (ADR-0053).
    config::volumes::write(
        &context.roots.state,
        sandbox_id,
        DEFAULT_TARGET,
        &config::volumes::Record {
            composition,
            volumes: evaluated.volumes,
        },
    )
    .map_err(|error| super::registry_failure(&error))?;
    Ok((built, evaluated.resources))
}

/// spec/10 step 5: record what this boot is from, then launch and wait for the guest to answer.
///
/// The record is written first, so a `status` racing this boot never reports a VM as fresh against
/// a build it was not launched from.
fn launch<E: Environment>(
    context: &Context<'_, E>,
    sandbox_id: &str,
    runtime: &Runtime,
    store_path: &str,
    resources: Resources,
    workspace_paths: &[PathBuf],
) -> Result<(), Failure> {
    // Before the record: a refused boot must not leave `running-build` naming a build that never
    // ran, which `status` would then read as this VM's provenance.
    require_current_contract(store_path)?;
    write_build_record(
        &running_build_path(&context.roots, sandbox_id, DEFAULT_TARGET),
        store_path,
    )?;
    execute_runner(
        context,
        store_path,
        runtime,
        sandbox_id,
        resources,
        workspace_paths,
    )
}

/// spec/10's pre-boot refusal: the selected build must speak this binary's launch contract.
///
/// A read of the built output, never a compile-time constant: a freshly built tree matches by
/// construction — this binary wrote the tree its build came from — but `--no-rebuild` and an old
/// generation keep old artifacts reachable on purpose, and a rolled-back `viv` can meet a newer
/// build. This is the check that still earns its place with the `vivarium` input gone
/// (ADR-0102), and the runner's own argv guard stays behind it as the belt for a hand-invoked
/// launcher.
fn require_current_contract(store_path: &str) -> Result<(), Failure> {
    let path = Path::new(store_path)
        .join("share")
        .join("vivarium")
        .join("launch-contract-schema");
    let refuse = |why: String| {
        diagnosed(
            Namespace::Vm,
            "launch-contract-skew",
            "the selected build does not speak this binary's launch contract",
            Locus::File(path.clone()),
            why,
            ExitKind::Config,
        )
        .with_hint(
            "`viv start --rebuild` rebuilds with this version, or run the vivarium \
            generation this build was made by",
        )
    };
    match fs::read_to_string(&path) {
        Ok(raw) => match raw.trim().parse::<u32>() {
            Ok(theirs) if theirs == LAUNCH_SCHEMA_VERSION => Ok(()),
            Ok(theirs) => Err(refuse(format!(
                "the build speaks launch contract schema {theirs} and this viv speaks \
                {LAUNCH_SCHEMA_VERSION}"
            ))),
            Err(_) => Err(refuse(format!(
                "the published contract schema is not a number, so it cannot be compared \
                against the {LAUNCH_SCHEMA_VERSION} this viv speaks"
            ))),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(refuse(format!(
            "the build predates the launch-contract publication and cannot name its schema; \
            this viv speaks {LAUNCH_SCHEMA_VERSION}"
        ))),
        Err(error) => Err(refuse(format!(
            "the build's contract schema could not be read: {error}"
        ))),
    }
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
    debug_assert!(
        resolved
            .matched_workspace
            .as_ref()
            .is_some_and(|workspace| context.project.starts_with(workspace))
    );
    let workspace_paths = resolved.workspace_paths.clone();
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let sandbox_id = resolved.selected.name.clone();
    let runtime = Runtime::locate(&runtime_root, &sandbox_id, DEFAULT_TARGET)?;

    // Step 1. Everything from here to the end of the boot is exclusive per target.
    let _lock = TargetLock::acquire(&runtime)?;

    // Steps 2 and 3: when the socket is there, the agent answered for this boot, and the record
    // names this project, nothing is preflighted, evaluated, built, or booted.
    let boot = if let Some(boot) =
        reusable(&runtime, &sandbox_id, &workspace_paths, context.ui).await?
    {
        boot
    } else {
        // Step 5, which is `viv start`'s own steps 2, 4, and 5 — the same preflight subset, the
        // same evaluation, the same launcher. The preflight runs only here, because spec/12 step 3
        // says a reused VM skips it: the host already held one.
        preflight(context)?;
        let (store_path, merged) = evaluate_and_build(context, &sandbox_id, resolved)?;
        launch(
            context,
            &sandbox_id,
            &runtime,
            &store_path,
            effective_resources(merged.as_ref()),
            &workspace_paths,
        )?;
        // The launcher returns only once the supervisor has awaited the guest agent's own readiness
        // handshake, so spec/12 step 5's "wait for readiness before releasing the lock" holds by
        // construction rather than by a second wait here. A skew here would mean the supervisor
        // this installation just ran wrote another version's record, which is the same broken
        // outcome as no record at all — the boot is unusable either way.
        match read_boot_record(&runtime) {
            BootRecord::Ready(boot) => boot,
            BootRecord::Skewed(theirs) => return Err(boot_record_skew(&runtime, theirs)),
            BootRecord::Absent | BootRecord::Unreadable => {
                return Err(diagnosed(
                    Namespace::Vm,
                    "boot-record-unreadable",
                    "the VM started but left no readable boot record",
                    Locus::File(runtime.boot_json()),
                    "a session is authorized by comparing the agent's answer against this record",
                    ExitKind::Unavailable,
                ));
            }
        }
    };
    prepared(&runtime, boot, &context.project)
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
    sandbox_id: &str,
    workspace_paths: &[PathBuf],
    ui: &Ui,
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
    let boot = match read_boot_record(runtime) {
        BootRecord::Ready(boot) => boot,
        // A record another version wrote is stale by construction (ADR-0102): shed for a dead
        // process like any stale record, and refused with both numbers named for a live one,
        // because acting on a generation this binary does not speak is not a repair.
        BootRecord::Skewed(theirs) => {
            return match vm_presence(runtime) {
                Presence::Dead => stale(runtime, ui).map(|()| None),
                Presence::Live => Err(boot_record_skew(runtime, theirs)),
                Presence::Indeterminate => Err(undecided(
                    runtime,
                    "its boot record was written by another vivarium version",
                )),
            };
        }
        // A socket with no readable record behind it cannot authorize anything: the agent's answer
        // is checked against this file, so without it there is no way to know whose agent replied.
        // Step 4 still decides what to do about it — records are removed for a dead process, and a
        // live one is reported unavailable rather than stopped, because a VM that is running is not
        // a stale record and tearing it down is not a repair.
        BootRecord::Absent | BootRecord::Unreadable => {
            return match vm_presence(runtime) {
                Presence::Dead => stale(runtime, ui).map(|()| None),
                Presence::Live => Err(unreachable(
                    runtime,
                    "the VM's own process is alive under its unit and its boot record cannot be \
                    read, so no connection to it can be authorized",
                )),
                Presence::Indeterminate => {
                    Err(undecided(runtime, "its boot record cannot be read"))
                }
            };
        }
    };
    // Step 3, the half `Ping` cannot answer. The identity comparison in the handshake proves which
    // *boot* replied; this proves the boot is the one this invocation meant.
    //
    // A record naming another sandbox or target refuses outright, live or dead: this invocation
    // has no standing to repair another manifest's records, and the hint says who does.
    if boot.sandbox_id != sandbox_id || boot.target != DEFAULT_TARGET {
        return Err(foreign(runtime, "it names another sandbox or target"));
    }
    // The other two fields spec/12 step 3 can decide. These name the same project, so a mismatch is
    // this sandbox's own record to act on, and which action depends on step 4's question rather
    // than on the mismatch: a live VM is refused because replacing it is not a repair, and a dead
    // one is stale state that must be cleared or the project could never start again.
    //
    // The manifest was already selected, parsed, and resolved before this routine, so comparing
    // its complete workspace set costs no preflight, evaluation, or build. It is also the only
    // comparison that can detect a changed declaration here: this path evaluates nothing, so a
    // manifest that gained or lost a tree reaches it through no other field.
    let mismatch = if boot.workspace_paths != *workspace_paths {
        Some("it names another declared workspace set")
    } else if boot.backend != crate::launch::BACKEND {
        Some("it names another backend")
    } else {
        None
    };
    if let Some(why) = mismatch {
        return match vm_presence(runtime) {
            Presence::Dead => stale(runtime, ui).map(|()| None),
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
        Presence::Dead => return stale(runtime, ui).map(|()| None),
        Presence::Indeterminate => {
            return Err(undecided(runtime, "its guest agent did not answer a ping"));
        }
        Presence::Live => {}
    }
    // A live VM whose agent has not answered yet is a boot in flight, so this waits rather than
    // booting a second VM into the same runtime directory.
    let step = ui.step("waiting for the guest agent");
    if ping(runtime, &boot, AGENT_TIMEOUT).await {
        step.done("the guest agent answered");
        return Ok(Some(boot));
    }
    drop(step);
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
fn stale(runtime: &Runtime, ui: &Ui) -> Result<(), Failure> {
    // The unit first, because a failed unit that is never reset keeps the name the next boot needs.
    // Its own teardown removes the runtime artifacts when its supervisor is still alive to do it.
    stop_unit(runtime, "the VM", true, None, ui)?;
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

/// What one read of `boot.json` can find, with skew separated from corruption (spec/14).
pub(super) enum BootRecord {
    /// No record — the ordinary cold case.
    Absent,
    /// A record this binary cannot make sense of at all: the corruption case.
    Unreadable,
    /// A record whose envelope reads but names another version's launch schema.
    Skewed(u32),
    /// This version's own record.
    Ready(BootMetadata),
}

/// Reads the boot record through the permissive envelope before the strict schema.
///
/// The envelope parses forever, so a record another vivarium version wrote yields its number
/// here instead of collapsing into `Unreadable` — which is the collapse that let a launch
/// report success against a record the running tool could not read.
pub(super) fn read_boot_record(runtime: &Runtime) -> BootRecord {
    let bytes = match fs::read(runtime.boot_json()) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return BootRecord::Absent,
        Err(_) => return BootRecord::Unreadable,
    };
    let Ok(envelope) = serde_json::from_slice::<VersionEnvelope>(&bytes) else {
        return BootRecord::Unreadable;
    };
    if !envelope.current() {
        return BootRecord::Skewed(envelope.schema_version);
    }
    serde_json::from_slice(&bytes).map_or(BootRecord::Unreadable, BootRecord::Ready)
}

/// Item 3's boot-half refusal: a live VM whose record another version wrote.
///
/// `78` and not `69`: the channel worked and the record was read — what this binary holds is a
/// generation it must not act on, which is a configuration fact with a remedy, not an outage.
fn boot_record_skew(runtime: &Runtime, theirs: u32) -> Failure {
    diagnosed(
        Namespace::Vm,
        "boot-record-skew",
        "the VM's boot record was written by another vivarium version",
        Locus::File(runtime.boot_json()),
        format!(
            "the record carries launch schema {theirs} and this viv speaks \
            {LAUNCH_SCHEMA_VERSION}, so it cannot authorize a session against the running VM"
        ),
        ExitKind::Config,
    )
    .with_hint("run `viv stop`, then `viv start`, so this version boots and records the VM")
}

/// Reads out of the launch specification what a session needs and the boot record does not carry.
fn prepared(
    runtime: &Runtime,
    boot: BootMetadata,
    invoking_cwd: &Path,
) -> Result<Prepared, Failure> {
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
    // The envelope before the strict schema (spec/14): a record another version wrote is skew
    // with both numbers named and a remedy, distinct from the corruption `unreadable` reports.
    if let Ok(envelope) = serde_json::from_slice::<VersionEnvelope>(&bytes)
        && !envelope.current()
    {
        return Err(diagnosed(
            Namespace::Vm,
            "launch-record-skew",
            "the running VM's launch record was written by another vivarium version",
            Locus::File(path.clone()),
            format!(
                "the record carries launch schema {} and this viv speaks \
                {LAUNCH_SCHEMA_VERSION}, so a session cannot read what it needs from it",
                envelope.schema_version
            ),
            ExitKind::Config,
        )
        .with_hint("run `viv stop`, then `viv start`, so this version boots and records the VM"));
    }
    // Deserialized rather than validated: `LaunchSpec::from_json` also checks the host facts that
    // were true when the VM was launched, and a session has no business re-litigating them.
    let spec: LaunchSpec = serde_json::from_slice(&bytes)
        .map_err(|_| unreadable("it does not match the launch schema this version understands"))?;
    // ADR-0108 privileges no declaration. Every workspace is mounted at its host path, so the
    // exact invoking cwd is already its guest path after ownership was proved during resolution.
    if !boot
        .workspace_paths
        .iter()
        .any(|workspace| invoking_cwd.starts_with(workspace))
    {
        return Err(unreadable(
            "the invoking directory is outside the workspace set recorded for this boot",
        ));
    }
    let workspace_cwd = invoking_cwd.to_path_buf();
    Ok(Prepared {
        control_socket: runtime.control_socket(),
        boot,
        workspace_cwd,
        session: spec.guest_session,
    })
}

/// Builds the runner the generated flake publishes.
fn build_runner(flake_directory: &Path, ui: &Ui) -> Result<String, Failure> {
    let attribute = format!("{}#runner.{}", flake_directory.display(), nix_system());
    // The longest silent wait on the surface — a cold build runs for minutes — so this is the
    // step that earns the streaming watcher most.
    let step = ui.step("building the guest");
    let output = watch::output(
        Command::new("nix").args([
            "build",
            "--no-link",
            "--print-out-paths",
            "--extra-experimental-features",
            "nix-command flakes",
            // The pin the first evaluation installed is the one that decides this build. Letting
            // `nix` update it here would be an unannounced input jump (N3).
            "--no-update-lock-file",
            &attribute,
        ]),
        &step,
    )
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
            output.stderr.trim().to_owned(),
            ExitKind::Software,
        ));
    }
    step.done("built the guest");
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

/// The digest recorded beside a generation: a content hash of the retained lock snapshot.
///
/// Through `nix-hash` rather than a hashing dependency: the build path already requires the Nix
/// toolchain two lines earlier, the flag set is the stable CLI, and the digest is computed once
/// per build — `generations list` reads it back from the record. Shaped for `append`'s closure
/// slot, which runs it against the published snapshot so the digest names the retained bytes.
fn lock_digest(lock_path: &Path) -> Result<String, String> {
    let output = Command::new("nix-hash")
        .args(["--flat", "--base32", "--type", "sha256"])
        .arg(lock_path)
        .output()
        .map_err(|source| source.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let digest = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if digest.is_empty() {
        return Err("`nix-hash` printed nothing".to_owned());
    }
    Ok(format!("sha256:{digest}"))
}

/// Registers one generation symlink as a root Nix follows.
///
/// `nix-store --realise` on an already-built path is a cheap lookup whose `--add-root` creates
/// the symlink at its final pathname and records the indirect root under
/// `/nix/var/nix/gcroots/auto` (ADR-0014). The link must be born at that pathname — the
/// registration names it, so a stage-and-rename would leave a root Nix no longer follows, which
/// is the one outcome slice 032 forbids.
fn register_root(link: &Path, store_path: &str) -> Result<(), String> {
    let output = Command::new("nix-store")
        .arg("--realise")
        .arg(store_path)
        .arg("--add-root")
        .arg(link)
        .output()
        .map_err(|source| source.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Seconds since the epoch, saturating at zero on a clock set before 1970.
pub(super) fn epoch_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
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

/// Runs the built runner to render the launch specification, then launches from the running
/// installation.
///
/// Two children, in order (ADR-0102): the built runner resolves the host-side tokens and writes
/// `launch.json` — its job ends there — and then this binary re-invokes itself as
/// `start --spec`, the async handoff that spawns the supervisor. The supervisor is resolved
/// beside the running executable and named to the runner, because no host-side vivarium program
/// may come from the project's build.
/// Resolve every declared mount source against this host, refusing before boot (spec/06).
///
/// The session roots N24 refuses are `/tmp`, `/var/tmp`, and the host's own
/// `${XDG_RUNTIME_DIR}`; `reject_session_source` in the launch specification is the backstop
/// behind this with the same rule over the runtime root. Variable lookup reads the process
/// environment directly rather than the `Environment` trait: the trait's names are static by
/// design, and a declaration may name any variable.
fn resolve_declared_mounts<E: Environment>(
    context: &Context<'_, E>,
    store_path: &str,
) -> Result<Vec<mounts::ResolvedMount>, Failure> {
    let declared = mounts::declared_shares(store_path)
        .map_err(|error| launch_contract_unreadable(store_path, &error))?;
    if declared.is_empty() {
        return Ok(Vec::new());
    }
    let lookup =
        |name: &str| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    let mut session_roots = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
    if let Some(runtime_dir) = context.environment.variable("XDG_RUNTIME_DIR") {
        session_roots.push(PathBuf::from(runtime_dir));
    }
    // Sources are compared symlink-resolved (a link into a session directory is still a
    // session source), so the roots must be the same kind of path or the comparison would
    // read two spellings of one directory as two directories. A root that fails to resolve
    // stays as spelled: the lexical check still covers the declared form.
    for root in &mut session_roots {
        if let Ok(resolved) = root.canonicalize() {
            *root = resolved;
        }
    }
    declared
        .iter()
        .map(|share| {
            let expanded = mounts::expand_source(&share.source_token, &lookup)
                .map_err(|defect| mount_source_failure(share, defect))?;
            mounts::classify_source(&share.tag, &expanded, &session_roots)
                .map_err(|defect| mount_source_failure(share, defect))
        })
        .collect()
}

/// The contract both pre-boot readers depend on being legible, refused identically so the two
/// reads cannot drift in wording.
fn launch_contract_unreadable(store_path: &str, error: &std::io::Error) -> Failure {
    diagnosed(
        Namespace::Vm,
        "launch-contract-unreadable",
        "the selected build's launch contract cannot be read",
        Locus::File(Path::new(store_path).join("share/vivarium/launch-arguments.json")),
        error.to_string(),
        ExitKind::Config,
    )
    .with_hint("`viv start --rebuild` rebuilds the selected build with this version")
}

/// The diagnostic id for one declared-mount source defect.
///
/// Ids are a published surface (ADR-0075), so each is written out as a literal rather than
/// composed at runtime: a grep for one of these strings has to find the site that emits it.
///
/// One family, since ADR-0110. A workspace is refused at resolution now, against manifest text
/// rather than against a built contract, so its own ids live beside that check in `src/cli/mod.rs`
/// and this surface has only mounts left to name.
const fn mount_source_id(defect: &mounts::MountSourceDefect) -> &'static str {
    use mounts::MountSourceDefect as Defect;
    match defect {
        Defect::UnsetVariable { .. } => "mount-source-unset-variable",
        Defect::NotAbsolute { .. } => "mount-source-not-absolute",
        Defect::Missing { .. } => "mount-source-missing",
        Defect::Unreadable { .. } => "mount-source-unreadable",
        Defect::NotMountable { .. } => "mount-source-not-mountable",
        Defect::SessionDirectory { .. } => "mount-source-session-directory",
    }
}

fn mount_source_failure(share: &mounts::BuiltShare, defect: mounts::MountSourceDefect) -> Failure {
    source_failure(share, defect)
}

/// One diagnostic per broken source invariant, every one `78` and every one naming the
/// declared spelling, so the refusal reads against what the user wrote rather than against a
/// tag they never chose. Parameterized by surface rather than duplicated, so the two families
/// cannot drift in wording while sharing a defect set.
fn source_failure(share: &mounts::BuiltShare, defect: mounts::MountSourceDefect) -> Failure {
    use mounts::MountSourceDefect as Defect;
    let declared = &share.source_token;
    let sources_hint = "`viv config sources` names the layer that declares this mount";
    let id = mount_source_id(&defect);
    let noun = "a declared mount's source";
    match defect {
        Defect::UnsetVariable { variable } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} names `${{{variable}}}`, which this host does not set"),
            Locus::Named("declared mounts"),
            format!("`{declared}` cannot resolve while `{variable}` is unset (ADR-0020)"),
            ExitKind::Config,
        )
        .with_hint(format!(
            "set `{variable}` or declare the source another way; {sources_hint}"
        )),
        Defect::NotAbsolute { expanded } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} does not expand to an absolute path"),
            Locus::File(expanded.clone()),
            format!("`{declared}` expanded to `{}`", expanded.display()),
            ExitKind::Config,
        )
        .with_hint(sources_hint),
        Defect::Missing { expanded } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} does not exist on this host"),
            Locus::File(expanded.clone()),
            format!(
                "`{declared}` expanded to `{}`, which is missing (ADR-0020)",
                expanded.display()
            ),
            ExitKind::Config,
        )
        .with_hint(format!(
            "create the path or remove the declaration; {sources_hint}"
        )),
        Defect::Unreadable { expanded, error } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} cannot be inspected"),
            Locus::File(expanded),
            error,
            ExitKind::Config,
        )
        .with_hint(sources_hint),
        Defect::NotMountable { expanded } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} is not a regular file or directory"),
            Locus::File(expanded),
            format!(
                "`{declared}` names a socket, FIFO, or device node; a share conveys an inode, \
                not a kernel object (ADR-0071)"
            ),
            ExitKind::Config,
        )
        .with_hint(
            "sockets cross through the credential channels of spec/07, never through a mount",
        ),
        Defect::SessionDirectory { expanded } => diagnosed(
            Namespace::Host,
            id,
            format!("{noun} resolves to a host session directory"),
            Locus::File(expanded),
            format!(
                "`{declared}` lands in `/tmp`, `/var/tmp`, or `${{XDG_RUNTIME_DIR}}`, which hold \
                live session state no share may carry (N24)"
            ),
            ExitKind::Config,
        )
        .with_hint(sources_hint),
    }
}

/// The diagnostic id for one declared credential channel's host-source defect.
///
/// Ids are a published surface (ADR-0075): literals, so a grep finds the emitting site. The four
/// are spec/13's four faults, which have four different repairs.
const fn agent_source_id(defect: &agent_source::AgentSourceDefect) -> &'static str {
    use agent_source::AgentSourceDefect as Defect;
    match defect {
        Defect::Unset { .. } => "agent-source-unset",
        Defect::Missing { .. } => "agent-source-missing",
        Defect::NotSocket { .. } => "agent-source-not-socket",
        Defect::NotOwned { .. } => "agent-source-not-owned",
    }
}

/// One diagnostic per broken credential-source invariant, every one `78`: a declared channel
/// whose host source is unusable is a launch-tier configuration refusal (spec/06's two-tier
/// shape, spec/14), not an unavailable service — the missing agent belongs to the host user, and
/// the repair is theirs. A sibling of [`source_failure`] rather than a parameterization of it:
/// the two fault sets are disjoint, and a channel has no declared spelling to quote — the id
/// `ssh`/`gpg` is the whole declaration.
fn agent_source_failure(id: CredentialId, defect: agent_source::AgentSourceDefect) -> Failure {
    use agent_source::AgentSourceDefect as Defect;
    let sources_hint = "`viv config sources` names the layer that declares this channel";
    let diagnostic_id = agent_source_id(&defect);
    let noun = format!("the declared `{id}` credential channel");
    match defect {
        Defect::Unset { consulted } => diagnosed(
            Namespace::Host,
            diagnostic_id,
            format!("{noun} has no host agent socket to resolve"),
            Locus::Named("credential channels"),
            format!(
                "`{consulted}` names nothing on this host, and `{id}` resolves from it and \
                nothing else (spec/07)"
            ),
            ExitKind::Config,
        )
        .with_hint(format!(
            "start the agent, or remove the declaration; {sources_hint}"
        )),
        Defect::Missing { path } => diagnosed(
            Namespace::Host,
            diagnostic_id,
            format!("{noun} resolves to a path that does not exist"),
            Locus::File(path),
            "the agent this socket belonged to is no longer running there (spec/07)".to_owned(),
            ExitKind::Config,
        )
        .with_hint(format!(
            "restart the agent, or remove the declaration; {sources_hint}"
        )),
        Defect::NotSocket { path } => diagnosed(
            Namespace::Host,
            diagnostic_id,
            format!("{noun} does not resolve to a socket"),
            Locus::File(path),
            "a credential leg names the exact socket object, never a file or a symlink to one \
                (spec/07)"
                .to_owned(),
            ExitKind::Config,
        )
        .with_hint(sources_hint),
        Defect::NotOwned { path, owner, uid } => diagnosed(
            Namespace::Host,
            diagnostic_id,
            format!("{noun} resolves to another user's socket"),
            Locus::File(path),
            format!(
                "the socket is owned by uid {owner}, not this user ({uid}); forwarding it \
                would relay someone else's agent"
            ),
            ExitKind::Config,
        )
        .with_hint(sources_hint),
    }
}

/// Resolve every declared credential channel against this host, or refuse by name.
///
/// Called twice by design: from the evaluating path before anything is built — the acceptance's
/// "refuses before it builds" — and from [`execute_runner`] against the built contract, which is
/// the only source of the list under `--no-rebuild`. The one outcome both sites forbid is a start
/// that proceeds with a declared channel silently absent, because a relay that is missing rather
/// than refused is discovered as an authentication failure inside the guest.
pub(super) fn resolve_declared_agent_sockets(
    ids: &[CredentialId],
) -> Result<Vec<agent_source::ResolvedAgentSource>, Failure> {
    let lookup =
        |name: &str| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    let uid = config::effective_uid();
    ids.iter()
        .map(|id| {
            agent_source::resolve_and_check(*id, &lookup, uid)
                .map_err(|defect| agent_source_failure(*id, defect))
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn execute_runner<E: Environment>(
    context: &Context<'_, E>,
    store_path: &str,
    runtime: &Runtime,
    sandbox_id: &str,
    resources: Resources,
    // What resolution decided a workspace is. Carried here rather than re-derived from the built
    // contract, because the contract cannot tell a tree the manifest owns from an ordinary mount
    // that happens to name a path at itself — only the side that read `[[workspaces]]` can, and
    // relaying it is what keeps the boot record's set and the set it is compared against one fact
    // (ADR-0110).
    workspace_paths: &[PathBuf],
) -> Result<(), Failure> {
    // spec/02 and ADR-0019 fix the location: a project's volumes are part of its per-project state
    // at `projects/<sandbox-id>/<target>/volumes/<name>.img`, under the same two components the
    // runtime root mirrors. Anywhere else and `viv volume list`, `viv volume rm`, and `viv destroy`
    // could not find the user's own data, because each of them looks under the project's subtree.
    // The directory is all this passes: which images go in it, and under what names, is decided by
    // the build and joined by the launcher. `--no-rebuild` is why — it evaluates nothing, so there
    // is no path on which this function knows what a merged configuration declared.
    // Every declared mount's source resolves against this host here, before anything is
    // created, for the workspace refusal's reason stated above: this is where a user meets
    // "the variable is unset" or "the path is missing" as a sentence rather than as a dead
    // boot (spec/06, ADR-0020). The list comes from the built contract because a
    // piece-declared mount exists only in the merged evaluation and `--no-rebuild`
    // evaluates nothing.
    let declared_mounts = resolve_declared_mounts(context, store_path)?;

    // The credential channels ride the same contract as the mounts and refuse on the same
    // pre-boot surface. The evaluating path already refused before the build; this site is the
    // one gate under `--no-rebuild`, and the resolution the runner arguments below carry.
    let declared_credentials = mounts::declared_credentials(store_path)
        .map_err(|error| launch_contract_unreadable(store_path, &error))?;
    let agent_sockets = resolve_declared_agent_sockets(&declared_credentials)?;

    let volumes = volume_directory(&context.roots, sandbox_id, DEFAULT_TARGET);
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

    let (viv, supervisor) = installation_programs()?;

    let program = Path::new(store_path)
        .join("bin")
        .join("vivarium-base-image");
    let step = context.ui.step("rendering the launch specification");
    let mut command = Command::new(&program);
    command
        .arg("--runtime-dir")
        .arg(&runtime.directory)
        .arg("--volume-dir")
        .arg(&volumes)
        .arg("--supervisor")
        .arg(&supervisor)
        .arg("--uid")
        .arg(config::effective_uid().to_string())
        .arg("--gid")
        .arg(config::effective_gid().to_string())
        .arg("--memory-mib")
        .arg(resources.mem_mib.to_string())
        .arg("--vcpu")
        .arg(resources.vcpu.to_string())
        .arg("--sandbox-id")
        .arg(sandbox_id)
        .arg("--target")
        .arg(DEFAULT_TARGET);
    for mount in &declared_mounts {
        // Four values per flag, matching the runner's `--mount TAG KIND ABS ENTRY` group; the
        // runner's own belt re-checks the set against the contract's declared tags.
        command
            .arg("--mount")
            .arg(&mount.tag)
            .arg(match mount.kind {
                MountPlanKind::Dir if workspace_paths.contains(&mount.share_source) => "tree",
                MountPlanKind::Dir | MountPlanKind::Tree => "dir",
                MountPlanKind::File => "file",
            })
            .arg(&mount.share_source)
            .arg(mount.entry.as_deref().unwrap_or("-"));
    }
    for credential in &agent_sockets {
        // The host socket the relay serves, resolved above; the runner's own guard re-checks the
        // pair against the contract's `credentialIds`.
        command
            .arg(match credential.id {
                CredentialId::Ssh => "--ssh-agent-socket",
                CredentialId::Gpg => "--gpg-agent-socket",
            })
            .arg(&credential.host_socket);
    }
    let output = watch::output(&mut command, &step).map_err(|source| {
        diagnosed(
            Namespace::Vm,
            "launcher-unavailable",
            "could not run the guest launcher",
            Locus::File(program.clone()),
            source.to_string(),
            ExitKind::Software,
        )
    })?;
    drop(step);

    if !output.status.success() {
        // The runner's own stderr names what it refused — including its usage guard, which is
        // the belt for a launcher whose argument names this binary does not speak.
        return Err(diagnosed(
            Namespace::Vm,
            "spec-render-failed",
            "the launch specification could not be rendered",
            Locus::File(program),
            output.stderr.trim().to_owned(),
            ExitKind::Software,
        ));
    }

    boot_rendered_spec(context, &viv, runtime)
}

/// The second half of the launch: hand the rendered specification to the async handoff and wait
/// for the supervisor's readiness answer.
fn boot_rendered_spec<E: Environment>(
    context: &Context<'_, E>,
    viv: &Path,
    runtime: &Runtime,
) -> Result<(), Failure> {
    let spec = runtime.directory.join("launch.json");
    let step = context.ui.step("booting the VM");
    let output = watch::output(
        Command::new(viv).arg("start").arg("--spec").arg(&spec),
        &step,
    )
    .map_err(|source| {
        diagnosed(
            Namespace::Vm,
            "launcher-unavailable",
            "could not re-invoke this binary for the launch handoff",
            Locus::File(viv.to_path_buf()),
            source.to_string(),
            ExitKind::Software,
        )
    })?;
    if output.status.success() {
        step.done("booted the VM");
    } else {
        drop(step);
    }

    if !output.status.success() {
        // The handoff's own stderr is the account of which child died; the readiness socket
        // carries a status and nothing more (see docs/explanation/launch-and-supervision.md).
        return Err(diagnosed(
            Namespace::Vm,
            "start-failed",
            "the VM did not come up",
            Locus::Named("guest launch"),
            output.stderr.trim().to_owned(),
            ExitKind::Unavailable,
        )
        .with_hint(format!(
            "`journalctl --user -u {}` carries the supervisor's own diagnostic",
            runtime.unit
        )));
    }
    Ok(())
}

/// The two host-side programs a launch runs, both from the running installation (ADR-0102).
///
/// `viv` and `vivarium-supervisor` install side by side — `cargo install` puts both in one
/// binary directory, and the development shim builds both into one target directory — so the
/// supervisor is resolved beside the running executable rather than searched for on `PATH`,
/// where a second installation could shadow the one actually running.
fn installation_programs() -> Result<(PathBuf, PathBuf), Failure> {
    let viv = std::env::current_exe().map_err(|source| {
        diagnosed(
            Namespace::Host,
            "supervisor-missing",
            "could not resolve the running executable",
            Locus::Named("installation"),
            source.to_string(),
            ExitKind::Unavailable,
        )
    })?;
    let supervisor = viv.with_file_name("vivarium-supervisor");
    if !supervisor.is_file() {
        return Err(diagnosed(
            Namespace::Host,
            "supervisor-missing",
            "the launch supervisor is not installed beside this binary",
            Locus::File(supervisor),
            "`viv` and `vivarium-supervisor` install together, and a launch runs both",
            ExitKind::Unavailable,
        )
        .with_hint("reinstall vivarium; `just install` places both binaries side by side"));
    }
    Ok((viv, supervisor))
}

/// Where one project's persistent volumes live (spec/02, ADR-0019).
pub(super) fn volume_directory(
    roots: &config::XdgRoots,
    sandbox_id: &str,
    target: &str,
) -> PathBuf {
    roots
        .state
        .join("projects")
        .join(sandbox_id)
        .join(target)
        .join("volumes")
}

/// The ceilings in force for one launch: declared wins, undeclared resolves from the host.
///
/// spec/10 step 5 puts this moment at ensure-running and spec/17 fixes the arithmetic, both for
/// the same reason: a host-derived ceiling is launch-channel, never a build input (N3, N19), so
/// two hosts building one manifest boot it with different ceilings and the same store path.
pub(super) fn effective_resources(declared: Option<&config::Resources>) -> Resources {
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

    // `MemTotal` is physical memory, which is the only reading this needs — the available figure
    // is what admission control asks about, and that gate is slice 026's.
    let total_mib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| crate::doctor::total_memory_bytes(&meminfo))
        .map(|bytes| bytes / (1024 * 1024));
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
/// records it. A specification that cannot be read or parsed yields `None`, which spec/01 renders
/// as `null` — the exit Q-015 chose, because the only honest value for an unknown integer is no
/// integer, and a fabricated floor is a constant where a reader reads the ceiling in force. A
/// `failed` state is not the alternative: a VM with a live process and an active unit is running,
/// and only this bookkeeping is broken.
pub(super) fn declared_resources(runtime: &Runtime) -> Option<Resources> {
    let raw = std::fs::read_to_string(runtime.launch_spec()).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let resources = &value["resources"];
    Some(Resources {
        mem_mib: resources["memoryMiB"].as_u64()?,
        vcpu: resources["vcpus"].as_u64()?,
    })
}

/// What the project's VM is doing. Read-only, and any reported state exits `0`.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`) or an unusable runtime root.
pub(super) async fn status<E: Environment + Sync>(
    context: &Context<'_, E>,
) -> Result<Report, Failure> {
    // Manifest first, then the runtime root, and the order is the contract rather than a
    // preference: ADR-0109 makes the undeclared-directory refusal one routine every
    // manifest-resolving verb fails fast through, so it has to be the first thing that can fail.
    // Resolving the runtime root first would answer an undeclared directory on a host with an
    // unusable `${XDG_RUNTIME_DIR}` with `77` and a runtime-root diagnostic, which is a true
    // statement about the host and the wrong answer to what the user did.
    let resolved = super::resolve_manifest_for_launch(context)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;

    let sandbox_id = resolved.selected.name.clone();
    let runtime = Runtime::locate(&runtime_root, &sandbox_id, DEFAULT_TARGET)?;
    let recorded = last_build(&context.roots, &sandbox_id, DEFAULT_TARGET);
    let state = discriminate(&runtime, recorded.is_some());

    let running = matches!(state, State::Running | State::Stopping);
    let record = if running {
        read_boot_record(&runtime)
    } else {
        BootRecord::Absent
    };
    let readings = runtime_readings(context, &runtime, &sandbox_id, running, &record).await;
    Ok(Report {
        manifest: Some(resolved.binding.manifest),
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
            && running_build(&context.roots, &sandbox_id, DEFAULT_TARGET).is_some_and(|launched| {
                recorded
                    .as_ref()
                    .is_some_and(|current| *current != launched)
            }),
        store_path: recorded,
        generation: config::generations::current_number(&generation_paths(
            &context.roots,
            &sandbox_id,
            DEFAULT_TARGET,
        )),
        uptime_seconds: running.then(|| uptime_seconds(&runtime)).flatten(),
        resources: running.then(|| declared_resources(&runtime)).flatten(),
        runtime: readings,
        record_skew: match record {
            BootRecord::Skewed(theirs) => Some(theirs),
            BootRecord::Absent | BootRecord::Unreadable | BootRecord::Ready(_) => None,
        },
    })
}

/// The measured half of one sandbox's report — spec/17's readings, assembled the same way for
/// the local report and for a fleet row. Every reading degrades to `None` rather than failing:
/// an unanswerable question is a `null` in a report, not a reason to refuse one, and spec/14's
/// status rows admit no exit code for it.
pub(super) async fn runtime_readings<E: Environment + Sync>(
    context: &Context<'_, E>,
    runtime: &Runtime,
    sandbox_id: &str,
    running: bool,
    record: &BootRecord,
) -> RuntimeReadings {
    let disk = super::volume::disk_totals(&context.roots, sandbox_id).ok();
    let (mem_used_bytes, cgroup) = if running {
        unit_memory_and_cgroup(&runtime.unit)
    } else {
        (None, None)
    };
    // The count is asked of this boot's own agent, so a record that names another version's
    // schema — or none — leaves the question unaskable and the field honestly `null`.
    let sessions = match record {
        BootRecord::Ready(boot) if running => {
            crate::launch::control::session_count(&runtime.control_socket(), boot, PING_TIMEOUT)
                .await
        }
        _ => None,
    };
    RuntimeReadings {
        mem_used_bytes,
        disk_allocated_bytes: disk.map(|(allocated, _)| allocated),
        disk_virtual_bytes: disk.map(|(_, apparent)| apparent),
        sessions,
        pressure_some_avg60: cgroup.as_deref().and_then(scope_pressure_some_avg60),
    }
}

/// How long the VM has been up, from the age of the record the supervisor writes at boot.
pub(super) fn uptime_seconds(runtime: &Runtime) -> Option<u64> {
    let metadata = std::fs::metadata(runtime.boot_json()).ok()?;
    let modified = metadata.modified().ok()?;
    modified.elapsed().ok().map(|elapsed| elapsed.as_secs())
}

/// Bring the VM down, stopping at the teardown boundary ADR-0018 fixes.
///
/// Async because the ladder's first rung asks the guest agent over the control socket
/// (spec/10, spec/12), so it is dispatched from `main` beside `status` and the session verbs.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), or a stop that cannot be confirmed (`69`).
pub(super) async fn stop<E: Environment + Sync>(
    context: &Context<'_, E>,
    force: bool,
    timeout: Option<i64>,
    output: Output,
) -> Result<Success, Failure> {
    // Manifest before runtime root, for the reason `status` states above.
    let resolved = super::resolve_manifest_for_launch(context)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let sandbox_id = resolved.selected.name;
    let runtime = Runtime::locate(&runtime_root, &sandbox_id, DEFAULT_TARGET)?;

    // Idempotent (spec/10): nothing running is a `0` no-op. Checked against the discriminator
    // rather than against the unit, so a `failed` VM with dead records still reaches the ladder.
    // The record's `rung` is `null` here because no rung ran (spec/01).
    let has_build = last_build(&context.roots, &sandbox_id, DEFAULT_TARGET).is_some();
    let state = discriminate(&runtime, has_build);
    if matches!(state, State::Absent | State::Built) {
        return Ok(stop_record(&sandbox_id, state, None, output));
    }

    let rung = stop_one(&runtime, &sandbox_id, force, timeout, context.ui).await?;

    // Re-discriminated rather than assumed, so the record reports what a `status` run now would.
    let state = discriminate(&runtime, has_build);
    Ok(stop_record(&sandbox_id, state, rung, output))
}

/// `viv stop --all`: the same ladder, applied to every enumerated sandbox that is up.
///
/// The one project-local verb that resolves no manifest — ADR-0109's stated exception, because
/// the sweep enumerates the index rather than a working directory, and a directory no manifest
/// declares is exactly where an end of day is allowed to start. Every sandbox is attempted: a
/// sweep that stopped at the first fault would leave the rest running behind a nonzero exit, so
/// each failing sandbox is named on stderr as it fails and the first failure is the verb's
/// result (spec/14).
///
/// # Errors
///
/// Returns [`Failure`] for an unreadable manifest library or index (`74`, `78`), or — after the
/// whole sweep ran — the first sandbox whose stop could not be confirmed (`69`).
pub(super) async fn stop_all<E: Environment + Sync>(
    context: &Context<'_, E>,
    force: bool,
    timeout: Option<i64>,
    output: Output,
) -> Result<Success, Failure> {
    let index = super::workspace_index(context, true)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;

    let mut rows = Vec::new();
    let mut first_failure = None;
    for indexed in index.manifests() {
        // `for_report` rather than `locate`: a control-socket path past the bind limit costs
        // that sandbox its first rung, and refusing the whole sweep for one long name would
        // leave everything else running.
        let runtime = Runtime::for_report(&runtime_root, &indexed.name, DEFAULT_TARGET);
        let has_build = last_build(&context.roots, &indexed.name, DEFAULT_TARGET).is_some();
        if matches!(
            discriminate(&runtime, has_build),
            State::Absent | State::Built
        ) {
            continue;
        }
        match stop_one(&runtime, &indexed.name, force, timeout, context.ui).await {
            Ok(rung) => rows.push(super::render::stop_row(
                &indexed.name,
                discriminate(&runtime, has_build).as_str(),
                rung.map(StopRung::as_str),
            )),
            Err(failure) => {
                // One naming line now, so the continuation does not silence the sandbox that
                // failed; the full skeleton is the exit's, rendered once (spec/14 keeps the
                // machine face to one failure object per invocation).
                context
                    .ui
                    .warn(&format!("could not stop `{}`", indexed.name));
                record_failure(&mut first_failure, failure);
            }
        }
    }
    if let Some(failure) = first_failure {
        return Err(failure);
    }
    Ok(Success::plain(if output.is_json() {
        super::render::stop_all_json(&rows)
    } else {
        String::new()
    }))
}

/// The verb's result: spec/01's stop record under `--json`, empty stdout otherwise.
fn stop_record(manifest: &str, state: State, rung: Option<StopRung>, output: Output) -> Success {
    Success::plain(if output.is_json() {
        super::render::stop_json(manifest, state.as_str(), rung.map(StopRung::as_str))
    } else {
        String::new()
    })
}

/// The sweep's failure fold: the first failure is the result, later ones never displace it.
fn record_failure(first: &mut Option<Failure>, failure: Failure) {
    first.get_or_insert(failure);
}

/// One sandbox's whole teardown: the ladder, then the post-condition.
///
/// The single routine both faces of the verb run. The sweep reporting success for a sandbox is
/// exactly this function returning `Ok`, which keeps "confirmed stopped" one judgement rather
/// than two (the outcome the slice's core forbids is a sweep with a cheaper one).
async fn stop_one(
    runtime: &Runtime,
    sandbox_id: &str,
    force: bool,
    timeout: Option<i64>,
    ui: &Ui,
) -> Result<Option<StopRung>, Failure> {
    let rung = stop_ladder(runtime, sandbox_id, force, timeout, ui).await?;
    confirm_teardown(runtime)?;
    Ok(rung)
}

/// The post-condition, asserted in the product rather than only in a trial: after a completed
/// stop the runtime directory holds no entry the allowlisted cleanup is required to remove. The
/// supervisor removes the directory itself, so the honest test is that it is gone, or holds
/// nothing but the startup lock. The lock is excluded because the sweep is required *not* to
/// remove it: it is the per-target mutex (spec/12 step 1) rather than an artifact of the VM, a
/// caller can be holding it across this very teardown, and unlinking it would end the exclusion
/// its pathname provides. Counting it here would make every `viv`-driven stop report a teardown
/// it completed correctly as incomplete.
///
/// An unreadable directory is not an empty one. Counting a failed inspection as zero would make
/// `stop` claim its post-condition at the one moment it cannot be established, so anything but
/// "gone" or "readable and holding at most the lock" is the unconfirmed teardown spec/10 assigns
/// the unavailable code to.
fn confirm_teardown(runtime: &Runtime) -> Result<(), Failure> {
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
            Locus::File(runtime.directory.clone()),
            format!("{residue} entries survived the allowlisted cleanup"),
            ExitKind::Unavailable,
        ));
    }
    Ok(())
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

/// The ladder's first rung and the dispatch below it (spec/10 "Stopping").
///
/// One deadline, computed from [`grace_seconds`], bounds the orderly ask: the agent is asked for
/// a guest shutdown, and an acknowledgement means the guest is powering itself off — the VMM
/// exits, the supervisor reads a clean guest exit, sweeps the runtime directory, and the unit
/// goes inactive, which is what the poll below watches for. Every non-answer falls through to
/// [`stop_unit`]'s manager rungs under what remains of the same deadline, exactly the fallback
/// spec/10 assigns to an unreachable agent. A skewed or unreadable boot record degrades the same
/// way rather than refusing, because `stop` stays the verb that works on what another vivarium
/// version left behind.
///
/// An acknowledged ask that outlives the grace is escalated through the power path with the
/// compiled [`POWER_SIGNAL_WINDOW_SECONDS`] rather than killed at the deadline. That bounded
/// overshoot is the `Q-018` exit: the operator's flag fully bounds the ask, and the constant
/// bounds only the escalation beneath it.
pub(super) async fn stop_ladder(
    runtime: &Runtime,
    sandbox_id: &str,
    force: bool,
    timeout: Option<i64>,
    ui: &Ui,
) -> Result<Option<StopRung>, Failure> {
    let grace = grace_seconds(force, timeout);
    let deadline = grace.map(|seconds| std::time::Instant::now() + Duration::from_secs(seconds));

    // Rung one, skipped when the operator asked for now (`--force`, `--timeout 0`).
    if grace != Some(0)
        && let BootRecord::Ready(metadata) = read_boot_record(runtime)
    {
        let ask = deadline.map_or(SHUTDOWN_ASK_TIMEOUT, |deadline| {
            SHUTDOWN_ASK_TIMEOUT.min(deadline.saturating_duration_since(std::time::Instant::now()))
        });
        if !ask.is_zero()
            && crate::launch::control::request_shutdown(&runtime.control_socket(), &metadata, ask)
                .await
        {
            let step = ui.step(&format!("stopping `{sandbox_id}` · guest shutdown"));
            let started = std::time::Instant::now();
            loop {
                match unit_active_state(&runtime.unit).as_deref() {
                    None | Some("inactive" | "failed") => {
                        step.done(&format!("stopped `{sandbox_id}` · guest shutdown"));
                        return Ok(Some(StopRung::Agent));
                    }
                    _ => {}
                }
                if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                    break;
                }
                step.update(&format!(
                    "stopping `{sandbox_id}` · guest shutdown · {}s",
                    started.elapsed().as_secs()
                ));
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            drop(step);
            // The guest acknowledged and then wedged inside its own shutdown: the power path
            // gets its compiled window — never the operator's grace again — and the unit
            // having vanished in between means the acknowledged shutdown finished after all.
            let rung = stop_unit(
                runtime,
                sandbox_id,
                false,
                Some(POWER_SIGNAL_WINDOW_SECONDS),
                ui,
            )?;
            return Ok(Some(rung.unwrap_or(StopRung::Agent)));
        }
    }

    // No acknowledgement: the manager rungs, under the operator's own grace — deliberately not
    // reduced by what the ask consumed. The fallback's floor is the supervisor's compiled
    // transaction (about eight seconds plus cleanup), so deducting a fully-hung ask from the
    // default ten would hand the power path seven and kill the group mid-cleanup — failing a
    // stop the fallback was specified to complete. An unanswered ask therefore stretches the
    // ladder by at most its own two-second budget, the bounded overshoot spec/10 states.
    stop_unit(runtime, sandbox_id, force, timeout, ui)
}

/// The manager rungs of spec/10's ladder, expressed against the transient unit.
///
/// Stopping the unit converges every exit route through the supervisor's single cancellation
/// path (ADR-0097). `KillMode=mixed` means the stop signal reaches the supervisor alone, which
/// is what lets it raise the guest's ACPI power button and wait — a whole-group signal would
/// kill the VMM where it stood and lose whatever the guest had not committed.
///
/// Rung one — the agent's orderly shutdown — is [`stop_ladder`]'s, above this. The two callers
/// that enter here directly do so deliberately: `start --rebuild` is replacing the guest and the
/// power signal still runs its full shutdown transaction, and the stale-record repair is
/// stopping a unit whose VM is already gone. Returns the rung that ended the stop, or `None`
/// when the unit was already gone when the manager was asked.
fn stop_unit(
    runtime: &Runtime,
    sandbox_id: &str,
    force: bool,
    timeout: Option<i64>,
    ui: &Ui,
) -> Result<Option<StopRung>, Failure> {
    // The power rung runs the supervisor's cancellation path, which raises the guest's ACPI
    // power button and then destroys the VM if the guest did not take it. What is decided here
    // is the rung after those — how long to wait before pulling the power — so the stop is
    // issued without waiting and the deadline is enforced here.
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
            return Ok(None);
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
    // is what `--timeout -1` asks for. The ticking message is the whole feedback for that
    // indefinite wait — without it, `--timeout -1` is indistinguishable from a hang.
    let step = ui.step(&format!("stopping `{sandbox_id}`"));
    let started = std::time::Instant::now();
    let deadline =
        grace.map(|seconds| std::time::Instant::now() + std::time::Duration::from_secs(seconds));
    loop {
        match unit_active_state(&runtime.unit).as_deref() {
            // Gone, or never loaded: the unit's lifetime is over and with it the VM's.
            None | Some("inactive" | "failed") => {
                step.done(&format!("stopped `{sandbox_id}`"));
                return Ok(Some(StopRung::PowerSignal));
            }
            _ => {}
        }
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            break;
        }
        step.update(&format!(
            "stopping `{sandbox_id}` · {}s",
            started.elapsed().as_secs()
        ));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    step.update(&format!("stopping `{sandbox_id}` · hard poweroff"));

    // The last rung, and it is whole-group regardless of `KillMode`: `systemctl kill` defaults to
    // `--kill-whom=all`, so the VMM, every per-share daemon, and any launch helper go away together
    // (ADR-0097, spec/17). That is why narrowing `KillMode` to `mixed` for the graceful path costs
    // nothing here — this rung never depended on it.
    let killed = Command::new("systemctl")
        .args(["--user", "kill", "--signal=SIGKILL", &runtime.unit])
        .output();
    if killed.is_ok_and(|output| output.status.success()) {
        // Give the group the moment it takes to reap before reporting.
        for _ in 0..50 {
            match unit_active_state(&runtime.unit).as_deref() {
                None | Some("inactive" | "failed") => {
                    step.done(&format!("stopped `{sandbox_id}` · hard poweroff"));
                    return Ok(Some(StopRung::HardPoweroff));
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    }
    drop(step);

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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{
        Admission, BootMetadata, BootRecord, ExitKind, LAUNCH_SCHEMA_VERSION, Locus, Namespace,
        Output, Runtime, State, StopRung, admission_warning, admit, classify, config, diagnosed,
        discriminate, effective_resources, grace_seconds, host_mem_mib, host_vcpu, ownership_of,
        parse_memory_current, parse_pressure_some_avg60, parse_unit_memory_show, read_boot_record,
        record_failure, recorded_pid_liveness, require_current_contract, vm_is_alive,
        volume_directory,
    };
    use crate::test_support::ScratchDirectory;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// spec/17's admission table, row by row at its boundaries. The reserve is 1 GiB.
    #[test]
    fn admit_walks_the_admission_table() {
        const GIB: u64 = 1024 * 1024 * 1024;
        // Below the reserve refuses; at the reserve does not.
        assert_eq!(admit(Some(GIB - 1), None), Admission::Refuse);
        assert_eq!(admit(Some(GIB - 1), Some(0)), Admission::Refuse);
        assert_eq!(admit(Some(GIB), Some(0)), Admission::Proceed);
        // Fleet use plus the reserve at available proceeds; one byte past it warns.
        assert_eq!(admit(Some(3 * GIB), Some(2 * GIB)), Admission::Proceed);
        assert_eq!(admit(Some(3 * GIB), Some(2 * GIB + 1)), Admission::Warn);
        // An unavailable fleet term cannot trip the warn tier: the decision is still reached
        // from the host reading alone (spec/17's degraded mode), never skipped.
        assert_eq!(admit(Some(GIB), None), Admission::Proceed);
        // An absent host reading decides nothing, the same honest skip the probe takes.
        assert_eq!(admit(None, None), Admission::Proceed);
        assert_eq!(admit(None, Some(u64::MAX)), Admission::Proceed);
        // The sum saturates rather than wrapping: a fleet near the ceiling still warns.
        assert_eq!(admit(Some(u64::MAX), Some(u64::MAX)), Admission::Warn);
    }

    /// spec/17's warning shape, minus the `viv trim` line slice 028 owns.
    #[test]
    fn admission_warning_is_the_spec_shape_without_trim() {
        let fleet = super::super::fleet::FleetUse {
            running: 4,
            measured: 4,
            complete: true,
            measured_bytes: Some(213 * 1024 * 1024 * 1024 / 10),
        };
        let warning = admission_warning(&fleet, Some(312 * 1024 * 1024 * 1024 / 10), "api-gateway");
        assert_eq!(
            warning,
            concat!(
                "warning: 4 project VMs are running and using 21.3 GiB of 31.2 GiB host memory.\n",
                "         Starting `api-gateway` may push the host into swap.\n",
                "         viv status -g     see what is running\n",
                "         viv stop          stop a project you are done with"
            )
        );
        assert!(!warning.contains("viv trim"));
    }

    /// A partial fleet sum is a lower bound and says so; a single VM reads singular.
    #[test]
    fn admission_warning_marks_a_partial_sum_as_a_lower_bound() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let fleet = super::super::fleet::FleetUse {
            running: 2,
            measured: 1,
            complete: true,
            measured_bytes: Some(2 * GIB),
        };
        let warning = admission_warning(&fleet, Some(8 * GIB), "api-gateway");
        assert!(warning.starts_with(
            "warning: 2 project VMs are running and using at least 2 GiB of 8 GiB host memory."
        ));
        let one = super::super::fleet::FleetUse {
            running: 1,
            measured: 1,
            complete: true,
            measured_bytes: Some(GIB),
        };
        assert!(
            admission_warning(&one, Some(8 * GIB), "api-gateway")
                .starts_with("warning: 1 project VM is running and using 1 GiB of 8 GiB")
        );
    }

    /// An incomplete scan turns an apparently exact sum into a stated floor.
    #[test]
    fn admission_warning_marks_an_incomplete_scan_as_a_lower_bound() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let fleet = super::super::fleet::FleetUse {
            running: 2,
            measured: 2,
            complete: false,
            measured_bytes: Some(3 * GIB),
        };
        assert!(
            admission_warning(&fleet, Some(8 * GIB), "api-gateway").starts_with(
                "warning: 2 project VMs are running and using at least 3 GiB of 8 GiB"
            )
        );
    }

    /// An unknown host total drops its clause rather than fabricating a figure.
    #[test]
    fn admission_warning_drops_an_unknown_total() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let fleet = super::super::fleet::FleetUse {
            running: 3,
            measured: 3,
            complete: true,
            measured_bytes: Some(4 * GIB),
        };
        assert!(
            admission_warning(&fleet, None, "api-gateway")
                .starts_with("warning: 3 project VMs are running and using 4 GiB of host memory.")
        );
    }

    static SHORT_RUNTIME_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    /// A short runtime root for tests that exercise the real Unix-socket layout.
    ///
    /// Heavy test runs deliberately place `TMPDIR` on the external drive. That path can exceed
    /// `sun_path` before the sandbox component is involved, while the production runtime root is
    /// the short, writable `XDG_RUNTIME_DIR`. These tests care about records and discrimination,
    /// not the separately asserted 107-byte socket boundary, so their fixture mirrors the
    /// production root instead of inheriting `TMPDIR`.
    struct ShortRuntimeRoot(PathBuf);

    impl ShortRuntimeRoot {
        fn new() -> Self {
            let sequence = SHORT_RUNTIME_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap()
                .join(format!("viv-test-{}-{sequence}", std::process::id()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for ShortRuntimeRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    /// The resting states, each reachable from what the filesystem says and nothing else.
    #[test]
    fn the_resting_states_are_decided_by_records_and_build_output() {
        let runtime_root = ShortRuntimeRoot::new();
        let runtime = Runtime::locate(runtime_root.path(), "api", "default").unwrap();
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
        let runtime_root = ShortRuntimeRoot::new();
        let runtime = Runtime::locate(runtime_root.path(), "api", "default").unwrap();
        fs::create_dir_all(&runtime.directory).ok();

        for raw in ["0", "0\n", "-1", "", "  ", "not-a-pid"] {
            fs::write(runtime.directory.join("vm.pid"), raw).ok();
            assert!(
                !vm_is_alive(&runtime),
                "a vm.pid of {raw:?} was read as a live VM"
            );
            // And three-valued for the destructive consumers: a record that establishes
            // nothing is no evidence, never a positive death.
            assert_eq!(
                recorded_pid_liveness(&runtime),
                None,
                "a vm.pid of {raw:?} was read as evidence"
            );
        }
        fs::remove_file(runtime.directory.join("vm.pid")).ok();
        assert_eq!(
            recorded_pid_liveness(&runtime),
            None,
            "a missing vm.pid was read as evidence"
        );

        // The control: this test's own process is unambiguously alive, so the check still says yes
        // when the record names something real. Without this the assertions above would pass just
        // as well against a function that always answered `false`.
        fs::write(
            runtime.directory.join("vm.pid"),
            format!("{}\n", std::process::id()),
        )
        .ok();
        assert!(vm_is_alive(&runtime));
        assert_eq!(recorded_pid_liveness(&runtime), Some(true));

        // The one positive death: a pid that parsed and whose process the kernel says is gone —
        // a spawned child, reaped before it is asked about. Reading it requires `SRCH` to be
        // classified as death while every other probe failure stays no-evidence.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        fs::write(runtime.directory.join("vm.pid"), format!("{pid}\n")).ok();
        assert_eq!(
            recorded_pid_liveness(&runtime),
            Some(false),
            "a reaped child must read as a positive death"
        );
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
        let runtime_root = ShortRuntimeRoot::new();
        let runtime = Runtime::locate(runtime_root.path(), "api", "default").unwrap();
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

    /// Item 4 of slice 015: the selected build's contract is read from the built output, and a
    /// build that speaks another number — or none — is refused before boot with `78`.
    #[test]
    fn a_build_speaking_another_contract_is_refused_before_boot() {
        let scratch = ScratchDirectory::new().unwrap();
        let build = scratch.path().join("fake-build");
        let schema_dir = build.join("share").join("vivarium");
        fs::create_dir_all(&schema_dir).unwrap();
        let store_path = build.to_string_lossy().into_owned();
        let schema = schema_dir.join("launch-contract-schema");

        fs::write(&schema, format!("{LAUNCH_SCHEMA_VERSION}\n")).unwrap();
        assert!(require_current_contract(&store_path).is_ok());

        fs::write(&schema, format!("{}\n", LAUNCH_SCHEMA_VERSION - 1)).unwrap();
        let refusal = require_current_contract(&store_path).unwrap_err();
        let rendered = format!("{refusal:?}");
        assert!(rendered.contains("launch-contract-skew"));
        assert!(rendered.contains(&format!("schema {}", LAUNCH_SCHEMA_VERSION - 1)));
        assert!(rendered.contains(&LAUNCH_SCHEMA_VERSION.to_string()));
        assert!(rendered.contains("--rebuild"));

        // A pre-publication build has no schema file at all, and is the same refusal with the
        // migration named: every build made before this slice meets exactly this case.
        fs::remove_file(&schema).unwrap();
        let refusal = require_current_contract(&store_path).unwrap_err();
        assert!(format!("{refusal:?}").contains("predates the launch-contract publication"));
    }

    /// Item 3 of slice 015: skew is told apart from corruption, and both from absence.
    #[test]
    fn a_record_from_another_version_reads_as_skew_not_corruption() {
        let runtime_root = ShortRuntimeRoot::new();
        let runtime = Runtime::locate(runtime_root.path(), "envelope", "default").unwrap();
        fs::create_dir_all(&runtime.directory).ok();

        assert!(matches!(read_boot_record(&runtime), BootRecord::Absent));

        fs::write(runtime.directory.join("boot.json"), "not json").ok();
        assert!(matches!(read_boot_record(&runtime), BootRecord::Unreadable));

        // A record from a hypothetical future version: fields this binary has never heard of,
        // and a version it does not speak. The envelope must still yield the number.
        fs::write(
            runtime.directory.join("boot.json"),
            format!(
                "{{\"schemaVersion\":{},\"bootIdentity\":\"x\",\"fieldFromTheFuture\":true}}",
                LAUNCH_SCHEMA_VERSION + 1
            ),
        )
        .ok();
        assert!(
            matches!(
                read_boot_record(&runtime),
                BootRecord::Skewed(theirs) if theirs == LAUNCH_SCHEMA_VERSION + 1
            ),
            "a foreign version must read as skew"
        );

        // This version's own record parses strictly.
        let own = BootMetadata {
            schema_version: LAUNCH_SCHEMA_VERSION,
            boot_identity: "b".into(),
            sandbox_id: "envelope".into(),
            target: "default".into(),
            backend: crate::launch::BACKEND.into(),
            workspace_paths: vec![PathBuf::from("/w")],
        };
        fs::write(
            runtime.directory.join("boot.json"),
            serde_json::to_vec(&own).unwrap(),
        )
        .ok();
        assert!(
            matches!(read_boot_record(&runtime), BootRecord::Ready(read) if read == own),
            "this version's record must parse strictly"
        );
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

    /// The sweep's exit is its first failure: later ones are named as they happen, never kept.
    #[test]
    fn the_first_failure_is_the_sweeps_result() {
        let failed = |id: &'static str| {
            diagnosed(
                Namespace::Vm,
                id,
                "the VM could not be confirmed stopped",
                Locus::Named("guest teardown"),
                "trial fixture",
                ExitKind::Unavailable,
            )
        };
        let mut first = None;
        record_failure(&mut first, failed("stop-unconfirmed"));
        record_failure(&mut first, failed("teardown-incomplete"));
        let kept = first.unwrap();
        assert!(
            kept.render(Output::Json, &crate::ui::style::Palette::plain())
                .contains("stop-unconfirmed"),
            "a later failure must never displace the first"
        );
    }

    /// The three-way presence judgment: only a positive answer may say `Dead`.
    ///
    /// The destructive consumers (destroy's unlink gate) key on that word, so the rows where
    /// evidence is missing — an unreadable pid record, an unaskable manager, or both — must
    /// land on `Indeterminate` or on the side the one positive answer supports.
    #[test]
    fn presence_requires_positive_evidence_of_death() {
        use super::{Presence, presence_of};
        // Positive death from either side wins.
        assert_eq!(presence_of(Some(false), None), Presence::Dead);
        assert_eq!(presence_of(Some(false), Some(true)), Presence::Dead);
        assert_eq!(presence_of(None, Some(false)), Presence::Dead);
        assert_eq!(presence_of(Some(true), Some(false)), Presence::Dead);
        // A live answer without a disowning manager is live.
        assert_eq!(presence_of(Some(true), Some(true)), Presence::Live);
        assert_eq!(presence_of(None, Some(true)), Presence::Live);
        // No evidence is not death.
        assert_eq!(presence_of(None, None), Presence::Indeterminate);
        assert_eq!(presence_of(Some(true), None), Presence::Indeterminate);
    }

    /// The record's rung spellings are the closed set spec/01 names.
    #[test]
    fn rung_spellings_are_the_specified_set() {
        assert_eq!(StopRung::Agent.as_str(), "agent");
        assert_eq!(StopRung::PowerSignal.as_str(), "power-signal");
        assert_eq!(StopRung::HardPoweroff.as_str(), "hard-poweroff");
    }

    /// The unit name is the one the runner hands `systemd-run`, because nothing can ask it.
    #[test]
    fn the_unit_name_matches_the_launcher() {
        let runtime = Runtime::locate(
            std::path::Path::new("/run/user/1000/vivarium"),
            "api",
            "default",
        )
        .unwrap();
        assert_eq!(runtime.unit, "vivarium-api-default.service");
        assert_eq!(
            runtime.directory,
            std::path::Path::new("/run/user/1000/vivarium/api/default")
        );
    }

    #[test]
    fn control_socket_path_enforces_the_linux_sun_path_limit() {
        const SUFFIX: &str = "/api/default/control.sock";
        let root_of = |total: usize| {
            let root_len = total - SUFFIX.len();
            PathBuf::from(format!("/{}", "r".repeat(root_len - 1)))
        };
        assert!(Runtime::locate(&root_of(107), "api", "default").is_ok());
        let failure = Runtime::locate(&root_of(108), "api", "default")
            .err()
            .unwrap();
        assert_eq!(failure.code(), crate::exit::ExitKind::Config);
        assert!(format!("{failure:?}").contains("runtime-socket-path-too-long"));
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

    /// Every spelling the manager has for "no reading" is `None`; zero is a reading (spec/17).
    #[test]
    fn memory_current_collapses_unavailable_and_keeps_zero() {
        assert_eq!(parse_memory_current("2254857830"), Some(2_254_857_830));
        assert_eq!(parse_memory_current("0"), Some(0));
        assert_eq!(parse_memory_current(""), None);
        assert_eq!(parse_memory_current("[not set]"), None);
        assert_eq!(parse_memory_current("18446744073709551615"), None);
        assert_eq!(parse_memory_current("weather"), None);
    }

    /// The two `show` properties bind by key, not by line order, and empty values vanish.
    #[test]
    fn unit_memory_show_reads_by_key() {
        let shown = concat!(
            "ControlGroup=/user.slice/vivarium.slice/vivarium-api-default.service\n",
            "MemoryCurrent=1048576\n"
        );
        let (memory, cgroup) = parse_unit_memory_show(shown);
        assert_eq!(memory, Some(1_048_576));
        assert_eq!(
            cgroup.as_deref(),
            Some("/user.slice/vivarium.slice/vivarium-api-default.service")
        );
        assert_eq!(
            parse_unit_memory_show("MemoryCurrent=[not set]\nControlGroup=\n"),
            (None, None)
        );
    }

    /// The PSI parse takes the `some` line's `avg60` share and nothing else.
    #[test]
    fn pressure_parse_takes_some_avg60() {
        let pressure = concat!(
            "some avg10=0.00 avg60=0.40 avg300=0.12 total=417963\n",
            "full avg10=0.00 avg60=0.11 avg300=0.05 total=205931\n"
        );
        assert_eq!(parse_pressure_some_avg60(pressure), Some(0.40));
        assert_eq!(parse_pressure_some_avg60("full avg10=0 avg60=0.1\n"), None);
        assert_eq!(parse_pressure_some_avg60(""), None);
    }
}
