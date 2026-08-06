//! Closure-internal supervisor entry point.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use vivarium::exit::ExitKind;
use vivarium::launch::secure_fs::{PRIVATE_DIR_MODE, PRIVATE_FILE_MODE};
use vivarium::launch::{
    LaunchError, LaunchReady, LaunchSpec, ReadinessError, ReadinessReport, Supervisor,
};

/// The permission bits of a mode, with the file-type bits masked away.
const MODE_MASK: u32 = 0o777;

/// Everything this process can fail at, named once.
///
/// A bare exit code cannot say which of nine `LaunchError` variants fired, and this binary is a
/// transient unit's `MainPID` — the only place its failure can be explained is the journal. The
/// variant carries the explanation; `exit_code` carries the category. Per
/// [`LaunchError`]'s own rule, every variant stays classified and non-secret.
#[derive(Debug, thiserror::Error)]
enum SupervisorError {
    #[error("invalid invocation: expected `--spec <path> --ready-socket <path>`, both absolute")]
    Usage,
    #[error(
        "the launch specification and its directory do not satisfy the ownership and mode rule"
    )]
    UntrustedSpecFile,
    #[error("the launch specification does not describe the paths this process was handed")]
    SpecPathMismatch,
    #[error("cannot read the launch specification")]
    ReadSpec(#[source] std::io::Error),
    #[error("cannot inspect the {what} before trusting it")]
    InspectPath {
        what: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("the launch specification is not valid")]
    InvalidSpec(#[source] LaunchError),
    #[error("cannot report readiness on the handoff socket")]
    ReportReadiness(#[source] std::io::Error),
    #[error("cannot encode the readiness report")]
    EncodeReadiness(#[source] ReadinessError),
    #[error("supervision failed")]
    Supervision(#[source] LaunchError),
    #[error("the supervision task did not run to completion")]
    SupervisionTask,
    #[error("supervision ended before it reported process readiness")]
    ReadinessNotReported,
}

impl SupervisorError {
    /// The exit category, chosen in one place rather than at each failing call.
    ///
    /// Exhaustive by construction: a new variant will not compile until it is classified here.
    const fn exit_code(&self) -> ExitKind {
        match self {
            Self::Usage
            | Self::UntrustedSpecFile
            | Self::SpecPathMismatch
            | Self::InvalidSpec(_) => ExitKind::Usage,
            Self::ReadSpec(_) | Self::InspectPath { .. } | Self::ReportReadiness(_) => {
                ExitKind::IoErr
            }
            // Encoding a fixed-shape report cannot fail on well-formed input, so a failure here is
            // a defect in this program rather than an I/O condition.
            Self::EncodeReadiness(_)
            | Self::Supervision(_)
            | Self::SupervisionTask
            | Self::ReadinessNotReported => ExitKind::Software,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // The process boundary owns the two things only it can: the connection to `argv`, and the
    // conversion of a failure into a rendered message plus an exit status. Everything between
    // them receives its inputs as parameters and reads no process global.
    //
    // `?` cannot appear in a function returning `ExitCode`, and `-> Result<_, _>` would collapse
    // every failure to exit 1 with a `Debug` dump, so the fallible program is `run` and this is
    // its adapter.
    let outcome = match arguments(std::env::args_os()) {
        Ok((spec_path, ready_path)) => run(&spec_path, &ready_path).await,
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

/// Writes the failure to stderr, which for a transient unit is the journal.
///
/// The chain walk is the point: `LaunchError::Io { operation, source }` keeps the underlying
/// `io::Error`, and without this the operator sees a number and nothing else.
fn report(error: &SupervisorError) {
    eprintln!("vivarium-supervisor: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}

/// Supervises one guest, given paths that have already been parsed and shown to be absolute.
///
/// Taking them as parameters rather than reading `argv` is what makes that precondition a fact of
/// the signature instead of a convention: this function is not callable until `arguments` has
/// succeeded.
async fn run(spec_path: &Path, ready_path: &Path) -> Result<(), SupervisorError> {
    validate_metadata(spec_path, ready_path).await?;
    let bytes = fs::read(spec_path)
        .await
        .map_err(SupervisorError::ReadSpec)?;
    let spec = LaunchSpec::from_json(&bytes).map_err(SupervisorError::InvalidSpec)?;
    if spec.runtime_paths.launch_spec != spec_path || spec.runtime_paths.ready_socket != ready_path
    {
        return Err(SupervisorError::SpecPathMismatch);
    }
    let (ready_tx, mut ready_rx) = mpsc::channel(1);
    let supervisor = Supervisor::new(spec);
    let task = tokio::spawn(supervisor.run(ready_tx));
    match ready_rx.recv().await {
        Some(LaunchReady::ProcessReady) => {
            send_ready(ready_path, ReadinessReport::process_ready()).await?;
        }
        None => {
            send_ready(ready_path, ReadinessReport::failed()).await?;
            // The task's own error is the real account of what went wrong; wait for it rather
            // than reporting the empty channel, which is only the symptom.
            return Err(join(task)
                .await
                .err()
                .unwrap_or(SupervisorError::ReadinessNotReported));
        }
    }
    join(task).await
}

/// Awaits the supervision task, keeping both failure layers distinguishable.
async fn join(
    task: tokio::task::JoinHandle<Result<vivarium::launch::ShutdownReason, LaunchError>>,
) -> Result<(), SupervisorError> {
    task.await
        .map_err(|_| SupervisorError::SupervisionTask)?
        .map(|_| ())
        .map_err(SupervisorError::Supervision)
}

async fn validate_metadata(spec: &Path, ready: &Path) -> Result<(), SupervisorError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if !spec.is_absolute() || !ready.is_absolute() || spec.parent() != ready.parent() {
        return Err(SupervisorError::UntrustedSpecFile);
    }
    let spec_metadata =
        fs::symlink_metadata(spec)
            .await
            .map_err(|source| SupervisorError::InspectPath {
                what: "launch specification",
                source,
            })?;
    let parent = spec.parent().ok_or(SupervisorError::UntrustedSpecFile)?;
    let parent_metadata =
        fs::symlink_metadata(parent)
            .await
            .map_err(|source| SupervisorError::InspectPath {
                what: "runtime directory",
                source,
            })?;
    // The modes come from the same constants the launcher wrote them with, so the producer and
    // this validator cannot drift apart without failing to compile.
    if !spec_metadata.is_file()
        || spec_metadata.file_type().is_symlink()
        || spec_metadata.permissions().mode() & MODE_MASK != PRIVATE_FILE_MODE
        || parent_metadata.permissions().mode() & MODE_MASK != PRIVATE_DIR_MODE
        || spec_metadata.uid() != parent_metadata.uid()
    {
        return Err(SupervisorError::UntrustedSpecFile);
    }
    Ok(())
}

async fn send_ready(path: &Path, report: ReadinessReport) -> Result<(), SupervisorError> {
    let encoded = report.encode().map_err(SupervisorError::EncodeReadiness)?;
    let mut stream = UnixStream::connect(path)
        .await
        .map_err(SupervisorError::ReportReadiness)?;
    stream
        .write_all(&encoded)
        .await
        .map_err(SupervisorError::ReportReadiness)
}

/// Parses the exact invocation the launcher constructs, and nothing looser.
///
/// `argv` includes the program name, as `std::env::args_os` yields it. Passing it in rather than
/// reading it keeps this a pure function of its input, so the whole grammar can be asserted
/// without a process.
fn arguments<I>(argv: I) -> Result<(PathBuf, PathBuf), SupervisorError>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut tokens = argv.into_iter().skip(1);
    if tokens.next().as_deref() != Some(std::ffi::OsStr::new("--spec")) {
        return Err(SupervisorError::Usage);
    }
    let spec = PathBuf::from(tokens.next().ok_or(SupervisorError::Usage)?);
    if tokens.next().as_deref() != Some(std::ffi::OsStr::new("--ready-socket")) {
        return Err(SupervisorError::Usage);
    }
    let ready = PathBuf::from(tokens.next().ok_or(SupervisorError::Usage)?);
    if tokens.next().is_some() || !spec.is_absolute() || !ready.is_absolute() {
        return Err(SupervisorError::Usage);
    }
    Ok((spec, ready))
}

#[cfg(test)]
mod tests {
    use super::{ExitKind, LaunchError, ReadinessError, SupervisorError, arguments};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn io() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }

    fn argv(rest: &[&str]) -> Vec<OsString> {
        std::iter::once("vivarium-supervisor")
            .chain(rest.iter().copied())
            .map(OsString::from)
            .collect()
    }

    /// The launcher constructs exactly one invocation, so anything else is a usage error rather
    /// than something to interpret. Testable at all only because `arguments` takes its input.
    #[test]
    fn only_the_exact_invocation_parses() {
        let ok = arguments(argv(&[
            "--spec",
            "/run/a/spec.json",
            "--ready-socket",
            "/run/a/r.sock",
        ]));
        assert_eq!(
            ok.ok(),
            Some((
                PathBuf::from("/run/a/spec.json"),
                PathBuf::from("/run/a/r.sock")
            ))
        );

        for rejected in [
            vec![],                                                          // no arguments
            vec!["--spec", "/run/a/spec.json"],                              // ready socket absent
            vec!["--ready-socket", "/run/a/r.sock", "--spec", "/run/a/s"],   // wrong order
            vec!["--spec", "spec.json", "--ready-socket", "/run/a/r.sock"],  // relative spec
            vec!["--spec", "/run/a/spec.json", "--ready-socket", "r.sock"],  // relative socket
            vec!["--spec", "/run/a/s", "--ready-socket", "/run/a/r", "--x"], // trailing argument
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
        let rows: Vec<(SupervisorError, ExitKind)> = vec![
            (SupervisorError::Usage, ExitKind::Usage),
            (SupervisorError::UntrustedSpecFile, ExitKind::Usage),
            (SupervisorError::SpecPathMismatch, ExitKind::Usage),
            (
                SupervisorError::InvalidSpec(LaunchError::InvalidSpec("schema")),
                ExitKind::Usage,
            ),
            (SupervisorError::ReadSpec(io()), ExitKind::IoErr),
            (
                SupervisorError::InspectPath {
                    what: "launch specification",
                    source: io(),
                },
                ExitKind::IoErr,
            ),
            (SupervisorError::ReportReadiness(io()), ExitKind::IoErr),
            (
                SupervisorError::EncodeReadiness(ReadinessError::UnsupportedSchema(2)),
                ExitKind::Software,
            ),
            (SupervisorError::SupervisionTask, ExitKind::Software),
            (SupervisorError::ReadinessNotReported, ExitKind::Software),
            (
                SupervisorError::Supervision(LaunchError::ChildExit {
                    kind: "virtiofsd",
                    status: Some(1),
                }),
                ExitKind::Software,
            ),
        ];
        for (error, expected) in rows {
            assert_eq!(error.exit_code(), expected, "misclassified: {error}");
        }
    }

    /// The reason the typed error exists: the operator has to be able to tell which child died.
    #[test]
    fn the_supervision_cause_survives_to_the_report() {
        let error = SupervisorError::Supervision(LaunchError::ChildExit {
            kind: "virtiofsd",
            status: Some(1),
        });
        let cause = std::error::Error::source(&error).map(ToString::to_string);
        assert_eq!(
            cause.as_deref(),
            Some("child virtiofsd exited unexpectedly with status Some(1)")
        );
    }
}
