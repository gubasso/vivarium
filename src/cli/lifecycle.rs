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

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Context, Failure, Success, diagnosed};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;

/// The only target a project has today (spec/15).
pub(super) const DEFAULT_TARGET: &str = "default";

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
    /// the published causes — `crashed`, `boot-timeout`, `socket-lost` — can be told apart. Today
    /// every `failed` reads `crashed`, which is exact for a dead recorded process and approximate
    /// for the other conditions `classify` reaches `failed` from. Which reasons vivarium publishes,
    /// and whether an unreadable launch specification is one of them, is Q-015.
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
    // Signal 0 asks the kernel whether the process exists and whether we may signal it, and sends
    // nothing. `Errno::PERM` means it exists and belongs to someone else, which for a per-user
    // runtime directory should not happen — but it is still alive, so it counts as alive.
    match rustix::process::test_kill_process(
        rustix::process::Pid::from_raw(pid).unwrap_or(rustix::process::Pid::INIT),
    ) {
        Ok(()) => true,
        Err(errno) => errno == rustix::io::Errno::PERM,
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
        let evaluated = super::evaluate_resolved_for_launch(context, &project_id, resolved)?;
        let built = build_runner(&evaluated.flake_directory)?;
        write_build_record(
            &last_build_path(&context.roots, &project_id, DEFAULT_TARGET),
            &built,
        )?;
        // spec/04: the launch channel is read by pure evaluation of the *merged* configuration, so
        // a `vivarium.resources` a piece proposes is as binding as the manifest's own table. Taken
        // only on this path, because it is the only one that evaluated anything.
        merged = evaluated.resources;
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
    //
    // Written before the launcher, so a `status` racing this boot never reports a VM as fresh
    // against a build it was not launched from.
    write_build_record(
        &running_build_path(&context.roots, &project_id, DEFAULT_TARGET),
        &store_path,
    )?;
    execute_runner(
        context,
        &store_path,
        &runtime,
        &project_id,
        effective_resources(merged.or(leaf_resources).as_ref()),
    )?;

    Ok(Success::plain(String::new()))
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
/// to today. Q-015 owns the choice between this floor and a `failed` state that says the record is
/// unreadable.
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
    // supervisor removes the directory itself, so the honest test is that it is gone or empty.
    // An unreadable directory is not an empty one. Counting a failed inspection as zero would make
    // `stop` claim its post-condition at the one moment it cannot be established, so anything but
    // "gone" or "readable and empty" is the unconfirmed teardown spec/10 assigns the unavailable
    // code to.
    let residue = match std::fs::read_dir(&runtime.directory) {
        Ok(entries) => {
            let mut counted = 0;
            for entry in entries {
                entry.map_err(|source| unconfirmed_teardown(&runtime.directory, &source))?;
                counted += 1;
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
        host_mem_mib, host_vcpu, volume_directory,
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
