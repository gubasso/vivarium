//! The drive gate's own contract: what a gated run's child sees, and what happens when there is no
//! drive to run on.
//!
//! `tests/host/heavy-run` is what every hook that compiles, evaluates or boots is invoked through,
//! so its argv and environment contract is the thing standing between a `git push` and a full boot
//! disk. It is asserted here rather than in a hand-run lane for the same reason the hooks call
//! it at all: a rule nothing executes is a rule that holds until someone forgets.
//!
//! Nothing here is heavy. Every trial runs a shell program against a recording stub — `env` for
//! what was bound, `touch` for whether the child ran at all — so this belongs in the ordinary
//! integration lane and needs no gate, no `/dev/kvm`, and no boot.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn heavy_run() -> PathBuf {
    repo_root().join("tests").join("host").join("heavy-run")
}

fn disk_preflight() -> PathBuf {
    repo_root()
        .join("tests")
        .join("host")
        .join("disk-preflight")
}

/// Runs a program with stdin closed, which is the shape that matters.
///
/// The gate decides between prompting and refusing on whether standard input is a terminal, and a
/// test runner's own stdin is not something this suite should depend on: nextest's is already not a
/// terminal, but a hand-run `cargo test` from a shell is, and that difference would silently turn
/// the refusal trials into a hang waiting for an answer nobody is there to type.
fn run(program: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .output()
        .unwrap_or_else(|error| panic!("cannot execute {}: {error}", program.display()))
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The environment a child was handed, read back from `env`.
fn bound_environment(output: &Output) -> HashMap<String, String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// A directory that stands in for a plugged-in drive.
///
/// Deliberately not under `std::env::temp_dir()`: `--images` mode refuses a root under a temp root,
/// because N24 refuses a workspace that resolves there one boot later. The base is the harness's
/// answer to the same question — `disk-preflight --locate`, which gates nothing and asks nothing —
/// so this fixture cannot disagree with the thing it is testing about where a real root may live.
struct FabricatedDrive {
    path: PathBuf,
}

impl FabricatedDrive {
    fn new(name: &str) -> Self {
        let located = run(&disk_preflight(), &["--locate", "--images"], &[]);
        assert!(
            located.status.success(),
            "disk-preflight --locate must answer without gating: {}",
            stderr_of(&located)
        );
        let base = PathBuf::from(String::from_utf8_lossy(&located.stdout).trim());
        let path = base.join(format!("heavy-gate-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&path).expect("cannot create the fabricated drive");
        Self { path }
    }

    fn as_str(&self) -> &str {
        self.path
            .to_str()
            .expect("the fabricated drive is not UTF-8")
    }
}

impl Drop for FabricatedDrive {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A drive path that cannot be created, which is how every refusal trial below reaches the
/// no-usable-drive branch.
///
/// Set rather than unset on purpose. An unset variable makes `disk-preflight` read the developer's
/// untracked `.envrc.local`, so on a machine that has answered the drive question once, an
/// "unconfigured" trial would find the real drive and assert nothing. A path under `/proc` is
/// unusable on every Linux host and needs no fixture.
const NO_USABLE_DRIVE: (&str, &str) = ("VIVARIUM_HEAVY_DRIVE", "/proc/vivarium-no-such-drive");

/// `VIVARIUM_HEAVY_ON_HOST` is inherited like anything else, so a developer who exported it in the
/// shell that runs this suite would otherwise waive the requirement in every trial below and turn
/// three of them green for the wrong reason.
const NO_INHERITED_WAIVER: (&str, &str) = ("VIVARIUM_HEAVY_ON_HOST", "0");

#[test]
fn the_run_roots_are_bound_under_the_drive() {
    let drive = FabricatedDrive::new("bound");
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "env",
        ],
        &[
            ("VIVARIUM_HEAVY_DRIVE", drive.as_str()),
            NO_INHERITED_WAIVER,
        ],
    );
    assert!(
        output.status.success(),
        "the gate refused a usable drive: {}",
        stderr_of(&output)
    );

    let bound = bound_environment(&output);
    for (key, expected) in [
        ("VIVARIUM_HEAVY_DRIVE", drive.path.clone()),
        ("TMPDIR", drive.path.join("tmp")),
        ("CARGO_TARGET_DIR", drive.path.join("cargo-target")),
        ("XDG_STATE_HOME", drive.path.join("state")),
        ("XDG_DATA_HOME", drive.path.join("data")),
        ("XDG_CACHE_HOME", drive.path.join("cache")),
    ] {
        assert_eq!(
            bound.get(key).map(String::as_str),
            expected.to_str(),
            "{key} was not bound under the drive"
        );
        assert!(
            expected.is_dir(),
            "{key} names {} , which the child would have to create itself",
            expected.display()
        );
    }
}

#[test]
fn the_runtime_and_config_roots_are_left_where_they_are() {
    let drive = FabricatedDrive::new("untouched");
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "env",
        ],
        &[
            ("VIVARIUM_HEAVY_DRIVE", drive.as_str()),
            NO_INHERITED_WAIVER,
            ("XDG_RUNTIME_DIR", "/run/user/1234"),
            ("XDG_CONFIG_HOME", "/home/someone/.config"),
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let bound = bound_environment(&output);
    // A control socket path cannot exceed 108 bytes and a drive's mount point does not leave room,
    // and the config root is the user's authored source of truth that the tool only reads. Both are
    // load-bearing omissions, so both are asserted rather than left to the comment that says so.
    assert_eq!(
        bound.get("XDG_RUNTIME_DIR").map(String::as_str),
        Some("/run/user/1234"),
        "the runtime root was moved onto the drive"
    );
    assert_eq!(
        bound.get("XDG_CONFIG_HOME").map(String::as_str),
        Some("/home/someone/.config"),
        "the config root was moved onto the drive"
    );
}

#[test]
fn an_unusable_drive_refuses_and_the_command_never_runs() {
    let drive = FabricatedDrive::new("refused");
    let marker = drive.path.join("the-child-ran");
    let marker_arg = marker.to_str().expect("marker path is not UTF-8");
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "touch",
            marker_arg,
        ],
        &[NO_USABLE_DRIVE, NO_INHERITED_WAIVER],
    );

    assert_eq!(
        output.status.code(),
        Some(69),
        "an unusable drive must refuse: {}",
        stderr_of(&output)
    );
    // The negative control, and the reason this trial exists rather than a check on the exit code
    // alone: a gate that reports a refusal after running the command has refused nothing.
    assert!(
        !marker.exists(),
        "the command ran despite the refusal, which is the failure this gate exists to prevent"
    );

    let stderr = stderr_of(&output);
    for escape in ["--on-host", "VIVARIUM_HEAVY_ON_HOST=1"] {
        assert!(
            stderr.contains(escape),
            "the refusal does not name the {escape} escape:\n{stderr}"
        );
    }
}

#[test]
fn the_on_host_flag_waives_the_requirement() {
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--on-host",
            "--",
            "env",
        ],
        &[
            NO_USABLE_DRIVE,
            NO_INHERITED_WAIVER,
            ("CARGO_TARGET_DIR", "/the/cache/this/shell/already/has"),
        ],
    );
    assert!(
        output.status.success(),
        "the flag did not waive the requirement: {}",
        stderr_of(&output)
    );
    // The waiver hands the run the environment it already had, which is asserted as a value passing
    // through rather than as an absent key: this suite is itself run through the gate, so the
    // variable is set in the parent either way. Rewriting it here would fill a second compile cache
    // beside the dev shell's, which is the outcome sharing one exists to avoid.
    assert_eq!(
        bound_environment(&output)
            .get("CARGO_TARGET_DIR")
            .map(String::as_str),
        Some("/the/cache/this/shell/already/has"),
        "the waived run was given a compile cache of its own"
    );
}

#[test]
fn the_on_host_variable_waives_the_requirement() {
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "env",
        ],
        &[NO_USABLE_DRIVE, ("VIVARIUM_HEAVY_ON_HOST", "1")],
    );
    // The variable exists because `git push` cannot pass a flag to a hook, so this is the spelling
    // CI and every git-started run reach for.
    assert!(
        output.status.success(),
        "the variable did not waive the requirement: {}",
        stderr_of(&output)
    );
}

#[test]
fn a_zero_waiver_variable_waives_nothing() {
    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "env",
        ],
        &[NO_USABLE_DRIVE, ("VIVARIUM_HEAVY_ON_HOST", "0")],
    );
    // `=0` reads as "off" to everyone who writes it, and a gate that treated it as "on" would be
    // waived by the very setting someone used to turn the waiver off.
    assert_eq!(
        output.status.code(),
        Some(69),
        "a zero waiver was read as a waiver: {}",
        stderr_of(&output)
    );
}

#[test]
fn the_command_separator_is_required() {
    let output = run(
        &heavy_run(),
        &["--need", "0", "--label", "the gate's own trial"],
        &[NO_INHERITED_WAIVER],
    );
    assert_eq!(output.status.code(), Some(64), "{}", stderr_of(&output));
}

#[test]
fn an_unknown_argument_is_a_usage_error() {
    let output = run(
        &heavy_run(),
        &["--nope", "--", "true"],
        &[NO_INHERITED_WAIVER],
    );
    assert_eq!(output.status.code(), Some(64), "{}", stderr_of(&output));
}

#[test]
fn a_required_locate_answers_with_the_drive_or_with_nothing() {
    // The narrower locate: still no gate and no prompt, but a fallback root is not an answer to
    // "where is the drive". It is what the dev shell asks before choosing a compile cache, where
    // the state root would not be a second-best location — just a different one nobody asked for.
    let drive = FabricatedDrive::new("located");
    let found = run(
        &disk_preflight(),
        &["--locate", "--images", "--require-drive"],
        &[
            ("VIVARIUM_HEAVY_DRIVE", drive.as_str()),
            NO_INHERITED_WAIVER,
        ],
    );
    assert!(found.status.success(), "{}", stderr_of(&found));
    assert_eq!(
        String::from_utf8_lossy(&found.stdout).trim(),
        drive.as_str()
    );

    let absent = run(
        &disk_preflight(),
        &["--locate", "--images", "--require-drive"],
        &[NO_USABLE_DRIVE, NO_INHERITED_WAIVER],
    );
    assert_eq!(absent.status.code(), Some(1), "{}", stderr_of(&absent));
    assert!(
        absent.stdout.is_empty(),
        "a locate with no drive answered with a fallback anyway"
    );
}

#[test]
fn an_absent_drive_directory_is_refused_rather_than_created() {
    // The unplugged drive, which is the case the gate exists for and the one it is easiest to get
    // wrong: an unmounted mountpoint is an ordinary empty directory, so a resolver that created its
    // own answer would rebuild the path on the boot filesystem, measure the boot disk, find it
    // roomy, and send the run exactly where the gate is meant to keep it out of. Refusing is only
    // half the contract; the other half is that the directory is still not there afterwards.
    let base = FabricatedDrive::new("mountpoint");
    let absent = base.path.join("not-mounted");
    let absent_arg = absent.to_str().expect("the fabricated path is not UTF-8");

    let output = run(
        &heavy_run(),
        &[
            "--need",
            "0",
            "--label",
            "the gate's own trial",
            "--",
            "env",
        ],
        &[("VIVARIUM_HEAVY_DRIVE", absent_arg), NO_INHERITED_WAIVER],
    );

    assert_eq!(
        output.status.code(),
        Some(69),
        "an absent drive directory must refuse: {}",
        stderr_of(&output)
    );
    assert!(
        !absent.exists(),
        "the resolver created the drive directory, which is how an unplugged drive would pass"
    );
}

#[test]
fn an_option_missing_its_operand_is_a_usage_error() {
    // `shift 2` with one argument left shifts nothing and returns non-zero, and neither program
    // runs under `set -e`. Without an explicit check the parse loop re-reads the same option
    // forever, so a hook invoked with a truncated argument list hangs instead of reporting usage —
    // and a hang at the push stage is indistinguishable from a slow test suite.
    for (program, option) in [
        (heavy_run(), "--need"),
        (heavy_run(), "--label"),
        (disk_preflight(), "--need"),
        (disk_preflight(), "--label"),
    ] {
        let output = run(&program, &[option], &[NO_INHERITED_WAIVER]);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{} {option} did not report a usage error: {}",
            program.display(),
            stderr_of(&output)
        );
    }
}

#[test]
fn a_capacity_request_that_is_not_a_quantity_is_a_usage_error() {
    // A requirement reaches the free-space comparisons as arithmetic, and arithmetic will happily
    // accept what the caller meant as a number without asking whether it is one. `--need -100`
    // makes every comparison true and leaves a gate that still logs and no longer decides, which is
    // the failure this repository names inert rather than absent; a non-numeric value expands to an
    // unbound variable partway through instead of at the argument that caused it.
    for (program, value) in [
        (heavy_run(), "-4"),
        (heavy_run(), "abc"),
        (disk_preflight(), "-4"),
        (disk_preflight(), "abc"),
    ] {
        let output = run(
            &program,
            &[
                "--need",
                value,
                "--label",
                "the gate's own trial",
                "--",
                "true",
            ],
            &[NO_INHERITED_WAIVER],
        );
        assert_eq!(
            output.status.code(),
            Some(64),
            "{} accepted --need {value}: {}",
            program.display(),
            stderr_of(&output)
        );
    }
}
