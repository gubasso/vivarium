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
    #[error(
        "invalid invocation: expected `--spec <path> --ready-socket <path>` (both \
        absolute), `net-init`, or `resolver --spec <path>`"
    )]
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
    #[error("cannot raise the namespace's forwarding sysctl")]
    NetInit(#[source] std::io::Error),
    #[error("the gating resolver cannot bind its socket")]
    ResolverBind(#[source] std::io::Error),
    #[error("the gating resolver's socket failed")]
    ResolverServe(#[source] std::io::Error),
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
            Self::ReadSpec(_)
            | Self::InspectPath { .. }
            | Self::ReportReadiness(_)
            | Self::NetInit(_)
            | Self::ResolverBind(_)
            | Self::ResolverServe(_) => ExitKind::IoErr,
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
        Ok(Invocation::Supervise { spec, ready }) => run(&spec, &ready).await,
        Ok(Invocation::NetInit) => net_init(),
        Ok(Invocation::Resolver { spec }) => run_resolver(&spec).await,
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
        Some(LaunchReady::Failed) => {
            // The supervisor sends `Failed` before its teardown unlinks the
            // readiness socket, so this report is what spares the launcher its
            // full handoff wait. Best-effort, because the journal and the exit
            // code below carry the real cause either way.
            let _ = send_ready(ready_path, ReadinessReport::failed()).await;
            return Err(join(task)
                .await
                .err()
                .unwrap_or(SupervisorError::ReadinessNotReported));
        }
        None => {
            // The channel dropping without a report is the unexpected shape now
            // that both outcomes send one; reporting is attempted anyway, and
            // best-effort, because the runtime directory — where the readiness
            // socket lives — may already be gone on this path. The task's own
            // error is the real account; wait for it rather than reporting the
            // empty channel, which is only the symptom.
            let reported = send_ready(ready_path, ReadinessReport::failed()).await;
            return Err(join(task)
                .await
                .err()
                .or_else(|| reported.err())
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

/// The three things this binary can be: the supervisor itself, the in-namespace
/// sysctl one-shot, and the in-namespace gating resolver. The latter two are here
/// rather than in separate binaries because the launch closure already carries this
/// program as a store path and both need code the crate owns.
#[derive(Debug, Eq, PartialEq)]
enum Invocation {
    Supervise { spec: PathBuf, ready: PathBuf },
    NetInit,
    Resolver { spec: PathBuf },
}

/// Raise the forwarding sysctl inside the namespace this process was joined into.
///
/// `/proc/sys/net` reflects the writing process's own network namespace, so no
/// remount is needed; and no pinned tool writes sysctls, which is why this
/// entrypoint exists. IPv4 only: the guest link carries no IPv6 addressing, and
/// the resolver withholds AAAA for exactly that reason.
fn net_init() -> Result<(), SupervisorError> {
    std::fs::write("/proc/sys/net/ipv4/ip_forward", b"1").map_err(SupervisorError::NetInit)
}

/// How long the gating resolver gives one upstream exchange before `SERVFAIL`.
const RESOLVER_UPSTREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The gating resolver (spec/05), run inside the VM's namespace under allowlist
/// mode: the only DNS the guest is given, releasing an answer only after its
/// addresses are in the kernel's allowed set.
async fn run_resolver(spec_path: &Path) -> Result<(), SupervisorError> {
    // The same trust rule as supervision; the second argument repeats the spec
    // path because this entrypoint has no ready socket and the check only needs
    // the paths to share a directory.
    validate_metadata(spec_path, spec_path).await?;
    let bytes = fs::read(spec_path)
        .await
        .map_err(SupervisorError::ReadSpec)?;
    let spec = LaunchSpec::from_json(&bytes).map_err(SupervisorError::InvalidSpec)?;
    if spec.runtime_paths.launch_spec != spec_path {
        return Err(SupervisorError::SpecPathMismatch);
    }
    // Validation already held every entry to the grammar, so a failure here is a
    // defect rather than an input.
    let allowlist = vivarium::net::allowlist::Allowlist::parse(&spec.egress.allow)
        .map_err(|_| SupervisorError::InvalidSpec(LaunchError::InvalidSpec("egress allowlist")))?;
    let bind = std::net::SocketAddr::new(spec.network.gateway_address, spec.network.resolver_port);
    let socket = tokio::net::UdpSocket::bind(bind)
        .await
        .map_err(SupervisorError::ResolverBind)?;
    let upstream = std::net::SocketAddr::new(spec.network.dns_forward_address, 53);
    let programmer = vivarium::net::nft::NftProgrammer::new(vivarium::net::nft::NftRunner::direct(
        &spec.backend_programs.nft,
    ));
    vivarium::net::resolver::serve(
        socket,
        allowlist,
        vivarium::net::resolver::ServeConfig {
            upstream,
            // The guest link is IPv4-only, so there is no working IPv6 path and
            // spec/05 has the resolver withhold the family rather than release
            // addresses the guest cannot reach.
            withhold_aaaa: true,
            upstream_timeout: RESOLVER_UPSTREAM_TIMEOUT,
        },
        programmer,
    )
    .await
    .map_err(SupervisorError::ResolverServe)
}

/// Parses the exact invocations the launcher constructs, and nothing looser.
///
/// `argv` includes the program name, as `std::env::args_os` yields it. Passing it in rather than
/// reading it keeps this a pure function of its input, so the whole grammar can be asserted
/// without a process.
fn arguments<I>(argv: I) -> Result<Invocation, SupervisorError>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut tokens = argv.into_iter().skip(1);
    let first = tokens.next().ok_or(SupervisorError::Usage)?;
    if first == *"net-init" {
        if tokens.next().is_some() {
            return Err(SupervisorError::Usage);
        }
        return Ok(Invocation::NetInit);
    }
    if first == *"resolver" {
        if tokens.next().as_deref() != Some(std::ffi::OsStr::new("--spec")) {
            return Err(SupervisorError::Usage);
        }
        let spec = PathBuf::from(tokens.next().ok_or(SupervisorError::Usage)?);
        if tokens.next().is_some() || !spec.is_absolute() {
            return Err(SupervisorError::Usage);
        }
        return Ok(Invocation::Resolver { spec });
    }
    if first != *"--spec" {
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
    Ok(Invocation::Supervise { spec, ready })
}

#[cfg(test)]
mod tests {
    use super::{ExitKind, Invocation, LaunchError, ReadinessError, SupervisorError, arguments};
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
    fn only_the_exact_invocations_parse() {
        let ok = arguments(argv(&[
            "--spec",
            "/run/a/spec.json",
            "--ready-socket",
            "/run/a/r.sock",
        ]));
        assert_eq!(
            ok.ok(),
            Some(Invocation::Supervise {
                spec: PathBuf::from("/run/a/spec.json"),
                ready: PathBuf::from("/run/a/r.sock"),
            })
        );
        assert_eq!(
            arguments(argv(&["net-init"])).ok(),
            Some(Invocation::NetInit)
        );
        assert_eq!(
            arguments(argv(&["resolver", "--spec", "/run/a/spec.json"])).ok(),
            Some(Invocation::Resolver {
                spec: PathBuf::from("/run/a/spec.json"),
            })
        );

        for rejected in [
            vec![],                                                          // no arguments
            vec!["--spec", "/run/a/spec.json"],                              // ready socket absent
            vec!["--ready-socket", "/run/a/r.sock", "--spec", "/run/a/s"],   // wrong order
            vec!["--spec", "spec.json", "--ready-socket", "/run/a/r.sock"],  // relative spec
            vec!["--spec", "/run/a/spec.json", "--ready-socket", "r.sock"],  // relative socket
            vec!["--spec", "/run/a/s", "--ready-socket", "/run/a/r", "--x"], // trailing argument
            vec!["net-init", "--x"],                                         // trailing argument
            vec!["resolver"],                                                // spec absent
            vec!["resolver", "--spec", "spec.json"],                         // relative spec
            vec!["resolver", "--spec", "/run/a/s", "--x"],                   // trailing argument
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
            (SupervisorError::NetInit(io()), ExitKind::IoErr),
            (SupervisorError::ResolverBind(io()), ExitKind::IoErr),
            (SupervisorError::ResolverServe(io()), ExitKind::IoErr),
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
