//! Minimal slice-002 handoff. Public manifest orchestration remains unimplemented.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::process::Command;
use vivarium::exit::ExitKind;
use vivarium::launch::secure_fs::{self, SocketState};
use vivarium::launch::{
    LaunchError, LaunchSpec, ReadinessError, ReadinessReport, ReadinessStatus, TransientUnitSpec,
};

/// How long the launcher waits for the supervisor to report on the handoff socket.
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything this process can fail at, named once.
///
/// The alternative this replaced — threading a `(u8, &'static str)` pair out of every fallible
/// call — forced a category to be chosen at each of nineteen call sites and had nowhere to keep
/// the underlying cause, so a failure reached the operator as one line with the `io::Error`
/// discarded. Here the variant carries the explanation and [`Self::exit_code`] carries the
/// category. Per [`LaunchError`]'s own rule, every variant stays classified and non-secret.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error(
        "slice 002 accepts only `start --spec <resolved-launch.json>`, one absolute path; \
        manifest orchestration is not implemented"
    )]
    Usage,
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
    #[error("the supervisor did not report readiness within {}s", READINESS_TIMEOUT.as_secs())]
    ReadinessTimeout,
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
    /// Whether the runtime-directory and socket failures below belong in [`ExitKind::IoErr`] at
    /// all is open as Q-008 — this preserves the categories the untyped version emitted, and
    /// makes them visible in one place so the question can be settled from evidence.
    const fn exit_code(&self) -> ExitKind {
        match self {
            Self::Usage | Self::InvalidSpec(_) | Self::ReadinessPathOccupied => ExitKind::Usage,
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
            | Self::ReadinessTimeout
            | Self::InvalidReadiness(_)
            | Self::SupervisorFailed => ExitKind::Software,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // The process boundary owns the two things only it can: the connection to `argv`, and the
    // conversion of a failure into a rendered message plus an exit status. Everything between them
    // receives its inputs as parameters and reads no process global.
    //
    // `?` cannot appear in a function returning `ExitCode`, and `-> Result<_, _>` would collapse
    // every failure to exit 1 with a `Debug` dump, so the fallible program is `run` and this is
    // its adapter.
    let outcome = match arguments(std::env::args_os()) {
        Ok(spec_path) => run(&spec_path).await,
        Err(error) => Err(error),
    };
    match outcome {
        Ok(()) => ExitCode::from(ExitKind::Success),
        Err(error) => {
            report(&error);
            ExitCode::from(error.exit_code())
        }
    }
}

/// Writes the failure and its causes to stderr.
///
/// The chain walk is what makes the report usable: `LaunchError::Io { operation, source }` keeps
/// both which step failed and the underlying `io::Error`, and without this the operator sees a
/// single sentence and a number.
fn report(error: &StartError) {
    eprintln!("viv: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}

/// Launches one guest, given a path that has already been parsed and shown to be absolute.
///
/// Taking it as a parameter rather than reading `argv` is what makes that precondition a fact of
/// the signature instead of a convention: this function is not callable until `arguments` has
/// succeeded.
async fn run(spec_path: &Path) -> Result<(), StartError> {
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
        .map_err(|_| StartError::ReadinessTimeout)?
        .map_err(StartError::AcceptReadiness)?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(StartError::ReadReadiness)?;
    ReadinessReport::decode(&response).map_err(StartError::InvalidReadiness)
}

/// Clears a socket left by an earlier run, refusing to unlink anything that is not one.
async fn clear_readiness_path(path: &Path) -> Result<(), StartError> {
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

/// Parses the only invocation this handoff accepts, and nothing looser.
///
/// `argv` includes the program name, as `std::env::args_os` yields it. Passing it in rather than
/// reading it keeps this a pure function of its input, so the whole grammar can be asserted
/// without a process.
fn arguments<I>(argv: I) -> Result<PathBuf, StartError>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut tokens = argv.into_iter().skip(1);
    if tokens.next().as_deref() != Some(std::ffi::OsStr::new("start"))
        || tokens.next().as_deref() != Some(std::ffi::OsStr::new("--spec"))
    {
        return Err(StartError::Usage);
    }
    let input = PathBuf::from(tokens.next().ok_or(StartError::Usage)?);
    if tokens.next().is_some() || !input.is_absolute() {
        return Err(StartError::Usage);
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::{ExitKind, LaunchError, ReadinessError, StartError, arguments};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn io() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }

    fn argv(rest: &[&str]) -> Vec<OsString> {
        std::iter::once("viv")
            .chain(rest.iter().copied())
            .map(OsString::from)
            .collect()
    }

    /// Slice 002 constructs exactly one invocation, so anything else is a usage error rather than
    /// something to interpret. Testable at all only because `arguments` takes its input.
    #[test]
    fn only_the_exact_invocation_parses() {
        assert_eq!(
            arguments(argv(&["start", "--spec", "/run/a/launch.json"])).ok(),
            Some(PathBuf::from("/run/a/launch.json"))
        );

        for rejected in [
            vec![],                                             // no arguments
            vec!["start"],                                      // no flag
            vec!["start", "--spec"],                            // no path
            vec!["--spec", "/run/a/launch.json"],               // no verb
            vec!["stop", "--spec", "/run/a/launch.json"],       // another verb
            vec!["start", "--spec", "launch.json"],             // relative path
            vec!["start", "--spec", "/run/a/launch.json", "x"], // trailing argument
        ] {
            assert!(
                arguments(argv(&rejected)).is_err(),
                "accepted a malformed invocation: {rejected:?}"
            );
        }
    }

    /// One row per variant. The classification is a contract with whoever reads `$?`, so it is
    /// asserted rather than left to whichever arm a later edit happens to land in.
    #[test]
    fn every_variant_is_classified() {
        let rows: Vec<(StartError, ExitKind)> = vec![
            (StartError::Usage, ExitKind::Usage),
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
            (StartError::ReadinessTimeout, ExitKind::Software),
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
