//! The fixture and assertion vocabulary every lane binary shares.
//!
//! Included per binary as `mod support;`, so each of the seven test binaries
//! compiles its own copy. No binary uses all of it — `local_workflows` never
//! touches the egress namespaces and `eval_workflows` never boots — which is why
//! the crate-level `dead_code` allowance below is here rather than a per-item
//! `#[allow]` nobody would keep true.

// Included per binary, and no binary uses every helper.
#![allow(dead_code)]

pub mod egress;
pub mod harness;
pub mod preflight;

use std::ffi::OsString;
use std::fs::{self, File, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use vivarium::exit::ExitKind;

// The categories trials currently assert, taken from the product's own `ExitKind` rather than
// respelled here, so a trial and the binary it runs cannot disagree about what a number means.
// `ExitStatus::code` yields `i32`, which is the only reason these are not `u8`.
//
// Only the reachable four are named. A trial that gains a way to provoke another category writes
// `ExitKind::TempFail.code()` at its assertion; an unused constant kept alive by a dead-code
// suppression would be a knob neutralized rather than absent.
pub const EX_USAGE: i32 = ExitKind::Usage.code() as i32;
pub const EX_DATAERR: i32 = ExitKind::DataErr.code() as i32;
pub const EX_CONFIG: i32 = ExitKind::Config.code() as i32;
pub const EX_IOERR: i32 = ExitKind::IoErr.code() as i32;
pub const EX_TEMPFAIL: i32 = ExitKind::TempFail.code() as i32;
pub const EX_UNAVAILABLE: i32 = ExitKind::Unavailable.code() as i32;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
/// Refuses the roots N24 refuses, and their ancestors, matching `reject_session_source` in
/// `src/launch/spec.rs` rather than guessing at it.
fn under_temp_root(path: &Path) -> bool {
    [Path::new("/tmp"), Path::new("/var/tmp")]
        .iter()
        .any(|forbidden| path.starts_with(forbidden) || forbidden.starts_with(path))
}

/// Where a fixture puts its project tree and its four durable XDG roots.
///
/// Deliberately not `std::env::temp_dir()`, and the reason is a product rule rather than a
/// preference. N24 refuses a share source that resolves under `/tmp` or `/var/tmp`, and the
/// project directory built on this base is exactly what a booting trial hands the launcher as
/// its workspace. With `TMPDIR` unset — the default on a host nobody has configured for this
/// suite — `temp_dir()` is `/tmp`, so a fixture defect surfaced wearing the product's own error
/// vocabulary: `vm.start-failed … share source violates N24`, from a spec line that was working
/// exactly as written. Measured on a host with `/dev/kvm`: all seven virtualization-gated trials
/// failed that way, and the same run passed 13 of 17 once `TMPDIR` named an external drive.
///
/// So the base is resolved rather than inherited, and the answer is the one
/// `tests/host/disk-preflight --locate --images` already gives for the same question, in the same
/// order — the configured drive when there is one, the state root otherwise, and never a temp
/// root. Only the environment variable is read here; the untracked shell file `disk-preflight`
/// also consults is a lane's concern, and this fallback is correct without it.
fn fixture_base() -> io::Result<PathBuf> {
    if let Some(drive) = std::env::var_os("VIVARIUM_HEAVY_DRIVE").filter(|value| !value.is_empty())
    {
        let drive = PathBuf::from(drive);
        // Validated rather than trusted: an unplugged drive looks exactly like an empty
        // directory, and a drive pointed at a temp root would reintroduce the refusal above.
        if drive.is_absolute() && !under_temp_root(&drive) && fs::create_dir_all(&drive).is_ok() {
            return Ok(drive.join("test-fixtures"));
        }
    }
    let state = match std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            io::Error::other("neither VIVARIUM_HEAVY_DRIVE, XDG_STATE_HOME, nor HOME is set")
        })?)
        .join(".local/state"),
    };
    if !state.is_absolute() || under_temp_root(&state) {
        return Err(io::Error::other(format!(
            "the state root `{}` cannot hold a fixture N24 will accept as a workspace",
            state.display()
        )));
    }
    Ok(state.join("vivarium/test-fixtures"))
}

#[derive(Debug)]
pub struct TempProject {
    root: PathBuf,
    project: PathBuf,
    token: String,
    home: PathBuf,
    config: PathBuf,
    state: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
    /// The lifetime locks of `root` and `runtime`, held until the fixture is dropped. See
    /// `sweep_abandoned_fixtures` for what they prove.
    lifetime: [File; 2],
}

impl TempProject {
    pub fn new() -> io::Result<Self> {
        Self::with_project_name("project")
    }

    pub fn with_project_name(name: &str) -> io::Result<Self> {
        sweep_abandoned_fixtures();
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = fixture_base()?.join(format!(
            "vivarium-user-workflows-{}-{sequence}",
            std::process::id()
        ));
        // The basename carries a per-process token so fixture paths remain distinct even when two
        // trials request the same descriptive stem.
        let token = format!("t{}x{sequence}", std::process::id());
        let basename = format!("{name}-{token}");
        let project = root.join(&basename);
        let home = root.join("home");
        let config = root.join("xdg-config");
        let state = root.join("xdg-state");
        let data = root.join("xdg-data");
        let cache = root.join("xdg-cache");
        // The runtime root sits under the session's real runtime directory rather than beside the
        // durable roots, and the reason is a hard limit rather than tidiness. A Unix socket path
        // cannot exceed 108 bytes, and the runtime layout spec/02 fixes already spends
        // `vivarium/<manifest>/<target>/workspace.sock` of it. Under `std::env::temp_dir()` the
        // remaining budget is whatever `TMPDIR` happens to be — and a `TMPDIR` on an external
        // drive, which is exactly what a disk-heavy lane sets, overruns it and fails the bind.
        // Measured: the failure reads as `vm.start-failed … path must be shorter than SUN_LEN`,
        // which looks like a product defect and is not one.
        //
        // `/run/user/<uid>` is short, is a per-user tmpfs, and is where runtime files genuinely
        // belong, so this is the layout being honest rather than a workaround.
        //
        // It falls back, and the fallback is what keeps the `local` lane's declared
        // need true. That lane says it needs the built `viv` and a filesystem, and a
        // login session's runtime directory is neither, so a developer whose
        // `/run/user/<uid>` does not exist would have had every `local` fixture fail
        // before its first assertion. Raised in review 2026-09-09.
        //
        // The fallback is `/tmp` rather than the fixture root, and that is the whole
        // point rather than convenience. What the roots above need from this path is
        // that it be short: measured 2026-09-09 by forcing this branch, a fixture
        // root on an external drive renders `control.sock` at 136 bytes and `viv
        // stop` refuses at `78` under `host.runtime-socket-path-too-long`, which is
        // the product reporting a fixture defect exactly as it should. `/tmp` is
        // twenty-odd bytes and is a tmpfs where runtime files belong.
        //
        // `/tmp` is refused elsewhere in this file and the two are not in tension.
        // N24 refuses it as a workspace source, because a share source there is a
        // user's tree on a filesystem that does not survive a reboot. A control
        // socket is the opposite kind of thing: it is meant not to survive one.
        let session = PathBuf::from(format!("/run/user/{}", vivarium::config::effective_uid()));
        let runtime = if session.is_dir() {
            session.join(format!("viv-t{}-{sequence}", std::process::id()))
        } else {
            PathBuf::from("/tmp").join(format!("viv-t{}-{sequence}", std::process::id()))
        };
        for path in [&project, &home, &config, &state, &data, &cache] {
            fs::create_dir_all(path)?;
        }
        // The runtime base is the one root with a mode requirement, and it is created
        // private and exclusively. ADR-0055 makes the privacy a precondition of
        // launching at all, and `config::resolve_runtime_root` refuses any base with
        // `mode & 0o077 != 0`, so a harness that skipped it would fail every launch
        // trial with `77` before the product path was reached.
        //
        // Both properties are load-bearing and neither is obvious. Setting the mode
        // after creating leaves the directory at the default mode for the instant
        // between the two calls, which under a permissive umask is long enough for
        // another user to write into it. And `recursive(true)` would accept a
        // directory that already exists, mode and owner untouched, so a name another
        // user created first would be adopted as this fixture's runtime root and
        // handed to the product as `XDG_RUNTIME_DIR`. The parent — `/run/user/<uid>`
        // checked above, or `/tmp` — always exists, so nothing needs creating
        // recursively. Both raised in review 2026-09-09, in successive rounds.
        create_private_directory(&runtime)?;
        bridge_user_manager(&runtime)?;
        let lifetime = [hold_lifetime_lock(&root)?, hold_lifetime_lock(&runtime)?];
        let project = project.canonicalize()?;
        Ok(Self {
            root,
            project,
            token,
            home,
            config,
            state,
            data,
            cache,
            runtime,
            lifetime,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    /// The per-fixture token appended to the project name.
    ///
    /// Used when a trial needs a second fixture path derived from the first.
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn state(&self) -> &Path {
        &self.state
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn cache(&self) -> &Path {
        &self.cache
    }

    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub fn env(&self) -> Vec<(OsString, OsString)> {
        [
            ("HOME", self.home()),
            ("XDG_CONFIG_HOME", self.config()),
            ("XDG_STATE_HOME", self.state()),
            ("XDG_DATA_HOME", self.data()),
            ("XDG_CACHE_HOME", self.cache()),
            ("XDG_RUNTIME_DIR", self.runtime()),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value.as_os_str().to_owned()))
        .collect()
    }
}

/// Points an isolated runtime root at the session's own systemd user manager.
///
/// ADR-0097 gives VM lifetime to a transient user unit, and `systemd-run --user` reaches the
/// manager through `$XDG_RUNTIME_DIR/systemd/private` — measured on a real host, and it ignores
/// `DBUS_SESSION_BUS_ADDRESS` for this, so passing that variable through does not help. A trial
/// that redirects `XDG_RUNTIME_DIR` for isolation therefore cuts the handoff unless it bridges
/// this one socket.
///
/// Only the socket is shared. Every file vivarium writes still lands in the temporary root, which
/// is what the isolation was for; what crosses is a connection to the manager that would own the
/// unit on a real user's machine anyway.
///
/// Absent on a host with no user manager, and that is not this function's failure to report: the
/// `Virtualization` gate is what decides whether such a host runs these trials at all.
fn bridge_user_manager(runtime: &Path) -> io::Result<()> {
    let session = PathBuf::from(format!("/run/user/{}", vivarium::config::effective_uid()))
        .join("systemd")
        .join("private");
    if !session.exists() {
        return Ok(());
    }
    let directory = runtime.join("systemd");
    fs::create_dir_all(&directory)?;
    let link = directory.join("private");
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(&session, &link)
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _result = stop_fixture_vms(Path::new("systemctl"), &self.runtime);
        let _result = fs::remove_dir_all(&self.root);
        // The runtime root is outside `root` on purpose (see above), so it is removed by name.
        let _result = fs::remove_dir_all(&self.runtime);
        // The lifetime locks are released after this body, when the fields drop, so a sweeper in
        // another process cannot claim a root this fixture is still removing.
    }
}

/// Stops any VM this fixture started, before its runtime directory is removed underneath it.
///
/// A trial that calls `viv start` and never `viv stop` leaves a live microVM: ADR-0097 hands
/// lifetime to a transient user unit, and that unit outlives the process that submitted it — which
/// is the whole point of a detached start. `systemd-run --collect` reaps only an *inactive* unit,
/// so nothing here reaps a running one.
///
/// Two things go wrong when it is left. The VM survives the trial, holding memory and a store
/// volume for as long as the session lives; and the next run of the same trial resolves the same
/// manifest name, so `systemd-run` refuses the name with "already loaded" and the trial fails for
/// a reason that has nothing to do with the product. Measured: the first run passed and every run
/// after it failed until the units were stopped by hand.
///
/// Units are selected by the runtime directory in their own command line, which is unique to this
/// fixture. Matching on the `vivarium-` name prefix alone would also match a VM the person running
/// the suite started for themselves. The match is by path component: a token of `ExecStart` that
/// is the runtime root or lies under it. A substring match would also select a unit of an unrelated
/// root whose name merely begins with this one, such as `viv-t123-1-personal` for `viv-t123-1`.
///
/// It reports rather than hides a failure to list, inspect, or stop. `Drop` ignores the report,
/// because a fixture's teardown must not fail the trial it served. The sweep does not: a root whose
/// units cannot be shown stopped is kept, so a running VM never loses its runtime directory.
fn stop_fixture_vms(systemctl: &Path, runtime: &Path) -> Result<(), String> {
    let listed = Command::new(systemctl)
        .args([
            "--user",
            "list-units",
            "--all",
            "--no-legend",
            "--plain",
            "--output=json",
            "vivarium-*",
        ])
        .output()
        .map_err(|error| format!("cannot list the user units: {error}"))?;
    if !listed.status.success() {
        return Err(format!("listing the user units exited {}", listed.status));
    }
    let units = serde_json::from_slice::<Vec<serde_json::Value>>(&listed.stdout)
        .map_err(|error| format!("cannot read the listed user units: {error}"))?;
    let root = runtime.to_string_lossy().into_owned();
    for unit in units {
        let Some(name) = unit.get("unit").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let shown = Command::new(systemctl)
            .args(["--user", "show", "-p", "ExecStart", "--value", name])
            .output()
            .map_err(|error| format!("cannot inspect {name}: {error}"))?;
        if !shown.status.success() {
            return Err(format!("inspecting {name} exited {}", shown.status));
        }
        let exec_start = String::from_utf8_lossy(&shown.stdout);
        let owned = names_path(&exec_start, &root);
        if !owned {
            continue;
        }
        let stopped = Command::new(systemctl)
            .args(["--user", "stop", name])
            .output()
            .map_err(|error| format!("cannot stop {name}: {error}"))?;
        if !stopped.status.success() {
            return Err(format!("stopping {name} exited {}", stopped.status));
        }
        let _ = Command::new(systemctl)
            .args(["--user", "reset-failed", name])
            .output();
    }
    Ok(())
}

/// Whether `text` names `path` or a path under it, as a whole path component.
///
/// The match is on the path's two ends rather than on whitespace tokens, so a fixture base that
/// contains a space still matches: before the path stands the start of the text, a space, or `=`,
/// and after it a `/`, a space, `;`, or the end. So `/run/user/1000/viv-t123-1-personal` is not
/// taken for `/run/user/1000/viv-t123-1`.
fn names_path(text: &str, path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    text.match_indices(path).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + path.len()..].chars().next();
        matches!(before, None | Some(' ' | '\t' | '='))
            && matches!(after, None | Some('/' | ' ' | '\t' | ';'))
    })
}

/// The file in each fixture root whose advisory lock the live fixture holds.
pub const LIFETIME_LOCK: &str = ".fixture-alive";

/// Create the lifetime lock of one fixture root and hold it.
///
/// `flock` belongs to the open file, and the kernel releases it when the last descriptor closes,
/// which a process does on exit by any path: a return, a panic, `SIGTERM`, or `SIGKILL`. So a lock
/// that another process can take proves that the fixture which held it is gone, and it proves that
/// across PID namespaces, where a bare process id cannot. Rust opens the file close-on-exec, so no
/// `viv` or VM that the fixture starts inherits it.
///
/// The lock is taken before the file gets its name. A sweeper only looks at a file named
/// `LIFETIME_LOCK`, so if the fixture created that name first and locked it after, a concurrent
/// sweeper could take the lock in between and remove a fixture that is still being built. The
/// file is created under a name no sweeper reads, locked, and then renamed into place, and a rename
/// keeps both the file and its lock.
fn hold_lifetime_lock(root: &Path) -> io::Result<File> {
    let staging = root.join(format!(
        "{LIFETIME_LOCK}.{}.{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    file.try_lock().map_err(|error| {
        io::Error::other(format!(
            "cannot hold the lifetime lock of {}: {error}",
            root.display()
        ))
    })?;
    fs::rename(&staging, root.join(LIFETIME_LOCK))?;
    Ok(file)
}

/// What the sweep did with one candidate root.
#[derive(Debug, PartialEq, Eq)]
pub enum Sweep {
    /// The root's fixture was gone, and the root is removed.
    Removed,
    /// A live fixture holds the root's lifetime lock.
    Alive,
    /// The root or its lock vanished first, which is what a concurrent sweeper leaves.
    Gone,
    /// The root stays, for the reason given. Nothing that cannot be shown abandoned is removed.
    Kept(String),
}

/// Remove the roots that fixtures of earlier processes left behind.
///
/// `Drop` removes both roots of a fixture when its trial ends, and that covers a return and a
/// panic. It cannot cover an end that never unwinds: nextest ends a trial at its `slow-timeout`
/// with `SIGTERM` and then `SIGKILL`, and a killed or interrupted run ends the same way. Those
/// ends left `viv-t<pid>-<n>` runtime roots under `/run/user/<uid>` and
/// `vivarium-user-workflows-<pid>-<n>` roots on the drive. A process that is killed cannot clean
/// up after itself, so the first fixture of each later process does it, and the guarantee is
/// exactly that: an abandoned root goes when a later process builds its first fixture.
///
/// A root is abandoned when its lifetime lock can be taken. The sweeper holds the lock while it
/// works, which claims the root against a concurrent sweeper, and it checks again after the claim
/// that the path still names the file it locked, so a root that another sweeper removed and a new
/// fixture recreated under the same name is never touched. A root with no lock, which is how a
/// root from before this change looks, is kept, and `tests/host/leftovers` reports it.
///
/// It runs once per process, and it never fails a fixture: a root it cannot remove is reported on
/// stderr and kept.
fn sweep_abandoned_fixtures() {
    static SWEPT: OnceLock<()> = OnceLock::new();
    SWEPT.get_or_init(|| {
        let systemctl = Path::new("systemctl");
        let session = PathBuf::from(format!("/run/user/{}", vivarium::config::effective_uid()));
        let mut outcomes = Vec::new();
        let runtime_parents = [session.as_path(), Path::new("/tmp")];
        for parent in runtime_parents {
            outcomes.extend(sweep_parent(parent, "viv-t", Some(systemctl)));
        }
        if let Ok(base) = fixture_base() {
            outcomes.extend(sweep_durable_parent(&base, &runtime_parents));
        }
        for (path, outcome) in outcomes {
            if let Sweep::Kept(reason) = outcome {
                eprintln!("fixture sweep kept {}: {reason}", path.display());
            }
        }
    });
}

/// Sweep one parent directory for fixture roots named `<prefix><pid>-<sequence>`.
///
/// Only failures are returned as `Kept`. A name of another form, and a root with no lifetime lock,
/// are skipped silently, because they are not this sweep's to judge.
pub fn sweep_parent(
    parent: &Path,
    prefix: &str,
    systemctl: Option<&Path>,
) -> Vec<(PathBuf, Sweep)> {
    sweep_matching(parent, prefix, |root, _| sweep_root(root, systemctl))
}

/// Sweep the durable fixture roots under `parent`, after the runtime roots are swept.
///
/// A durable root holds the workspace and the volume images of any VM its fixture started, and the
/// VM's unit names only the runtime root. So a durable root goes only when no runtime root of the
/// same `<pid>-<sequence>` remains in any of `runtime_parents`. A runtime root that the sweep kept,
/// because a unit could not be shown stopped, keeps its durable root with it.
pub fn sweep_durable_parent(parent: &Path, runtime_parents: &[&Path]) -> Vec<(PathBuf, Sweep)> {
    sweep_matching(parent, "vivarium-user-workflows-", |root, suffix| {
        let runtime = format!("viv-t{suffix}");
        if runtime_parents
            .iter()
            .any(|runtime_parent| fs::symlink_metadata(runtime_parent.join(&runtime)).is_ok())
        {
            return Sweep::Kept(format!("its runtime root {runtime} remains"));
        }
        sweep_root(root, None)
    })
}

/// Visit the fixture roots under `parent` that carry a lifetime lock, with their `<pid>-<n>`.
fn sweep_matching(
    parent: &Path,
    prefix: &str,
    mut visit: impl FnMut(&Path, &str) -> Sweep,
) -> Vec<(PathBuf, Sweep)> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut outcomes = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_fixture_name(name, prefix) {
            continue;
        }
        let path = entry.path();
        if !path.join(LIFETIME_LOCK).exists() {
            continue;
        }
        let suffix = &name[prefix.len()..];
        let outcome = visit(&path, suffix);
        outcomes.push((path, outcome));
    }
    outcomes
}

/// Whether `name` is exactly `<prefix><digits>-<digits>`.
fn is_fixture_name(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let Some((pid, sequence)) = rest.split_once('-') else {
        return false;
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    digits(pid) && digits(sequence)
}

/// Claim one root through its lifetime lock, stop its VMs, and remove it.
pub fn sweep_root(root: &Path, systemctl: Option<&Path>) -> Sweep {
    let Ok(metadata) = fs::symlink_metadata(root) else {
        return Sweep::Gone;
    };
    if !metadata.is_dir() || metadata.uid() != vivarium::config::effective_uid() {
        return Sweep::Kept("it is not a directory this user owns".to_owned());
    }
    let lock_path = root.join(LIFETIME_LOCK);
    let Ok(before) = fs::symlink_metadata(&lock_path) else {
        return Sweep::Gone;
    };
    if !before.is_file() {
        return Sweep::Kept("its lifetime lock is not a regular file".to_owned());
    }
    let lock = match File::open(&lock_path) {
        Ok(lock) => lock,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Sweep::Gone,
        Err(error) => return Sweep::Kept(format!("cannot open its lifetime lock: {error}")),
    };
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Sweep::Alive,
        Err(TryLockError::Error(error)) => {
            return Sweep::Kept(format!("cannot take its lifetime lock: {error}"));
        }
    }
    let Ok(held) = lock.metadata() else {
        return Sweep::Kept("cannot read the lifetime lock it holds".to_owned());
    };
    let same = |other: &fs::Metadata| other.dev() == held.dev() && other.ino() == held.ino();
    if !same(&before) || !fs::symlink_metadata(&lock_path).is_ok_and(|now| same(&now)) {
        return Sweep::Kept("its lifetime lock was replaced during the claim".to_owned());
    }
    if let Some(systemctl) = systemctl
        && let Err(reason) = stop_fixture_vms(systemctl, root)
    {
        return Sweep::Kept(reason);
    }
    // `remove_dir_all` removes a symbolic link inside the root, the `systemd/private` bridge
    // among them, and never follows it.
    match fs::remove_dir_all(root) {
        Ok(()) => Sweep::Removed,
        Err(error) => Sweep::Kept(format!("cannot remove it: {error}")),
    }
}

/// Create one directory, private from birth, refusing a path that already exists.
///
/// `DirBuilder` with an explicit mode rather than `create_dir` plus
/// `set_permissions`, so no instant passes at the default mode; and without
/// `recursive`, so an existing path is a refusal rather than something adopted.
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)
}

#[derive(Debug)]
pub struct VivOutput {
    pub argv: Vec<String>,
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The baseline flake inputs every lane evaluates against, pinned to local store paths.
///
/// vivarium ships branch references, so a released tool resolves today's upstream and pins it in
/// the lock the first evaluation creates. That default is a trap for this suite, which evaluates
/// from a fresh data root in trial after trial: each one queries the GitHub API, sixty of them
/// exhaust the anonymous limit, and the `ConfigEval` gate then closes for a reason that is about
/// GitHub rather than about vivarium. Worse, it closes silently — the trials skip and the run is
/// green.
///
/// So the lanes pin. The references come from this repository's own product flake, which already
/// locks the two, and `nix flake archive` reports where its inputs landed in the store: a `path:`
/// reference needs no network and no API at all, and it is the same revision vivarium is developed
/// against. Computed once per process and fails open — a host that cannot produce them evaluates
/// live, exactly as a user would.
fn baseline_inputs() -> &'static Vec<(OsString, OsString)> {
    static BASELINE: OnceLock<Vec<(OsString, OsString)>> = OnceLock::new();
    BASELINE.get_or_init(|| {
        let flake = format!("git+file://{}?dir=nix", env!("CARGO_MANIFEST_DIR"));
        let Ok(output) = Command::new("nix")
            .args([
                "flake",
                "archive",
                "--json",
                "--extra-experimental-features",
                "nix-command flakes",
                &flake,
            ])
            .output()
        else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        let Ok(archived) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
            return Vec::new();
        };
        [
            ("nixpkgs", "VIVARIUM_BASELINE_NIXPKGS"),
            ("microvm", "VIVARIUM_BASELINE_MICROVM"),
        ]
        .into_iter()
        .filter_map(|(input, variable)| {
            let path = archived.get("inputs")?.get(input)?.get("path")?.as_str()?;
            Some((
                OsString::from(variable),
                OsString::from(format!("path:{path}")), // nix-path-ok: an archived store path
            ))
        })
        .collect()
    })
}

/// The only host variables re-injected after `env_clear`. Everything else is dropped,
/// which is what makes the fail-closed assertions honest: an ambient `VIVARIUM_MANIFEST`
/// sits second in the manifest resolution order (spec/01), so a leaked one would silently
/// turn every expected `78` into a success.
const HOST_PASSTHROUGH: [&str; 6] = ["PATH", "TERM", "LANG", "LC_ALL", "SSL_CERT_FILE", "TMPDIR"];

/// The complete environment a `viv` under test runs with, as pairs.
///
/// Returned rather than applied so the one trial that cannot use [`run_viv`] — `viv shell`, which
/// needs a real pty and therefore a different spawner — configures its child from the same source
/// rather than from a second reading of these rules.
pub fn viv_environment(tp: &TempProject) -> Vec<(OsString, OsString)> {
    let mut environment: Vec<(OsString, OsString)> = HOST_PASSTHROUGH
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
        .collect();
    // `nix` needs its own ambient configuration to run at all; it carries no manifest
    // resolution meaning, so passing the family through is safe.
    environment
        .extend(std::env::vars_os().filter(|(name, _)| name.to_string_lossy().starts_with("NIX_")));
    environment.extend(tp.env());
    environment.extend(baseline_inputs().iter().cloned());
    environment
}

pub fn run_viv(bin: &Path, tp: &TempProject, cwd: &Path, args: &[&str]) -> io::Result<VivOutput> {
    run_viv_with_env(bin, tp, cwd, args, &[])
}

/// `run_viv` with named variables on top of the controlled set.
///
/// The controlled environment is the default for the reason `env_clear` states; this exists for
/// the trials whose subject IS a variable — a declared mount source that only expansion reveals
/// as refusable cannot be arranged out of the fixed set.
pub fn run_viv_with_env(
    bin: &Path,
    tp: &TempProject,
    cwd: &Path,
    args: &[&str],
    extra: &[(&str, &str)],
) -> io::Result<VivOutput> {
    let command_line = std::iter::once(bin.display().to_string())
        .chain(args.iter().map(|arg| (*arg).to_owned()))
        .collect();
    let mut command = Command::new(bin);
    command.args(args).current_dir(cwd).env_clear();
    command.envs(viv_environment(tp));
    command.envs(extra.iter().map(|(name, value)| (*name, *value)));
    let output = command.output()?;
    Ok(VivOutput {
        argv: command_line,
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub fn expect_code(out: &VivOutput, code: i32) -> Result<(), String> {
    if out.status.code() == Some(code) {
        return Ok(());
    }
    Err(diagnostic(out, &format!("expected exit code {code}")))
}

pub fn expect_stderr_mentions(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stderr).contains(needle) {
        return Ok(());
    }
    Err(diagnostic(
        out,
        &format!("expected stderr to mention {needle:?}"),
    ))
}

pub fn expect_stdout_mentions(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stdout).contains(needle) {
        return Ok(());
    }
    Err(diagnostic(
        out,
        &format!("expected stdout to mention {needle:?}"),
    ))
}

/// The negative half of `expect_stdout_mentions`. Provenance assertions need both:
/// a merged view must carry the winning value and must **not** carry the shadowed one.
pub fn expect_stdout_lacks(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stdout).contains(needle) {
        return Err(diagnostic(
            out,
            &format!("expected stdout to omit {needle:?}"),
        ));
    }
    Ok(())
}

/// Structural JSON checks, kept separate from the `VivOutput` wrapper for two reasons.
///
/// They parse rather than scan: the substring check these replace could not tell a
/// top-level key from the same text inside a value or from a nested one, so an
/// envelope assertion passed against output of the wrong shape. And taking raw bytes
/// rather than a `VivOutput` lets `harness_self_check` exercise them directly, without
/// fabricating a process exit status — which matters because every gated trial that
/// uses them is skipped until a real `viv` exists.
pub mod json {
    /// Names from `keys` that are absent from the top-level object.
    pub fn missing_keys(stdout: &[u8], keys: &[&str]) -> Result<Vec<String>, String> {
        missing_at_path(stdout, "", keys)
    }

    /// Names from `fields` absent from the object reached by the dot-separated `path`;
    /// an empty `path` addresses the top-level object. The specified envelopes nest —
    /// `config eval` puts the merged config under `config`, `manifest show` puts the
    /// knobs under `resources`/`egress` — so a top-level-only check would demand that a
    /// conforming implementation duplicate nested field names at the root.
    pub fn missing_at_path(
        stdout: &[u8],
        path: &str,
        fields: &[&str],
    ) -> Result<Vec<String>, String> {
        let value: serde_json::Value = serde_json::from_slice(stdout)
            .map_err(|error| format!("expected stdout to be JSON: {error}"))?;
        let mut cursor = &value;
        for segment in path.split('.').filter(|segment| !segment.is_empty()) {
            cursor = cursor
                .get(segment)
                .ok_or_else(|| format!("expected an object at {path:?}: {segment:?} is absent"))?;
        }
        let object = cursor.as_object().ok_or_else(|| {
            if path.is_empty() {
                "expected a JSON object at the top level".to_owned()
            } else {
                format!("expected a JSON object at {path:?}")
            }
        })?;
        Ok(fields
            .iter()
            .filter(|field| !object.contains_key(**field))
            .map(|field| {
                if path.is_empty() {
                    (*field).to_owned()
                } else {
                    format!("{path}.{field}")
                }
            })
            .collect())
    }

    /// Fields absent from the member objects of the **map** under `key`, reported as
    /// `key[<member>].field`. `config sources` keys `values` by option path rather than
    /// listing it, so its per-key record cannot be reached by index.
    pub fn map_entries_missing(
        stdout: &[u8],
        key: &str,
        fields: &[&str],
    ) -> Result<Vec<String>, String> {
        let value: serde_json::Value = serde_json::from_slice(stdout)
            .map_err(|error| format!("expected stdout to be JSON: {error}"))?;
        let entries = value
            .get(key)
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("expected a top-level {key:?} object"))?;
        let mut missing = Vec::new();
        for (member, entry) in entries {
            let Some(object) = entry.as_object() else {
                missing.push(format!("{key}[{member:?}] is not an object"));
                continue;
            };
            for field in fields {
                if !object.contains_key(*field) {
                    missing.push(format!("{key}[{member:?}].{field}"));
                }
            }
        }
        Ok(missing)
    }

    /// The number of items in the array under `key`.
    pub fn array_len(stdout: &[u8], key: &str) -> Result<usize, String> {
        let value: serde_json::Value = serde_json::from_slice(stdout)
            .map_err(|error| format!("expected stdout to be JSON: {error}"))?;
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .ok_or_else(|| format!("expected a top-level {key:?} array"))
    }

    /// The value of a top-level string field, or `None` when it is absent or not a string.
    pub fn string_field(stdout: &[u8], key: &str) -> Result<Option<String>, String> {
        let value: serde_json::Value = serde_json::from_slice(stdout)
            .map_err(|error| format!("expected stdout to be JSON: {error}"))?;
        Ok(value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned))
    }

    /// Fields absent from the objects in the array under `key`, reported as `key[i].field`.
    /// An empty array passes: an envelope is still correct when it has nothing to carry.
    pub fn array_items_missing(
        stdout: &[u8],
        key: &str,
        fields: &[&str],
    ) -> Result<Vec<String>, String> {
        let value: serde_json::Value = serde_json::from_slice(stdout)
            .map_err(|error| format!("expected stdout to be JSON: {error}"))?;
        let items = value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("expected a top-level {key:?} array"))?;
        let mut missing = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let Some(object) = item.as_object() else {
                missing.push(format!("{key}[{index}] is not an object"));
                continue;
            };
            for field in fields {
                if !object.contains_key(*field) {
                    missing.push(format!("{key}[{index}].{field}"));
                }
            }
        }
        Ok(missing)
    }
}

pub fn expect_json_keys(out: &VivOutput, keys: &[&str]) -> Result<(), String> {
    match json::missing_keys(&out.stdout, keys) {
        Err(problem) => Err(diagnostic(out, &problem)),
        Ok(missing) if missing.is_empty() => Ok(()),
        Ok(missing) => Err(diagnostic(
            out,
            &format!("missing top-level JSON keys: {}", missing.join(", ")),
        )),
    }
}

/// Assert the object at a dot-separated `path` carries `fields`. The counterpart to
/// `expect_json_keys` for the nested halves of the specified envelopes (spec/01).
pub fn expect_json_fields_at(out: &VivOutput, path: &str, fields: &[&str]) -> Result<(), String> {
    match json::missing_at_path(&out.stdout, path, fields) {
        Err(problem) => Err(diagnostic(out, &problem)),
        Ok(missing) if missing.is_empty() => Ok(()),
        Ok(missing) => Err(diagnostic(
            out,
            &format!("missing JSON fields: {}", missing.join(", ")),
        )),
    }
}

/// Assert every member of the map under `key` carries `fields` — the `values` shape
/// `viv config sources` emits, keyed by option path (spec/01).
pub fn expect_json_map_entries(out: &VivOutput, key: &str, fields: &[&str]) -> Result<(), String> {
    match json::map_entries_missing(&out.stdout, key, fields) {
        Err(problem) => Err(diagnostic(out, &problem)),
        Ok(missing) if missing.is_empty() => Ok(()),
        Ok(missing) => Err(diagnostic(
            out,
            &format!("missing JSON fields: {}", missing.join(", ")),
        )),
    }
}

/// Assert the array under `key` carries **at least one** record, each with `fields`.
/// Distinct from `expect_json_array_items`, which passes on an empty array: a defect
/// report that merely has the key would otherwise be satisfied by `"conflicts": []`,
/// letting an implementation that never encodes the defect pass (ADR-0042).
pub fn expect_json_array_nonempty(
    out: &VivOutput,
    key: &str,
    fields: &[&str],
) -> Result<(), String> {
    match json::array_len(&out.stdout, key) {
        Err(problem) => return Err(diagnostic(out, &problem)),
        Ok(0) => {
            return Err(diagnostic(
                out,
                &format!("expected at least one record under {key:?}"),
            ));
        }
        Ok(_) => {}
    }
    expect_json_array_items(out, key, fields)
}

/// Assert a top-level string field holds exactly `expected`. Structural, so a value
/// that merely *contains* the word somewhere else in the record cannot satisfy it.
pub fn expect_json_string(out: &VivOutput, key: &str, expected: &str) -> Result<(), String> {
    match json::string_field(&out.stdout, key) {
        Err(problem) => Err(diagnostic(out, &problem)),
        Ok(Some(actual)) if actual == expected => Ok(()),
        Ok(Some(actual)) => Err(diagnostic(
            out,
            &format!("expected {key:?} to be {expected:?}, got {actual:?}"),
        )),
        Ok(None) => Err(diagnostic(
            out,
            &format!("expected a top-level string field {key:?}"),
        )),
    }
}

/// Assert the `{ "<key>": [ { … } ] }` envelope shape the library and project-state
/// readers share (spec/01), rather than grepping for field names in the raw text.
pub fn expect_json_array_items(out: &VivOutput, key: &str, fields: &[&str]) -> Result<(), String> {
    match json::array_items_missing(&out.stdout, key, fields) {
        Err(problem) => Err(diagnostic(out, &problem)),
        Ok(missing) if missing.is_empty() => Ok(()),
        Ok(missing) => Err(diagnostic(
            out,
            &format!("missing JSON fields: {}", missing.join(", ")),
        )),
    }
}

pub fn expect_nonzero(out: &VivOutput) -> Result<(), String> {
    if out.status.code().is_some_and(|code| code != 0) {
        return Ok(());
    }
    Err(diagnostic(out, "expected a non-zero exit status"))
}

pub fn write_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeSnapshot(Vec<(PathBuf, Option<Vec<u8>>)>);

pub fn snapshot_tree(root: &Path) -> io::Result<TreeSnapshot> {
    fn visit(
        base: &Path,
        current: &Path,
        entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
    ) -> io::Result<()> {
        let mut children = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let relative = path
                .strip_prefix(base)
                .map_err(io::Error::other)?
                .to_path_buf();
            if path.is_dir() {
                entries.push((relative, None));
                visit(base, &path, entries)?;
            } else {
                entries.push((relative, Some(fs::read(path)?)));
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries)?;
    Ok(TreeSnapshot(entries))
}

pub fn expect_tree_unchanged(root: &Path, before: &TreeSnapshot) -> Result<(), String> {
    let after = snapshot_tree(root).map_err(|error| error.to_string())?;
    if &after == before {
        Ok(())
    } else {
        Err(format!(
            "project tree changed\nbefore: {before:?}\nafter: {after:?}"
        ))
    }
}

pub fn expect_derived_manifest_visible(out: &VivOutput, manifest: &str) -> Result<(), String> {
    expect_code(out, 0)?;
    // spec/01 "Config inspection output" nests the four roots under `paths`, so only
    // three keys are top-level. Asserting all seven at the top level would demand that
    // a conforming implementation duplicate the nested names at the root — the hazard
    // `missing_at_path` exists to avoid.
    expect_json_keys(out, &["manifest", "source", "paths"])?;
    expect_json_fields_at(out, "paths", &["config", "state", "data", "cache"])?;
    expect_stdout_mentions(out, manifest)?;
    // Every call site reads with no override and a cleared environment, so explicit workspace
    // ownership is the only source that can produce this result.
    expect_json_string(out, "source", "derived")
}

pub fn expect_no_volume_images(tp: &TempProject) -> Result<(), String> {
    let state = tp.state().join("vivarium").join("projects");
    if !state.exists() {
        return Ok(());
    }
    let mut pending = vec![state];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let child = entry.path();
            if child.is_dir() {
                pending.push(child);
            } else if child
                .extension()
                .is_some_and(|extension| extension == "img")
            {
                return Err(format!("read-only command created {}", child.display()));
            }
        }
    }
    Ok(())
}

/// Where a volume image of this manifest-keyed sandbox lands under its state root.
pub fn volume_image(tp: &TempProject, manifest: &str, name: &str) -> PathBuf {
    tp.state()
        .join("vivarium")
        .join("projects")
        .join(manifest)
        .join("default")
        .join("volumes")
        .join(format!("{name}.img"))
}

pub fn expect_volume_image(tp: &TempProject, manifest: &str, name: &str) -> Result<(), String> {
    let path = volume_image(tp, manifest, name);
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("volume image is absent: {}", path.display()))
    }
}

pub fn diagnostic(out: &VivOutput, message: &str) -> String {
    format!(
        "{message}\nargv: {:?}\nexit: {:?}\nstdout: {}\nstderr: {}",
        out.argv,
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// --- The shared trial vocabulary -------------------------------------------
//
// Everything below was `tests/user_workflows.rs`'s own until that binary was
// split into the `local`, `eval` and `boot` lanes. What landed here is what two
// or more of those three still call: a fixture arrangement, a record reader, or
// one of the three `Failed` adapters. A helper only one lane uses stays in that
// lane's file, where its reader can see who wants it.

use libtest_mimic::Failed;

pub const VOLUME_TAIL: &str = "\n[[volumes]]\nname = \"cache\"\nmount = \"/var/cache/project\"\n";

pub fn arrange_egress_fixture() -> Result<TempProject, Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    // The image supplies the shadowed default; only the piece forces the allowlist, so
    // the precedence under test is genuinely arranged rather than pre-decided by the
    // manifest. Each layer contributes a distinct host to make concatenation
    // observable — the manifest the reachable endpoint's name, the piece the name
    // whose upstream answer is NXDOMAIN.
    arrange_manifest_with_image(
        &tp,
        "restricted",
        "pieces = [ \"egress-restriction\" ]\n",
        &format!("\n[egress]\nallow = [ \"{}\" ]\n", egress::ALLOWED_NAME),
        "{ lib, ... }: { sandbox.egress.mode = lib.mkDefault \"open\"; }\n",
    )?;
    write_piece(
        &tp,
        "egress-restriction",
        &format!(
            "{{ lib, ... }}: {{\n    sandbox.egress.mode = lib.mkForce \"allowlist\";\n    \
            sandbox.egress.allow = [ \"{}\" ];\n}}\n",
            egress::GHOST_NAME
        ),
    )?;
    Ok(tp)
}

/// One `--json` record parsed whole, for the value assertions the key helpers cannot make.
pub fn json_record(out: &VivOutput) -> Result<serde_json::Value, Failed> {
    serde_json::from_slice(&out.stdout)
        .map_err(|error| Failed::from(format!("stdout was not one JSON record: {error}")))
}

/// The parsed rows under a record's `projects` key, in published order — `status -g` and the
/// `stop --all` sweep share the wrapper (spec/01).
pub fn project_rows(out: &VivOutput) -> Result<Vec<serde_json::Value>, Failed> {
    json_record(out)?["projects"]
        .as_array()
        .cloned()
        .ok_or_else(|| Failed::from("the record published no `projects` array"))
}

pub fn viv_with_env(
    tp: &TempProject,
    args: &[&str],
    extra: &[(&str, &str)],
) -> Result<VivOutput, Failed> {
    run_viv_with_env(preflight::viv(), tp, tp.project(), args, extra).map_err(io_failed)
}

pub fn arrange_manifest(
    tp: &TempProject,
    name: &str,
    manifest_body: &str,
    manifest_tail: &str,
) -> Result<(), Failed> {
    arrange_manifest_with_image(tp, name, manifest_body, manifest_tail, "{ ... }: { }\n")
}

pub fn arrange_manifest_with_image(
    tp: &TempProject,
    name: &str,
    manifest_body: &str,
    manifest_tail: &str,
    image_body: &str,
) -> Result<(), Failed> {
    let workspace =
        if manifest_body.contains("[[workspaces]]") || manifest_tail.contains("[[workspaces]]") {
            String::new()
        } else {
            format!("\n[[workspaces]]\nsource = '{}'\n", tp.project().display())
        };
    write_file(
        &tp.config()
            .join("vivarium")
            .join("images")
            .join("minimal.nix"),
        image_body,
    )
    .map_err(io_failed)?;
    write_file(
        &tp.config()
            .join("vivarium")
            .join("manifests")
            .join(format!("{name}.toml")),
        &format!("image = \"minimal\"\n{manifest_body}{manifest_tail}{workspace}"),
    )
    .map_err(io_failed)
}

pub fn write_piece(tp: &TempProject, name: &str, contents: &str) -> Result<(), Failed> {
    write_file(
        &tp.config()
            .join("vivarium")
            .join("pieces")
            .join(format!("{name}.nix")),
        contents,
    )
    .map_err(io_failed)
}

pub fn viv(tp: &TempProject, args: &[&str]) -> Result<VivOutput, Failed> {
    run_viv(preflight::viv(), tp, tp.project(), args).map_err(io_failed)
}

pub fn viv_at(tp: &TempProject, cwd: &Path, args: &[&str]) -> Result<VivOutput, Failed> {
    run_viv(preflight::viv(), tp, cwd, args).map_err(io_failed)
}

pub fn check(result: Result<(), String>) -> Result<(), Failed> {
    result.map_err(Failed::from)
}

// Consumes the error rather than stringifying a borrow, which is what keeps
// `clippy::needless_pass_by_value` satisfied without an explicit `drop`.
pub fn io_failed(error: std::io::Error) -> Failed {
    Failed::from(error)
}

pub fn fail<T>(message: impl Into<String>) -> Result<T, Failed> {
    Err(Failed::from(message.into()))
}

/// That a named check is present and soft, so it can never make doctor refuse.
///
/// Severity is the mechanism, not a detail: only a hard probe carries an exit
/// code (spec/13), so a soft finding cannot turn a condition into `78` however
/// the rest of the host reads. A trial asserting that doctor reports rather than
/// refuses is asserting this, and asserting it here rather than through the
/// process exit code is what keeps the claim about the finding instead of about
/// the machine.
pub fn expect_soft_check(out: &VivOutput, id: &str) -> Result<(), String> {
    let check = doctor_check(out, id).map_err(|error| format!("{error:?}"))?;
    let severity = check["severity"].as_str().unwrap_or("absent");
    if severity == "soft" {
        return Ok(());
    }
    Err(format!(
        "the `{id}` check is `{severity}`, so doctor can refuse on it"
    ))
}

/// One named check out of a `viv doctor --json` report.
///
/// Reaching for a check by id is what lets a trial assert on the report without
/// asserting on the host. `viv doctor` exits `78` when any hard probe fails, and
/// a probe about this machine's virtualization support has nothing to do with a
/// trial about how an ownership finding is worded. Measured 2026-09-09 on a
/// GitHub runner, where `host-userns-available` trips and took two trials with
/// it, each failing for a reason it was not testing.
pub fn doctor_check(out: &VivOutput, id: &str) -> Result<serde_json::Value, Failed> {
    let record = json_record(out)?;
    record["checks"]
        .as_array()
        .ok_or_else(|| Failed::from("the doctor report published no `checks` array"))?
        .iter()
        .find(|check| check["id"] == serde_json::json!(id))
        .cloned()
        .ok_or_else(|| Failed::from(format!("the doctor report carries no `{id}` check")))
}
