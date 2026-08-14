//! The process boundary: `argv` in, bytes on a stream and a number out.
//!
//! Everything below this file is a function of its inputs. This is where the three things only a
//! process can supply arrive — the arguments, the environment, and whether stdin is a terminal —
//! and where a typed failure becomes an exit status. Keeping it that thin is what lets the whole
//! command surface be asserted without spawning anything.

use std::ffi::OsString;
use std::io::{IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::process::Command;
use vivarium::cli::grammar::{self, Invocation, Output, Streams, UsageError};
use vivarium::cli::{Context, Failure};
use vivarium::config::{self, Environment};
use vivarium::exit::ExitKind;
use vivarium::launch::secure_fs::{self, SocketState};
use vivarium::launch::{
    LaunchError, LaunchSpec, ReadinessError, ReadinessReport, ReadinessStatus, TransientUnitSpec,
};
use vivarium::protocol::SessionMode;

/// How long the launcher waits for the supervisor to report on the handoff socket.
///
/// This is the launcher's own wait, not the guest's boot budget: the supervisor
/// bounds each launch rung itself (`AGENT_STARTUP_TIMEOUT` and its siblings in
/// `src/launch/supervisor.rs`) and reports failure through the readiness socket
/// before its teardown unlinks it. Those budgets are deliberately strictly
/// smaller than this one, with headroom for the report to cross — so the
/// supervisor is always the party that reports, and this timeout's expiry keeps
/// a distinct meaning: no report arrived at all, which points at the unit never
/// running or the supervisor being killed, not at a slow guest.
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// The process's own environment, as the resolver's injected source.
struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn variable(&self, name: &'static str) -> Option<OsString> {
        std::env::var_os(name)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let streams = Streams {
        stdin_is_tty: std::io::stdin().is_terminal(),
    };
    let invocation = match grammar::parse(std::env::args_os(), streams) {
        Ok(invocation) => invocation,
        Err(error) => return fail(&Failure::Usage(error), Output::Human),
    };

    // The handoff is the one invocation that needs the async runtime, and it predates the published
    // surface: `nix/runner.sh` executes it to hand a resolved specification back to vivarium. It is
    // dispatched here rather than through `cli::run` because nothing else in that module is async,
    // and threading a runtime through the synchronous surface to serve one private form would make
    // every other verb pay for it.
    if let Invocation::StartSpec { spec } = &invocation {
        return match handoff(spec).await {
            Ok(()) => ExitCode::from(ExitKind::Success),
            Err(error) => {
                report_handoff(&error);
                ExitCode::from(error.exit_code())
            }
        };
    }

    let output = requested_output(&invocation);
    let context = match context() {
        Ok(context) => context,
        Err(failure) => return fail(&failure, output),
    };

    // The two session verbs, dispatched here for the same reason the handoff above is: they are
    // async, and they return a code the synchronous surface cannot express — the guest command's
    // own. `cli::run` stays synchronous, and the environment is read at the one boundary that owns
    // reading it, so the passthrough policy below it is a function of its inputs.
    if let Invocation::Exec(requested) | Invocation::Shell(requested) = &invocation {
        let mode = if matches!(invocation, Invocation::Shell(_)) {
            SessionMode::Shell
        } else {
            SessionMode::Exec
        };
        let host: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        return match vivarium::cli::session::run(requested, mode, &context, &host).await {
            Ok(kind) => ExitCode::from(kind),
            Err(failure) => fail(&failure, output),
        };
    }

    match vivarium::cli::run(&invocation, &context) {
        Ok(success) => {
            print!("{}", success.stdout);
            let _ = std::io::stdout().flush();
            // A note is not the result, so it never joins stdout: `config sources` marks a tie
            // and still succeeds, and a consumer piping stdout to `jq` must not receive it.
            if !success.notes.is_empty() {
                eprint!("{}", success.notes);
                let _ = std::io::stderr().flush();
            }
            ExitCode::from(ExitKind::Success)
        }
        Err(failure) => fail(&failure, output),
    }
}

/// Writes a failure to stderr and converts it into a status.
///
/// stderr rather than stdout because stdout carries the result and a failure has none — the rule
/// that keeps `… --json 2>/dev/null | jq` clean on success and empty on failure.
fn fail(failure: &Failure, output: Output) -> ExitCode {
    eprint!("{}", failure.render(output));
    let _ = std::io::stderr().flush();
    ExitCode::from(failure.code())
}

/// Which output shape a failure should be rendered in.
///
/// Read from the invocation rather than from a global, because `--json` is per-command: a consumer
/// that asked one verb for JSON has not asked the next one.
const fn requested_output(invocation: &Invocation) -> Output {
    match invocation {
        Invocation::Init { output, .. }
        | Invocation::Config { output, .. }
        | Invocation::ConfigEval { output }
        | Invocation::ConfigSources { output }
        | Invocation::ManifestList { output }
        | Invocation::ManifestShow { output, .. }
        | Invocation::Start { output, .. }
        | Invocation::Status { output, .. }
        | Invocation::Stop { output, .. }
        | Invocation::VolumeList { output }
        | Invocation::VolumePrune { output, .. }
        | Invocation::Destroy { output, .. }
        | Invocation::Deferred { output, .. } => *output,
        // Neither the private handoff nor a session carries `--json`, and a session's own failures
        // are vivarium's own: spec/12 puts them on stderr beside the guest's, in the human form.
        Invocation::StartSpec { .. } | Invocation::Exec(_) | Invocation::Shell(_) => Output::Human,
    }
}

/// Gathers what the host can say about itself, before any command runs.
fn context() -> Result<Context<'static, ProcessEnvironment>, Failure> {
    let roots = config::resolve_xdg_roots(&ProcessEnvironment).map_err(|error| {
        Failure::Usage(UsageError {
            message: error.to_string(),
            usage: None,
        })
    })?;
    // A working directory that cannot be read is not a project, so there is nothing to resolve
    // against; falling back to `.` would silently bind whatever the parent happened to be.
    let project = std::env::current_dir().map_err(|error| {
        Failure::Usage(UsageError {
            message: format!("cannot read the current directory: {error}"),
            usage: None,
        })
    })?;
    Ok(Context {
        roots,
        project: config::canonical_project(&project),
        environment: &ProcessEnvironment,
    })
}

/// Everything the supervisor handoff can fail at, named once.
///
/// The alternative this replaced — threading a `(u8, &'static str)` pair out of every fallible
/// call — forced a category to be chosen at each of nineteen call sites and had nowhere to keep
/// the underlying cause, so a failure reached the operator as one line with the `io::Error`
/// discarded. Here the variant carries the explanation and [`Self::exit_code`] carries the
/// category. Per [`LaunchError`]'s own rule, every variant stays classified and non-secret.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("cannot read the launch specification")]
    ReadInputSpec(#[source] std::io::Error),
    #[error("the launch specification is not valid")]
    InvalidSpec(#[source] LaunchError),
    #[error("cannot prepare the private runtime directory")]
    PrepareRuntime(#[source] LaunchError),
    #[error("the readiness path exists and is not a socket")]
    ReadinessPathOccupied,
    #[error("cannot replace the stale readiness socket")]
    ReplaceReadinessSocket(#[source] std::io::Error),
    #[error("cannot bind the readiness socket")]
    BindReadiness(#[source] std::io::Error),
    #[error("cannot submit the transient user service")]
    SubmitUnit(#[source] std::io::Error),
    #[error("the transient user service was rejected")]
    UnitRejected,
    #[error(
        "no readiness report arrived within the launcher's {}s handoff wait; a failing launch \
            reports promptly, so nothing reported at all — the unit may never have run",
        READINESS_TIMEOUT.as_secs()
    )]
    HandoffTimeout,
    #[error("the readiness connection failed")]
    AcceptReadiness(#[source] std::io::Error),
    #[error("cannot read the readiness report")]
    ReadReadiness(#[source] std::io::Error),
    #[error("the supervisor's readiness report cannot be understood")]
    InvalidReadiness(#[source] ReadinessError),
    #[error("the supervisor reported that the launch failed; its own diagnostic is in the journal")]
    SupervisorFailed,
}

impl StartError {
    /// The exit category, chosen in one place rather than at each failing call.
    ///
    /// Exhaustive by construction: a new variant will not compile until it is classified here.
    ///
    /// The six [`ExitKind::IoErr`] arms were Q-008, and slice 012 settled it in their favour: the
    /// private runtime directory vivarium creates, the launch specification it writes there, and
    /// the readiness socket it binds are all channels vivarium owns, which is spec/14's own
    /// description of `74`. The `start` row already admitted `74` for generated-tree and lock
    /// staging I/O, so the resolution widened that admission to name these too rather than adding
    /// a code to a row that had none.
    const fn exit_code(&self) -> ExitKind {
        match self {
            Self::InvalidSpec(_) | Self::ReadinessPathOccupied => ExitKind::Usage,
            Self::ReadInputSpec(_)
            | Self::PrepareRuntime(_)
            | Self::ReplaceReadinessSocket(_)
            | Self::BindReadiness(_)
            | Self::AcceptReadiness(_)
            | Self::ReadReadiness(_) => ExitKind::IoErr,
            // A rejected unit, a missed deadline, and an unintelligible report are all failures of
            // machinery vivarium owns rather than of a channel it was using.
            Self::SubmitUnit(_)
            | Self::UnitRejected
            | Self::HandoffTimeout
            | Self::InvalidReadiness(_)
            | Self::SupervisorFailed => ExitKind::Software,
        }
    }
}

/// Writes the handoff failure and its causes to stderr.
///
/// The chain walk is what makes the report usable: `LaunchError::Io { operation, source }` keeps
/// both which step failed and the underlying `io::Error`, and without this the operator sees a
/// single sentence and a number.
fn report_handoff(error: &StartError) {
    eprintln!("viv: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}

/// Launches one guest, given a path the grammar has already shown to be absolute.
async fn handoff(spec_path: &Path) -> Result<(), StartError> {
    let bytes = fs::read(spec_path)
        .await
        .map_err(StartError::ReadInputSpec)?;
    let spec = LaunchSpec::from_json(&bytes).map_err(StartError::InvalidSpec)?;

    // The supervisor will only trust a specification whose directory and file carry the private
    // modes, so the same two constants that describe that rule are what writes it here.
    secure_fs::private_dir(&spec.runtime_paths.root)
        .await
        .map_err(StartError::PrepareRuntime)?;
    secure_fs::private_write(&spec.runtime_paths.launch_spec, &bytes)
        .await
        .map_err(StartError::PrepareRuntime)?;

    clear_readiness_path(&spec.runtime_paths.ready_socket).await?;
    let listener =
        UnixListener::bind(&spec.runtime_paths.ready_socket).map_err(StartError::BindReadiness)?;

    let unit = TransientUnitSpec::new(&spec);
    let status = Command::new(unit.command().program())
        .args(unit.command().args())
        .status()
        .await
        .map_err(StartError::SubmitUnit)?;
    if !status.success() {
        return Err(StartError::UnitRejected);
    }

    match await_readiness(&listener).await?.status {
        ReadinessStatus::ProcessReady => Ok(()),
        ReadinessStatus::Failed => Err(StartError::SupervisorFailed),
    }
}

/// Accepts the supervisor's one connection and reads the report it writes before hanging up.
async fn await_readiness(listener: &UnixListener) -> Result<ReadinessReport, StartError> {
    let (mut stream, _) = tokio::time::timeout(READINESS_TIMEOUT, listener.accept())
        .await
        .map_err(|_| StartError::HandoffTimeout)?
        .map_err(StartError::AcceptReadiness)?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(StartError::ReadReadiness)?;
    ReadinessReport::decode(&response).map_err(StartError::InvalidReadiness)
}

/// Clears a socket left by an earlier run, refusing to unlink anything that is not one.
async fn clear_readiness_path(path: &PathBuf) -> Result<(), StartError> {
    match secure_fs::socket_state(path)
        .await
        .map_err(StartError::PrepareRuntime)?
    {
        SocketState::Absent => Ok(()),
        SocketState::Socket => fs::remove_file(path)
            .await
            .map_err(StartError::ReplaceReadinessSocket),
        SocketState::Other => Err(StartError::ReadinessPathOccupied),
    }
}

#[cfg(test)]
mod tests {
    use super::{ExitKind, LaunchError, ReadinessError, StartError};

    fn io() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }

    /// One row per variant. The classification is a contract with whoever reads `$?`, so it is
    /// asserted rather than left to whichever arm a later edit happens to land in.
    ///
    /// The six `IoErr` rows now pin a settled answer rather than a preserved status quo: slice 012
    /// resolved Q-008 by widening the `start` row of spec/14 to admit `74` for runtime-directory
    /// and readiness-socket I/O, so moving one of them is a spec change and not a refactor.
    #[test]
    fn every_variant_is_classified() {
        let rows: Vec<(StartError, ExitKind)> = vec![
            (
                StartError::InvalidSpec(LaunchError::InvalidSpec("schema")),
                ExitKind::Usage,
            ),
            (StartError::ReadinessPathOccupied, ExitKind::Usage),
            (StartError::ReadInputSpec(io()), ExitKind::IoErr),
            (
                StartError::PrepareRuntime(LaunchError::Io {
                    operation: "create runtime directory",
                    source: io(),
                }),
                ExitKind::IoErr,
            ),
            (StartError::ReplaceReadinessSocket(io()), ExitKind::IoErr),
            (StartError::BindReadiness(io()), ExitKind::IoErr),
            (StartError::AcceptReadiness(io()), ExitKind::IoErr),
            (StartError::ReadReadiness(io()), ExitKind::IoErr),
            (StartError::SubmitUnit(io()), ExitKind::Software),
            (StartError::UnitRejected, ExitKind::Software),
            (StartError::HandoffTimeout, ExitKind::Software),
            (
                StartError::InvalidReadiness(ReadinessError::UnsupportedSchema(2)),
                ExitKind::Software,
            ),
            (StartError::SupervisorFailed, ExitKind::Software),
        ];
        for (error, expected) in rows {
            assert_eq!(error.exit_code(), expected, "misclassified: {error}");
        }
    }

    /// The reason the typed error exists: the untyped predecessor discarded every `io::Error`, so
    /// a runtime-directory failure reached the operator with no account of why.
    #[test]
    fn the_preparation_cause_survives_to_the_report() {
        let error = StartError::PrepareRuntime(LaunchError::Io {
            operation: "set runtime permissions",
            source: io(),
        });
        let mut chain = Vec::new();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            chain.push(cause.to_string());
            source = cause.source();
        }
        assert_eq!(
            chain,
            vec![
                "I/O failure while handling set runtime permissions".to_string(),
                "permission denied".to_string(),
            ]
        );
    }
}
