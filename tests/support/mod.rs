use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use vivarium::exit::ExitKind;

// The categories trials currently assert, taken from the product's own `ExitKind` rather than
// respelled here, so a trial and the binary it runs cannot disagree about what a number means.
// `ExitStatus::code` yields `i32`, which is the only reason these are not `u8`.
//
// Only the reachable three are named. A trial that gains a way to provoke another category writes
// `ExitKind::TempFail.code()` at its assertion; an unused constant kept alive by a dead-code
// suppression would be a knob neutralized rather than absent.
pub const EX_USAGE: i32 = ExitKind::Usage.code() as i32;
pub const EX_DATAERR: i32 = ExitKind::DataErr.code() as i32;
pub const EX_CONFIG: i32 = ExitKind::Config.code() as i32;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
static GATE: OnceLock<GateDecision> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub enum GateLevel {
    Cli,
    ConfigEval,
    Virtualization,
}

#[derive(Debug)]
pub struct GateDecision {
    viv: PathBuf,
    cli: Result<(), String>,
    config_eval: Result<(), String>,
    virtualization: Result<(), String>,
}

impl GateDecision {
    pub fn viv(&self) -> &Path {
        &self.viv
    }

    pub const fn result(&self, level: GateLevel) -> &Result<(), String> {
        match level {
            GateLevel::Cli => &self.cli,
            GateLevel::ConfigEval => &self.config_eval,
            GateLevel::Virtualization => &self.virtualization,
        }
    }

    pub fn is_well_formed(&self) -> bool {
        !self.viv.as_os_str().is_empty()
            && self
                .result(GateLevel::Cli)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
            && self
                .result(GateLevel::ConfigEval)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
            && self
                .result(GateLevel::Virtualization)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
    }
}

pub fn gate() -> &'static GateDecision {
    GATE.get_or_init(probe_gate)
}

fn probe_gate() -> GateDecision {
    let viv = resolve_viv();
    let cli = probe_cli(&viv);
    let config_eval = cli
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|()| probe_nix())
        .and_then(|()| probe_config_eval(&viv));
    let virtualization = config_eval
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|()| probe_kvm());

    GateDecision {
        viv,
        cli,
        config_eval,
        virtualization,
    }
}

fn resolve_viv() -> PathBuf {
    if let Some(path) = std::env::var_os("VIVARIUM_TEST_VIV") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_viv") {
        return PathBuf::from(path);
    }

    // `CARGO_TARGET_DIR` before the repository-local `target/`, and not the other way around. The
    // dev shell sets that variable, so a repository-local `target/debug/viv` is a leftover from
    // before it did — and a gate that probed one of those would describe a binary nobody is
    // building. Reached only when neither env var above is set, which is how a runner that lists
    // the trials outside `cargo test` arrives here.
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("target"),
        PathBuf::from,
    );
    let local = target
        .join("debug")
        .join(format!("viv{}", std::env::consts::EXE_SUFFIX));
    if local.is_file() {
        return local;
    }

    PathBuf::from(format!("viv{}", std::env::consts::EXE_SUFFIX))
}

fn probe_cli(viv: &Path) -> Result<(), String> {
    let tp = TempProject::new().map_err(|error| format!("cannot isolate CLI probe: {error}"))?;
    let out = run_viv(viv, &tp, tp.project(), &["manifest", "list", "--json"])
        .map_err(|error| format!("cannot execute {}: {error}", viv.display()))?;
    if out.status.code() != Some(0) {
        return Err(format!(
            "{} manifest probe exited {:?}",
            viv.display(),
            out.status.code()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.contains("\"manifests\"") {
        return Err(format!(
            "{} manifest probe did not emit the required manifests key",
            viv.display()
        ));
    }
    Ok(())
}

fn probe_nix() -> Result<(), String> {
    match Command::new("nix").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!("nix --version exited {:?}", output.status.code())),
        Err(error) => Err(format!("nix --version is unavailable: {error}")),
    }
}

/// Whether this binary can evaluate configuration at all, separately from whether nix is installed.
///
/// `probe_nix` answers a question about the host. This answers the matching one about the product,
/// and the level needs both: a host with nix and a `viv` whose `config eval` does not exist yet
/// would otherwise open a gate named for an ability nothing has. That is the vacuous-check failure
/// the harness method note warns about — the probe passed, and every trial behind it failed for a
/// reason the gate was supposed to describe.
///
/// Probed from an unbound project, so the two answers separate cleanly: `78` is the fail-closed
/// path of an implemented verb, and anything else means the verb is not there to fail closed.
///
/// That half is necessary and not sufficient. The verb existing says nothing about whether this
/// host can reach the flake inputs an evaluation resolves, and a gate that opened on the strength
/// of a `78` would send every trial behind it into a failure the gate was supposed to describe.
/// So the second half evaluates a real bound manifest end to end. It is the expensive probe on
/// purpose: nothing cheaper distinguishes "cannot evaluate" from "evaluates wrongly", and those
/// two must not arrive as the same red.
fn probe_config_eval(viv: &Path) -> Result<(), String> {
    let tp =
        TempProject::new().map_err(|error| format!("cannot isolate config-eval probe: {error}"))?;
    let out = run_viv(viv, &tp, tp.project(), &["config", "eval", "--json"])
        .map_err(|error| format!("cannot execute {}: {error}", viv.display()))?;
    if out.status.code() != Some(EX_CONFIG) {
        return Err(format!(
            "{} config eval is not implemented (unbound probe exited {:?}, expected {EX_CONFIG})",
            viv.display(),
            out.status.code()
        ));
    }
    probe_evaluation(viv, &tp)
}

/// Whether this host can actually evaluate a bound manifest.
fn probe_evaluation(viv: &Path, tp: &TempProject) -> Result<(), String> {
    let library = tp.config().join("vivarium");
    write_file(&library.join("images").join("probe.nix"), "{ ... }: { }\n")
        .and_then(|()| {
            write_file(
                &library.join("manifests").join("probe.toml"),
                "image = \"probe\"\n",
            )
        })
        .map_err(|error| format!("cannot write the evaluation probe fixture: {error}"))?;
    let bind = run_viv(
        viv,
        tp,
        tp.project(),
        &["init", "--manifest", "probe", "--write", "--yes"],
    )
    .map_err(|error| format!("cannot execute {}: {error}", viv.display()))?;
    if bind.status.code() != Some(0) {
        return Err(format!(
            "the evaluation probe could not bind its manifest (exited {:?}): {}",
            bind.status.code(),
            String::from_utf8_lossy(&bind.stderr).trim()
        ));
    }
    let evaluated = run_viv(viv, tp, tp.project(), &["config", "eval", "--json"])
        .map_err(|error| format!("cannot execute {}: {error}", viv.display()))?;
    if evaluated.status.code() == Some(0) {
        return Ok(());
    }
    Err(format!(
        "this host cannot evaluate a bound manifest (exited {:?}): {}",
        evaluated.status.code(),
        String::from_utf8_lossy(&evaluated.stderr).trim()
    ))
}

fn probe_kvm() -> Result<(), String> {
    let path = Path::new("/dev/kvm");
    if !path.exists() {
        return Err("/dev/kvm is absent".to_owned());
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map(drop)
        .map_err(|error| format!("/dev/kvm is not read-write openable: {error}"))
}

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
    basename: String,
    token: String,
    home: PathBuf,
    config: PathBuf,
    state: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
}

impl TempProject {
    pub fn new() -> io::Result<Self> {
        Self::with_project_name("project")
    }

    pub fn with_project_name(name: &str) -> io::Result<Self> {
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = fixture_base()?.join(format!(
            "vivarium-user-workflows-{}-{sequence}",
            std::process::id()
        ));
        // The project's own basename carries the fixture token, because the project id derived from
        // it reaches a namespace no temporary root can isolate. Every other root here is a path
        // this fixture chooses, but `vivarium-<project-id>-<target>.service` is a name in the
        // session's systemd user manager, shared by every trial in the run. Two fixtures built from
        // the same name mint the same id in their own state roots, believe it unique, and then
        // collide on that one unit name — so a parallel run fails with "already loaded" for a
        // reason that has nothing to do with the product. Isolating the durable roots and leaving
        // the basename fixed is isolation that is total everywhere except the one place it is
        // observable from outside.
        let token = format!("t{}x{sequence}", std::process::id());
        let basename = format!("{name}-{token}");
        // The token is only load-bearing if it survives into the id, and spec/15 truncates a
        // sanitized name to 48 characters. Checked through the product's own sanitizer rather than
        // by respelling the cap, so a fixture whose name grew too long fails here by name instead
        // of silently sharing a truncated prefix with its neighbour.
        if !vivarium::config::sanitize_project_name(&basename).ends_with(&token) {
            return Err(io::Error::other(format!(
                "the fixture name `{basename}` is too long to carry its uniqueness token into the \
                project id"
            )));
        }
        let project = root.join(&basename);
        let home = root.join("home");
        let config = root.join("xdg-config");
        let state = root.join("xdg-state");
        let data = root.join("xdg-data");
        let cache = root.join("xdg-cache");
        // The runtime root sits under the session's real runtime directory rather than beside the
        // durable roots, and the reason is a hard limit rather than tidiness. A Unix socket path
        // cannot exceed 108 bytes, and the runtime layout spec/02 fixes already spends
        // `vivarium/<project-id>/<target>/workspace.sock` of it. Under `std::env::temp_dir()` the
        // remaining budget is whatever `TMPDIR` happens to be — and a `TMPDIR` on an external
        // drive, which is exactly what a disk-heavy lane sets, overruns it and fails the bind.
        // Measured: the failure reads as `vm.start-failed … path must be shorter than SUN_LEN`,
        // which looks like a product defect and is not one.
        //
        // `/run/user/<uid>` is short, is a per-user tmpfs, and is where runtime files genuinely
        // belong, so this is the layout being honest rather than a workaround.
        let runtime = PathBuf::from(format!("/run/user/{}", vivarium::config::effective_uid()))
            .join(format!("viv-t{}-{sequence}", std::process::id()));
        for path in [&project, &home, &config, &state, &data, &cache, &runtime] {
            fs::create_dir_all(path)?;
        }
        // The runtime base is the one root with a mode requirement. ADR-0055 makes it a
        // precondition of launching at all, and `config::resolve_runtime_root` refuses any base
        // with `mode & 0o077 != 0`. `create_dir_all` yields `0755` under the usual umask, so a
        // harness that skipped this would fail every launch trial with `77` before the product
        // path was reached — a precondition the product has always had, learned here.
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        bridge_user_manager(&runtime)?;
        let project = project.canonicalize()?;
        Ok(Self {
            root,
            project,
            basename,
            token,
            home,
            config,
            state,
            data,
            cache,
            runtime,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    /// The project directory's own name, which is what identity sanitizes into the project id.
    ///
    /// A trial that builds a second project meant to collide with this one takes its name from
    /// here: sharing the token is what makes the two resolve the same base and exercise the
    /// smallest-free-suffix rule.
    pub fn basename(&self) -> &str {
        &self.basename
    }

    /// The project id vivarium derives from this fixture's directory name.
    ///
    /// For locating an artifact whose path contains the id, never for asserting the id itself: it
    /// is computed with the product's own sanitizer, so an assertion written against it would hold
    /// for any sanitizer at all. A trial checking what the id came out to spells the stem it
    /// expects and appends [`TempProject::token`].
    pub fn project_id(&self) -> String {
        vivarium::config::sanitize_project_name(&self.basename)
    }

    /// The per-fixture token appended to the project name.
    ///
    /// A trial asserting a derived id spells the stem it expects and appends this, so the
    /// sanitizer's own mapping stays asserted rather than recomputed from the product.
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
        stop_leaked_vms(&self.runtime);
        let _result = fs::remove_dir_all(&self.root);
        // The runtime root is outside `root` on purpose (see above), so it is removed by name.
        let _result = fs::remove_dir_all(&self.runtime);
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
/// `<project-id>`, so `systemd-run` refuses the name with "already loaded" and the trial fails for
/// a reason that has nothing to do with the product. Measured: the first run passed and every run
/// after it failed until the units were stopped by hand.
///
/// Units are selected by the runtime directory in their own command line, which is unique to this
/// fixture. Matching on the `vivarium-` name prefix alone would also match a VM the person running
/// the suite started for themselves.
fn stop_leaked_vms(runtime: &Path) {
    let Ok(listed) = Command::new("systemctl")
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
    else {
        return;
    };
    let Ok(units) = serde_json::from_slice::<Vec<serde_json::Value>>(&listed.stdout) else {
        return;
    };
    let needle = runtime.to_string_lossy().into_owned();
    for unit in units {
        let Some(name) = unit.get("unit").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let owned = Command::new("systemctl")
            .args(["--user", "show", "-p", "ExecStart", "--value", name])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&needle));
        if owned {
            let _ = Command::new("systemctl")
                .args(["--user", "stop", name])
                .output();
            let _ = Command::new("systemctl")
                .args(["--user", "reset-failed", name])
                .output();
        }
    }
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
        let flake = format!("path:{}?dir=nix", env!("CARGO_MANIFEST_DIR"));
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
        let mut pins: Vec<(OsString, OsString)> = [
            ("nixpkgs", "VIVARIUM_BASELINE_NIXPKGS"),
            ("microvm", "VIVARIUM_BASELINE_MICROVM"),
        ]
        .into_iter()
        .filter_map(|(input, variable)| {
            let path = archived.get("inputs")?.get(input)?.get("path")?.as_str()?;
            Some((
                OsString::from(variable),
                OsString::from(format!("path:{path}")),
            ))
        })
        .collect();
        // The third baseline input is this repository's own product flake, which carries the guest
        // module and the launch seam a generated flake composes. Pinned to the working tree rather
        // than to the archived store path, because a trial must exercise the tree under test — a
        // store snapshot would silently measure an older product. Its shipped default names a
        // branch, so
        // leaving it unset here would resolve upstream on every evaluation, which is the network
        // dependence the other two pins exist to avoid.
        pins.push((
            OsString::from("VIVARIUM_BASELINE_VIVARIUM"),
            OsString::from(flake),
        ));
        pins
    })
}

/// The only host variables re-injected after `env_clear`. Everything else is dropped,
/// which is what makes the fail-closed assertions honest: an ambient `VIVARIUM_MANIFEST`
/// sits second in the manifest resolution order (spec/01), so a leaked one would silently
/// turn every expected `78` into a success.
const HOST_PASSTHROUGH: [&str; 6] = ["PATH", "TERM", "LANG", "LC_ALL", "SSL_CERT_FILE", "TMPDIR"];

pub fn run_viv(bin: &Path, tp: &TempProject, cwd: &Path, args: &[&str]) -> io::Result<VivOutput> {
    let command_line = std::iter::once(bin.display().to_string())
        .chain(args.iter().map(|arg| (*arg).to_owned()))
        .collect();
    let mut command = Command::new(bin);
    command.args(args).current_dir(cwd).env_clear();
    for name in HOST_PASSTHROUGH {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // `nix` needs its own ambient configuration to run at all; it carries no manifest
    // resolution meaning, so passing the family through is safe.
    for (name, value) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("NIX_") {
            command.env(name, value);
        }
    }
    command.envs(tp.env());
    command.envs(baseline_inputs().iter().cloned());
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

pub fn expect_no_project_binding_files(project: &Path) -> Result<(), String> {
    for name in [".vivarium.toml", ".vivarium.local.toml"] {
        if project.join(name).exists() {
            return Err(format!(
                "obsolete project binding file exists: {}",
                project.join(name).display()
            ));
        }
    }
    Ok(())
}

pub fn expect_marker_id(project: &Path, expected: &str) -> Result<(), String> {
    let marker = project.join(".vivarium");
    let ignore =
        fs::read_to_string(marker.join(".gitignore")).map_err(|error| error.to_string())?;
    let id = fs::read_to_string(marker.join("id")).map_err(|error| error.to_string())?;
    if ignore.trim_end() != "*" || id != format!("{expected}\n") {
        return Err(format!(
            "invalid marker: .gitignore={ignore:?}, id={id:?}, expected id={expected:?}"
        ));
    }
    Ok(())
}

pub fn expect_marker_absent(project: &Path) -> Result<(), String> {
    if project.join(".vivarium").exists() {
        Err(format!(
            "identity marker still exists: {}",
            project.join(".vivarium").display()
        ))
    } else {
        Ok(())
    }
}

pub fn expect_registry_binding_visible(out: &VivOutput, manifest: &str) -> Result<(), String> {
    expect_code(out, 0)?;
    // spec/01 "Config inspection output" nests the four roots under `paths`, so only
    // three keys are top-level. Asserting all seven at the top level would demand that
    // a conforming implementation duplicate the nested names at the root — the hazard
    // `missing_at_path` exists to avoid.
    expect_json_keys(out, &["manifest", "source", "paths"])?;
    expect_json_fields_at(out, "paths", &["config", "state", "data", "cache"])?;
    expect_stdout_mentions(out, manifest)?;
    // Every call site reaches this through `bind()`, which writes the registry and then
    // reads it back with no `--manifest` override and a cleared environment, so the
    // structural assertion is provable here and strictly stronger than a substring scan.
    expect_json_string(out, "source", "registry")
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

/// Where a volume image of this fixture's own project lands under its state root.
///
/// The id comes from the fixture rather than from a caller-supplied literal, because the fixture
/// decorates its project name with a uniqueness token and a restated name would silently point at
/// a directory nothing ever writes — reported as an absent image rather than as a stale path.
pub fn volume_image(tp: &TempProject, name: &str) -> PathBuf {
    tp.state()
        .join("vivarium")
        .join("projects")
        .join(tp.project_id())
        .join("default")
        .join("volumes")
        .join(format!("{name}.img"))
}

pub fn expect_volume_image(tp: &TempProject, name: &str) -> Result<(), String> {
    let path = volume_image(tp, name);
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
