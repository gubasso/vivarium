//! Target-host proof for the real guest `AF_VSOCK` and credential relay.
//!
//! Everything here needs a booted guest, so the lane carries one gated trial rather than
//! many: a trial per assertion would spend a boot per assertion. The gate is evaluated at
//! run time and reports why it did not run, because a lane that skips silently reads as a
//! lane that passed. `tests/host/guest-agent-check` is what builds the runner, exports
//! `VIVARIUM_AGENT_RUNNER`, and runs this twice.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use libtest_mimic::{Arguments, Failed, Trial};
use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;
use vivarium::launch::BootMetadata;
use vivarium::protocol::hybrid;
use vivarium::protocol::{
    AgentFrame, CONTROL_PORT, CREDENTIAL_POOL_SIZE, ClientFrame, EnvironmentVariable, Hello,
    SCHEMA_VERSION, SessionMode, StartRequest, TerminalSize, UnixBytes, read_agent_frame,
    write_client_frame,
};

/// The guest-local context id. A peer that reaches either port from here is the guest
/// talking to itself, which both accept loops must refuse.
const VMADDR_CID_LOCAL: u32 = 1;

/// The unprivileged port the loopback positive control uses. It only has to be free and
/// outside the two the agent owns.
const LOOPBACK_PROBE_PORT: u32 = 59_000;

/// The guest's own system path, which every assertion below runs its commands through.
///
/// A NixOS system puts its environment's binaries here and nowhere a bare `execve` would
/// find them. The agent clears the environment, so this is passed the way a real `viv exec`
/// forwards the caller's; without it a session reaches shell builtins and nothing else.
const GUEST_PATH: &[u8] = b"/run/current-system/sw/bin";

#[path = "support/harness.rs"]
mod harness;

use harness::{base64, gate_required};

fn main() -> std::process::ExitCode {
    let args = Arguments::from_args();
    let decision = gate();
    let require = gate_required();
    let ignored = decision.is_err() && !require;
    if ignored && let Err(reason) = &decision {
        eprintln!("gated: guest_agent_and_credential_relay_on_capable_host — {reason}");
    }
    let trials = vec![
        Trial::test("agent_host_self_check", self_check),
        Trial::test(
            "guest_agent_and_credential_relay_on_capable_host",
            move || {
                let runner = gate().map_err(|reason| {
                    // Reached either because the operator set VIVARIUM_TEST_REQUIRE=1, or
                    // because the trial ran despite its ignored flag (`--run-ignored`).
                    let cause = if gate_required() {
                        "gate unmet but VIVARIUM_TEST_REQUIRE=1"
                    } else {
                        "gate unmet and the ignored flag was overridden"
                    };
                    Failed::from(format!("{cause}: {reason}"))
                })?;
                tokio::runtime::Runtime::new()
                    .map_err(|error| Failed::from(error.to_string()))?
                    .block_on(guest_agent_and_credential_relay(&runner));
                Ok(())
            },
        )
        .with_ignored_flag(ignored),
    ];
    libtest_mimic::run(&args, trials).exit_code()
}

/// Resolve the guest runner, or the first reason this host cannot run the lane.
///
/// The premises are the lane's own, not the CLI harness's: this trial never invokes `viv`
/// directly, it invokes the Nix runner, which carries its own. Chaining onto the CLI probe
/// would make the lane skip for a reason that is not among its prerequisites.
fn gate() -> Result<PathBuf, String> {
    let Some(runner) = std::env::var_os("VIVARIUM_AGENT_RUNNER") else {
        return Err(
            "VIVARIUM_AGENT_RUNNER is unset; run tests/host/guest-agent-check, which builds \
            the runner and sets it"
                .to_owned(),
        );
    };
    let runner = PathBuf::from(runner);
    if !runner.is_file() {
        return Err(format!(
            "VIVARIUM_AGENT_RUNNER is not a file: {}",
            runner.display()
        ));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .map(drop)
        .map_err(|error| format!("/dev/kvm is not read-write openable: {error}"))?;
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(value) if Path::new(&value).is_dir() => {}
        Some(value) => {
            return Err(format!(
                "XDG_RUNTIME_DIR is not a directory: {}",
                Path::new(&value).display()
            ));
        }
        None => return Err("XDG_RUNTIME_DIR is unset".to_owned()),
    }
    match std::process::Command::new("systemctl")
        .args(["--user", "show", "--property=Version"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(runner),
        Ok(output) => Err(format!(
            "systemctl --user exited {:?}; no systemd user manager",
            output.status.code()
        )),
        Err(error) => Err(format!("systemctl is unavailable: {error}")),
    }
}

/// Assertions that need no guest, so the lane is never entirely ignored.
///
/// nextest exits 4 when every trial matching a filter is ignored, and a lane that cannot
/// report anything on an ordinary developer host is a lane nobody notices has rotted.
fn self_check() -> Result<(), Failed> {
    // An unmet gate must carry a reason a reader can act on, which is the difference
    // between a skip that informs and one that hides.
    if let Err(reason) = gate()
        && reason.is_empty()
    {
        return Err(Failed::from("the gate refused without giving a reason"));
    }

    let runtime =
        tokio::runtime::Runtime::new().map_err(|error| Failed::from(error.to_string()))?;
    runtime.block_on(async {
        // The frame helpers this lane drives the guest with, over an in-memory pair.
        let (mut client, mut server) = tokio::io::duplex(4096);
        let hello = Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: "01234567-89ab-cdef-0123-456789abcdef".to_owned(),
        };
        write_client_frame(&mut client, &ClientFrame::Hello(hello.clone()))
            .await
            .unwrap();
        assert_eq!(
            vivarium::protocol::read_client_frame(&mut server)
                .await
                .unwrap(),
            ClientFrame::Hello(hello)
        );
        vivarium::protocol::write_agent_frame(&mut server, &AgentFrame::Pong)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Pong
        );
    });

    Ok(())
}

fn bytes(value: &[u8]) -> UnixBytes {
    UnixBytes::new(value.to_vec())
}

/// Open a control connection without completing the handshake.
async fn connect_raw(runtime: &Path) -> UnixStream {
    hybrid::connect(
        &runtime.join("control.sock"),
        CONTROL_PORT,
        Duration::from_secs(5),
    )
    .await
    .unwrap()
}

async fn connect_control(runtime: &Path, metadata: &BootMetadata) -> UnixStream {
    let mut stream = connect_raw(runtime).await;
    write_client_frame(
        &mut stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        read_agent_frame(&mut stream).await.unwrap(),
        AgentFrame::Hello(_)
    ));
    stream
}

async fn execute(
    runtime: &Path,
    metadata: &BootMetadata,
    argv: Vec<UnixBytes>,
    stdin: &[u8],
    pty: bool,
) -> (Vec<u8>, Vec<u8>, u8) {
    let mut stream = begin_session(runtime, metadata, argv, stdin, pty).await;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    loop {
        match read_agent_frame(&mut stream).await.unwrap() {
            AgentFrame::Stdout(bytes) => stdout.extend(bytes),
            AgentFrame::Stderr(bytes) => stderr.extend(bytes),
            AgentFrame::Exit(exit) => return (stdout, stderr, exit.status),
            AgentFrame::Error(error) => panic!("guest agent error: {}", error.code),
            _ => panic!("unexpected frame"),
        }
    }
}

/// Open a session and send its request, returning before any reply is read.
///
/// Split out of `execute` for the one caller whose session is expected to die without ever
/// replying. `execute`'s read loop unwraps, so it cannot be reused where end-of-file is the
/// correct outcome rather than a failure.
async fn begin_session(
    runtime: &Path,
    metadata: &BootMetadata,
    argv: Vec<UnixBytes>,
    stdin: &[u8],
    pty: bool,
) -> UnixStream {
    let mut stream = connect_control(runtime, metadata).await;
    if pty {
        write_client_frame(
            &mut stream,
            &ClientFrame::Resize(TerminalSize {
                rows: 31,
                columns: 97,
            }),
        )
        .await
        .unwrap();
    }
    write_client_frame(
        &mut stream,
        &ClientFrame::Start(StartRequest {
            mode: SessionMode::Exec,
            argv,
            environment: vec![
                // The session clears the environment, so a guest command reaches nothing
                // outside the shell's own builtins unless the request names a search path.
                EnvironmentVariable {
                    name: bytes(b"PATH"),
                    value: bytes(GUEST_PATH),
                },
                // Deliberately poisoned: the guest's own credential socket must win over
                // anything the request carries, and nothing else in this lane would notice
                // if the host value leaked through.
                EnvironmentVariable {
                    name: bytes(b"SSH_AUTH_SOCK"),
                    value: bytes(b"/host/path/must-not-win"),
                },
            ],
            cwd: bytes(b"/"),
            pty,
        }),
    )
    .await
    .unwrap();
    if !stdin.is_empty() {
        write_client_frame(&mut stream, &ClientFrame::Stdin(stdin.to_vec()))
            .await
            .unwrap();
    }
    write_client_frame(&mut stream, &ClientFrame::StdinEnd)
        .await
        .unwrap();
    stream
}

/// Run one shell script in the guest and return its trimmed standard output and status.
async fn sh(runtime: &Path, metadata: &BootMetadata, script: &str) -> (String, u8) {
    let (stdout, _, status) = execute(
        runtime,
        metadata,
        vec![bytes(b"/bin/sh"), bytes(b"-c"), bytes(script.as_bytes())],
        b"",
        false,
    )
    .await;
    (String::from_utf8_lossy(&stdout).trim().to_owned(), status)
}

/// Emit one measured figure for the harness findings register.
fn record(label: &str, value: &str) {
    eprintln!("[RECORD] {label}: {value}");
}

async fn guest_agent_and_credential_relay(runner: &Path) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // The heavy half — the two volume images, and whatever a failure leaves behind to read —
    // stays under `TMPDIR`, which `tests/host/guest-agent-check` points at the configured
    // drive. Gigabytes belong there, and `--clean` looks for this name.
    let root = std::env::temp_dir().join(format!("vivarium-agent-host-{nonce}"));
    // The runtime half does not, and the reason is a hard limit rather than tidiness. A Unix
    // socket path cannot exceed 108 bytes, and this directory is where the launcher binds
    // `ws0.sock`, `store.sock`, `api.sock` and `ready.sock`. Under a drive-backed
    // `TMPDIR` the longest of them measured 109 — one byte over — and the whole lane failed as
    // `child startup child exited unexpectedly with status Some(1)`, two seconds in, saying
    // nothing about a path length. `TempProject` in `tests/support/mod.rs` learned this in
    // slice 012 and moved for the same reason; this trial did not, and only started failing
    // when the lane began redirecting `TMPDIR` at a drive whose mountpoint is long.
    //
    // `/run/user/<uid>` is short, is a per-user tmpfs, and is where runtime files belong.
    let base = PathBuf::from(format!("/run/user/{}", vivarium::config::effective_uid()))
        .join(format!("viv-agent-{nonce}"));
    let runtime = base.join("runtime");
    tokio::fs::create_dir_all(&runtime).await.unwrap();
    tokio::fs::create_dir_all(&root).await.unwrap();
    // The supervisor refuses a launch specification whose directory is not private, which
    // is the runtime-directory rule spec/02 states. `create_dir_all` applies the ambient
    // umask, so the mode is set rather than inherited: a test that prepares the directory
    // itself has to satisfy the same rule the runner's own `umask 077` satisfies.
    for directory in [&root, &base, &runtime] {
        tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
    }
    // Beside the runtime directory rather than inside it: the supervisor's cleanup refuses to
    // proceed when it finds a runtime artifact it does not own, and this socket is the
    // harness's own listener rather than one of the launcher's.
    let agent_path = base.join("ssh-agent.sock");
    let agent = UnixListener::bind(&agent_path).unwrap();
    let echo = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = agent.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bytes = [0_u8; 4096];
                loop {
                    let Ok(count) = stream.read(&mut bytes).await else {
                        return;
                    };
                    if count == 0 {
                        return;
                    }
                    if stream.write_all(&bytes[..count]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    // A previous run that failed mid-assertion never reached its teardown, and its unit
    // name is derived from the project id rather than the nonce, so it is still loaded and
    // `systemd-run` refuses the new one. The runbook deliberately runs this lane twice, so
    // clearing the name first is what makes the second run mean anything.
    stop_unit().await;

    let boot_started = Instant::now();
    let rendered = Command::new(runner)
        .args([
            "--mount",
            "mnt0",
            "dir",
            std::env::current_dir().unwrap().to_str().unwrap(),
            "-",
            "--runtime-dir",
            runtime.to_str().unwrap(),
            // The directory, not the images: the launcher names each one from the build, so a
            // lane that picked its own filenames would be testing a mapping the product does
            // not have.
            "--volume-dir",
            root.to_str().unwrap(),
            "--supervisor",
            env!("CARGO_BIN_EXE_vivarium-supervisor"),
            "--uid",
            "1000",
            "--gid",
            "1000",
            "--memory-mib",
            "2048",
            "--vcpu",
            "2",
            "--sandbox-id",
            "agent-check",
            "--target",
            "default",
            "--ssh-agent-socket",
            agent_path.to_str().unwrap(),
        ])
        .status()
        .await
        .unwrap();
    assert!(
        rendered.success(),
        "runner could not render the launch specification; diagnostics retained at {}",
        root.display()
    );

    // ADR-0102 split the build-owned runner from the running installation's launch handoff. The
    // runner renders `launch.json`; the current `viv` starts the transient supervisor and waits for
    // its readiness report. Keeping both calls here makes this lane exercise the same boundary as
    // lifecycle rather than treating a successfully rendered file as a booted guest.
    let launched = Command::new(env!("CARGO_BIN_EXE_viv"))
        .args([
            "start",
            "--spec",
            runtime.join("launch.json").to_str().unwrap(),
        ])
        .status()
        .await
        .unwrap();
    let boot_elapsed = boot_started.elapsed();
    assert!(
        launched.success(),
        "launch handoff failed; diagnostics retained at {}",
        root.display()
    );

    // Readiness ordering: the supervisor reports ready only after the current-boot agent
    // ping and the initial credential pool, so the runner having exited zero means the
    // agent was reachable before command readiness was reported. Every assertion below
    // runs after this point and none of them wait for the agent to appear.
    record("runner start to readiness", &format!("{boot_elapsed:?}"));

    let metadata: BootMetadata =
        serde_json::from_slice(&tokio::fs::read(runtime.join("boot.json")).await.unwrap()).unwrap();

    // --- Transport ------------------------------------------------------------------
    // The agent drops every peer whose context id is not the host's, so a `Pong` is also
    // the observation that Cloud Hypervisor presents the host as that id.
    let ping_started = Instant::now();
    let mut ping = connect_control(&runtime, &metadata).await;
    write_client_frame(&mut ping, &ClientFrame::Ping)
        .await
        .unwrap();
    assert_eq!(read_agent_frame(&mut ping).await.unwrap(), AgentFrame::Pong);
    record(
        "connect to first pong",
        &format!("{:?}", ping_started.elapsed()),
    );

    // --- Authorization --------------------------------------------------------------
    stale_identity_executes_nothing(&runtime, &metadata).await;

    // --- Sessions -------------------------------------------------------------------
    let (stdout, stderr, status) = execute(
        &runtime,
        &metadata,
        vec![
            bytes(b"/bin/sh"),
            bytes(b"-c"),
            bytes(b"printf out; printf err >&2; exit 42"),
        ],
        b"",
        false,
    )
    .await;
    assert_eq!(
        (stdout, stderr, status),
        (b"out".to_vec(), b"err".to_vec(), 42)
    );

    let (_, status) = sh(&runtime, &metadata, "exit 0").await;
    assert_eq!(status, 0, "a successful command did not report zero");
    let (_, status) = sh(&runtime, &metadata, "kill -TERM $$").await;
    assert_eq!(status, 143, "a signalled guest did not report 128+15");

    let (stdout, _, status) = execute(
        &runtime,
        &metadata,
        vec![bytes(b"/bin/sh"), bytes(b"-c"), bytes(b"stty size")],
        b"",
        true,
    )
    .await;
    assert_eq!(status, 0);
    assert!(String::from_utf8_lossy(&stdout).contains("31 97"));

    concurrent_sessions_run_at_once(&runtime, &metadata).await;

    // --- Guest posture --------------------------------------------------------------
    service_posture_matches_the_declaration(&runtime, &metadata).await;
    credential_socket_is_owned_by_the_guest_user(&runtime, &metadata).await;
    credential_relay_survives_pool_depletion(&runtime, &metadata).await;
    a_guest_local_peer_is_rejected_on_both_ports(&runtime, &metadata).await;

    // --- Measured bounds ------------------------------------------------------------
    measure_exit_drain(&runtime, &metadata).await;
    measure_disconnect_grace(&runtime, &metadata).await;

    // The agent restarting is what makes every assertion above repeatable across a crash,
    // and it invalidates the sessions the rest of the lane used, so it runs last.
    the_agent_restarts_after_a_crash(&runtime, &metadata).await;

    stop_unit().await;
    echo.abort();
    // Once the supervisor has cleaned the runtime directory this holds no evidence, so a
    // passing run takes it with it. A failing one panics before here and leaves it beside the
    // diagnostics root, which is where a reader is told to look.
    let _result = tokio::fs::remove_dir_all(&base).await;
}

/// Stop and forget this lane's transient unit, whatever state it is in.
async fn stop_unit() {
    for verb in ["stop", "reset-failed"] {
        let _ = Command::new("systemctl")
            .args(["--user", verb, "vivarium-agent-check-default.service"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }
}

/// A stale boot identity is refused, and refused before anything runs.
///
/// Asserting only the `Error` frame would leave the more important half unproven: that the
/// rejected connection cannot still reach a session. The marker file is what settles it.
async fn stale_identity_executes_nothing(runtime: &Path, metadata: &BootMetadata) {
    const MARKER: &str = "/tmp/stale-identity-executed";

    let mut stream = connect_raw(runtime).await;
    write_client_frame(
        &mut stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: "ffffffff-ffff-ffff-ffff-ffffffffffff".to_owned(),
        }),
    )
    .await
    .unwrap();
    match read_agent_frame(&mut stream).await.unwrap() {
        AgentFrame::Error(error) => assert_eq!(error.code, "authorization"),
        frame => panic!("a stale identity was not refused: {frame:?}"),
    }
    // The refusal closes the connection; a `Start` written after it must reach nothing.
    let _ = write_client_frame(
        &mut stream,
        &ClientFrame::Start(StartRequest {
            mode: SessionMode::Exec,
            argv: vec![
                bytes(b"/bin/sh"),
                bytes(b"-c"),
                bytes(format!("touch {MARKER}").as_bytes()),
            ],
            environment: Vec::new(),
            cwd: bytes(b"/"),
            pty: false,
        }),
    )
    .await;
    assert!(
        read_agent_frame(&mut stream).await.is_err(),
        "the connection stayed open after an authorization failure"
    );
    drop(stream);

    let (_, status) = sh(runtime, metadata, &format!("test -e {MARKER}")).await;
    assert_ne!(
        status, 0,
        "a connection that failed authorization still executed its request"
    );
}

/// Sessions run concurrently, and each keeps its own streams and status.
///
/// The wall-clock bound is what makes this an assertion about concurrency: without it a
/// serialized agent would satisfy every other check here.
async fn concurrent_sessions_run_at_once(runtime: &Path, metadata: &BootMetadata) {
    const SESSIONS: u8 = 8;
    const HOLD: Duration = Duration::from_secs(2);

    let started = Instant::now();
    let mut live = tokio::task::JoinSet::new();
    for index in 0..SESSIONS {
        let runtime = runtime.to_path_buf();
        let metadata = metadata.clone();
        live.spawn(async move {
            let sentinel = format!("live-{index}");
            let (stdout, _, status) = execute(
                &runtime,
                &metadata,
                vec![
                    bytes(b"/bin/sh"),
                    bytes(b"-c"),
                    bytes(format!("printf %s {sentinel}; sleep 2").as_bytes()),
                ],
                b"",
                false,
            )
            .await;
            assert_eq!(
                String::from_utf8_lossy(&stdout),
                sentinel,
                "session {index} received another session's output"
            );
            assert_eq!(status, 0);
        });
    }
    while let Some(finished) = live.join_next().await {
        finished.unwrap();
    }
    let elapsed = started.elapsed();
    record("eight concurrent sessions", &format!("{elapsed:?}"));
    assert!(
        elapsed < HOLD * u32::from(SESSIONS) / 2,
        "sessions did not run concurrently: \
        {SESSIONS} sessions holding {HOLD:?} each took {elapsed:?}"
    );
}

/// The running agent unit matches what the image declares.
///
/// Reading the live unit rather than the Nix expression is the whole point: it is also the
/// observation that an unprivileged user with no capabilities can bind both vsock ports,
/// because the agent that answered every frame above is the one described here.
async fn service_posture_matches_the_declaration(runtime: &Path, metadata: &BootMetadata) {
    let (shown, status) = sh(
        runtime,
        metadata,
        "systemctl show vivarium-agent \
        -p User -p Group -p NoNewPrivileges -p CapabilityBoundingSet \
        -p AmbientCapabilities -p Restart -p RuntimeDirectoryMode",
    )
    .await;
    assert_eq!(status, 0, "could not read the agent unit: {shown}");
    for expected in [
        "User=vivarium",
        "Group=vivarium",
        "NoNewPrivileges=yes",
        "CapabilityBoundingSet=",
        "AmbientCapabilities=",
        "Restart=on-failure",
        "RuntimeDirectoryMode=0700",
    ] {
        assert!(
            shown.lines().any(|line| line.trim() == expected),
            "the running agent unit lacks {expected}; it reports:\n{shown}"
        );
    }
    record("agent unit posture", "matches nix/guest.nix");
}

/// The credential socket is reachable only by the guest user, and `SSH_AUTH_SOCK` names it.
///
/// The relay working proves bytes move; it does not prove which path the guest process was
/// told to use, nor that no other guest user could reach it. Both are asserted directly.
async fn credential_socket_is_owned_by_the_guest_user(runtime: &Path, metadata: &BootMetadata) {
    let (shown, status) = sh(
        runtime,
        metadata,
        "stat -c '%U %G %a %F' /run/vivarium; stat -c '%U %G %a %F' /run/vivarium/ssh-agent.sock",
    )
    .await;
    assert_eq!(status, 0, "could not stat the credential paths: {shown}");
    let mut lines = shown.lines();
    assert_eq!(
        lines.next().unwrap().trim(),
        "vivarium vivarium 700 directory"
    );
    assert_eq!(lines.next().unwrap().trim(), "vivarium vivarium 600 socket");

    // The request deliberately carries a host path for `SSH_AUTH_SOCK`; the guest's own
    // socket must win, on both spawn paths, which set the variable independently.
    let (shown, status) = sh(runtime, metadata, "printf %s \"$SSH_AUTH_SOCK\"").await;
    assert_eq!(status, 0);
    assert_eq!(shown, "/run/vivarium/ssh-agent.sock");

    let (stdout, _, status) = execute(
        runtime,
        metadata,
        vec![
            bytes(b"/bin/sh"),
            bytes(b"-c"),
            bytes(b"printf %s \"$SSH_AUTH_SOCK\""),
        ],
        b"",
        true,
    )
    .await;
    assert_eq!(status, 0);
    assert!(
        String::from_utf8_lossy(&stdout).contains("/run/vivarium/ssh-agent.sock"),
        "the terminal spawn path did not set the guest credential socket"
    );
}

/// The pool refills, so the relay is usable more often than it is deep.
///
/// More concurrent clients than slots is what forces depletion; the sequential use this
/// replaced could never exhaust a four-slot pool and so never exercised a refill.
async fn credential_relay_survives_pool_depletion(runtime: &Path, metadata: &BootMetadata) {
    const CONCURRENT: usize = CREDENTIAL_POOL_SIZE + 2;
    const ROUNDS: usize = 20;

    /// One guest-local client of the credential socket, echoing its own sentinel.
    async fn relay(runtime: &Path, metadata: &BootMetadata, sentinel: &str, label: &str) {
        let (stdout, stderr, status) = execute(
            runtime,
            metadata,
            vec![
                bytes(b"/bin/sh"),
                bytes(b"-c"),
                // `-t 5`, not the 0.5s default: end-of-input on the client's side is not
                // end of the exchange. A client that arrives once the pool is empty holds
                // an accepted connection the guest proxy has not yet paired with a parked
                // one, and it is served when a relay ends and the host refills. Waiting is
                // the designed behaviour, so a client that gives up in half a second would
                // report the bound as a failure.
                //
                // It was 30s while the refill did not work at all, where the linger was
                // the difference between a slow lane and a hung one. With the backend at
                // v53.0 delivering the half-close, a finished relay unwinds and its slot
                // refills promptly, so the wait now covers queueing behind five other
                // clients rather than an unbounded stall. 5s is an order of magnitude over
                // the observed round time and still fails in a tenth of the old budget.
                bytes(b"socat -t 5 - UNIX-CONNECT:$SSH_AUTH_SOCK"),
            ],
            sentinel.as_bytes(),
            false,
        )
        .await;
        assert_eq!(
            (status, String::from_utf8_lossy(&stdout).as_ref()),
            (0, sentinel),
            "{label}: the relay did not echo its own bytes; stderr was {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    // One client first. The pool, the guest proxy, and the host relay are three separate
    // mechanisms, and a failure under contention says nothing about which of them broke
    // unless the uncontended case is known to work.
    relay(runtime, metadata, "opaque-single", "single client").await;

    for round in 0..ROUNDS {
        let mut clients = tokio::task::JoinSet::new();
        for index in 0..CONCURRENT {
            let runtime = runtime.to_path_buf();
            let metadata = metadata.clone();
            clients.spawn(async move {
                let sentinel = format!("opaque-{round}-{index}");
                relay(
                    &runtime,
                    &metadata,
                    &sentinel,
                    &format!("client {index} of round {round}"),
                )
                .await;
            });
        }
        while let Some(finished) = clients.join_next().await {
            finished.unwrap();
        }
    }
    record(
        "credential relay",
        &format!(
            "{CONCURRENT} concurrent clients against {CREDENTIAL_POOL_SIZE} slots, {ROUNDS} rounds"
        ),
    );
}

/// A guest-local peer is refused on both vsock ports.
///
/// The positive control runs first and fails the check rather than skipping it: the image
/// declares `vsock_loopback`, so a loopback that does not work is a defect in the image,
/// not a fact about the environment. Without it the two refusals below would read exactly
/// the same on a guest with no loopback transport at all, and prove nothing.
async fn a_guest_local_peer_is_rejected_on_both_ports(runtime: &Path, metadata: &BootMetadata) {
    let (echoed, status) = sh(
        runtime,
        metadata,
        &format!(
            // `PIPE`, not `-`, and that is the whole assertion rather than a detail.
            // `socat VSOCK-LISTEN:...,fork -` joins the accepted socket to the listener's
            // own stdio, so the probe bytes land on the listener's stdout — redirected to
            // /dev/null to keep it off the captured output — and nothing is ever sent back.
            // The client then reads end-of-file, prints nothing, and the comparison below
            // fails no matter how well loopback works. `PIPE` gives socat an unnamed pipe
            // it both reads and writes, which is the echo this control has to have to
            // observe a round trip rather than a one-way write.
            "socat VSOCK-LISTEN:{LOOPBACK_PROBE_PORT},reuseaddr,fork PIPE >/dev/null 2>&1 & \
            sleep 1; \
            printf loopback-works \
            | socat -T5 - VSOCK-CONNECT:{VMADDR_CID_LOCAL}:{LOOPBACK_PROBE_PORT}"
        ),
    )
    .await;
    assert_eq!(
        (echoed.as_str(), status),
        ("loopback-works", 0),
        "the guest cannot originate a loopback vsock connection, so the two refusals below \
        would be vacuous"
    );

    // A well-formed `Hello` on the control port. The accept loop drops the peer before
    // reading a byte, so end-of-file with no reply is the exact signature of the refusal.
    let mut hello = Vec::new();
    write_client_frame(
        &mut hello,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .unwrap();
    let encoded = base64(&hello);
    let (reply, status) = sh(
        runtime,
        metadata,
        &format!(
            "printf %s {encoded} | base64 -d \
            | socat -T5 - VSOCK-CONNECT:{VMADDR_CID_LOCAL}:{CONTROL_PORT} | wc -c"
        ),
    )
    .await;
    assert_eq!(status, 0);
    assert_eq!(
        reply, "0",
        "the control port answered a guest-local peer with {reply} bytes"
    );

    // The credential port is sharper still: admission is a single acknowledgement byte, so
    // a refusal is byte-distinguishable rather than merely quieter.
    let (reply, status) = sh(
        runtime,
        metadata,
        &format!(
            "printf '\\001' \
            | socat -T5 - VSOCK-CONNECT:{VMADDR_CID_LOCAL}:{} | wc -c",
            CONTROL_PORT + 1
        ),
    )
    .await;
    assert_eq!(status, 0);
    assert_eq!(
        reply, "0",
        "the credential port acknowledged a guest-local peer with {reply} bytes"
    );
}

/// Measure the post-exit drain against a real vsock write path.
///
/// The constant was chosen against an in-memory pair, where a write costs nothing. This is
/// the shape the deterministic test uses, run where the bytes actually cross a transport.
async fn measure_exit_drain(runtime: &Path, metadata: &BootMetadata) {
    const BYTES: usize = 96 * 1024;
    const ROUNDS: usize = 20;

    let mut worst = Duration::ZERO;
    for round in 0..ROUNDS {
        let started = Instant::now();
        let (stdout, _, status) = execute(
            runtime,
            metadata,
            vec![
                bytes(b"/bin/sh"),
                bytes(b"-c"),
                bytes(b"head -c 98304 /dev/zero | tr '\\0' x & exit 5"),
            ],
            b"",
            false,
        )
        .await;
        let elapsed = started.elapsed();
        assert_eq!(status, 5);
        assert_eq!(
            stdout.len(),
            BYTES,
            "round {round} lost inherited output: {} of {BYTES} bytes",
            stdout.len()
        );
        worst = worst.max(elapsed);
    }
    record(
        "exit drain, worst of 20 (whole session, drain-bound)",
        &format!("{worst:?}"),
    );
}

/// Measure the disconnect escalation against a guest that refuses the first signal.
///
/// Nothing has ever run this path against a real guest; the deterministic test asserts the
/// bound holds in memory, and this is what says the same of the guest's own process table.
async fn measure_disconnect_grace(runtime: &Path, metadata: &BootMetadata) {
    let mut abandoned = connect_control(runtime, metadata).await;
    write_client_frame(
        &mut abandoned,
        &ClientFrame::Start(StartRequest {
            mode: SessionMode::Exec,
            argv: vec![
                bytes(b"/bin/sh"),
                bytes(b"-c"),
                bytes(b"trap '' TERM; echo $$; while :; do sleep 1; done"),
            ],
            environment: Vec::new(),
            cwd: bytes(b"/"),
            pty: false,
        }),
    )
    .await
    .unwrap();
    let pid = match read_agent_frame(&mut abandoned).await.unwrap() {
        AgentFrame::Stdout(bytes) => String::from_utf8_lossy(&bytes).trim().to_owned(),
        frame => panic!("the abandoned session did not report its pid: {frame:?}"),
    };
    assert!(!pid.is_empty());

    let dropped = Instant::now();
    drop(abandoned);
    let (_, status) = sh(
        runtime,
        metadata,
        &format!("for _ in $(seq 200); do test -d /proc/{pid} || exit 0; sleep 0.1; done; exit 1"),
    )
    .await;
    let elapsed = dropped.elapsed();
    assert_eq!(
        status, 0,
        "a term-resistant guest process outlived its abandoned session"
    );
    record(
        "disconnect to guest process death (includes the polling session's own boot)",
        &format!("{elapsed:?}"),
    );
}

/// The agent comes back after a crash, which is what `Restart=on-failure` claims.
///
/// Killing it also ends every live session and the credential relay, so this runs last.
async fn the_agent_restarts_after_a_crash(runtime: &Path, metadata: &BootMetadata) {
    let (before, status) = sh(
        runtime,
        metadata,
        "systemctl show vivarium-agent -p MainPID --value",
    )
    .await;
    assert_eq!(status, 0);

    // The session is the agent's own descendant, so it dies with it and never reports: the
    // stream ends with no `Exit` frame. That end-of-file is this step working, not failing,
    // so the frames are drained tolerantly instead of through `execute`, whose read loop
    // unwraps and would panic on exactly the outcome being provoked here.
    let mut killer = begin_session(
        runtime,
        metadata,
        vec![
            bytes(b"/bin/sh"),
            bytes(b"-c"),
            bytes(format!("kill -9 {before}").as_bytes()),
        ],
        b"",
        false,
    )
    .await;
    while read_agent_frame(&mut killer).await.is_ok() {}
    drop(killer);

    let mut after = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Ok(mut stream) = hybrid::connect(
            &runtime.join("control.sock"),
            CONTROL_PORT,
            Duration::from_secs(2),
        )
        .await
        else {
            continue;
        };
        if write_client_frame(
            &mut stream,
            &ClientFrame::Hello(Hello {
                schema_version: SCHEMA_VERSION,
                boot_identity: metadata.boot_identity.clone(),
            }),
        )
        .await
        .is_err()
            || read_agent_frame(&mut stream).await.is_err()
        {
            continue;
        }
        drop(stream);
        let (pid, status) = sh(
            runtime,
            metadata,
            "systemctl show vivarium-agent -p MainPID --value",
        )
        .await;
        if status == 0 && pid != before {
            after = pid;
            break;
        }
    }
    assert!(
        !after.is_empty() && after != before,
        "the agent did not restart: it reported {before} before the kill and {after} after"
    );
    record("agent restart", &format!("{before} replaced by {after}"));
}
