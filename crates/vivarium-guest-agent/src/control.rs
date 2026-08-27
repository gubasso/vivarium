use crate::session;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_HOST, VsockListener};
use vivarium::protocol::{
    AgentFrame, ClientFrame, CredentialId, Hello, MemoryReport, ProtocolErrorMessage,
    SCHEMA_VERSION, SessionCount, TrimRequest, read_client_frame, write_agent_frame,
};

/// How often the trim arm looks for the fstrim unit's completion marker.
const TRIM_POLL: Duration = Duration::from_millis(250);

/// How long a trim may run before the connection answers with an error instead of hanging.
///
/// Generous because `fstrim` walks every free extent of a filesystem; a bound exists so a wedged
/// unit costs the host an error it can classify rather than a connection that never ends.
const TRIM_DEADLINE: Duration = Duration::from_mins(1);

/// The files the agent exchanges with the guest's root-owned units, and the turn-taking they
/// need.
///
/// The agent holds no privilege (its bounding set is empty), so every privileged act is a file
/// with one meaning that a root-owned path unit watches. The poweroff trigger promises motion;
/// the fstrim pair promises completion — the request names the mountpoints, and the unit moves
/// the processed request into the done marker only after `fstrim` finished, because the host
/// reads the image's allocation after the acknowledgement. Single files with single meanings
/// cannot serve two requests at once, so `trim_serial` makes overlapping trim connections take
/// turns rather than consume each other's marker.
#[derive(Clone)]
pub struct Triggers {
    pub poweroff: PathBuf,
    pub fstrim_request: PathBuf,
    pub fstrim_done: PathBuf,
    pub trim_serial: Arc<tokio::sync::Mutex<()>>,
}

/// Decrement-on-drop half of the live-session count, so a session that ends by panic or by
/// error leaves the number as honest as one that ends cleanly.
struct SessionLive(Arc<AtomicU64>);

impl Drop for SessionLive {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

pub async fn accept_loop(
    listener: VsockListener,
    boot_identity: String,
    credentials: Vec<CredentialId>,
    triggers: Triggers,
    cancellation: CancellationToken,
) -> Result<(), std::io::Error> {
    // spec/12: sessions are counted, not tracked. One integer is the entire session state this
    // loop keeps — no ids, no registry, nothing the host could fall out of sync with.
    let sessions = Arc::new(AtomicU64::new(0));
    loop {
        tokio::select! {
            () = cancellation.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                // ADR-0065's authorization rests on the guest being unable to originate a
                // control connection. The listener binds `VMADDR_CID_ANY`, which also admits
                // a guest-local loopback peer, so host origin is checked rather than assumed.
                if peer.cid() != VMADDR_CID_HOST {
                    continue;
                }
                let identity = boot_identity.clone();
                let credentials = credentials.clone();
                let sessions = Arc::clone(&sessions);
                let triggers = triggers.clone();
                tokio::spawn(async move {
                    let _ = Box::pin(handle(stream, &identity, &credentials, sessions, &triggers))
                        .await;
                });
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one arm per protocol request, not logic
pub async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    expected_identity: &str,
    credentials: &[CredentialId],
    sessions: Arc<AtomicU64>,
    triggers: &Triggers,
) -> Result<(), ()> {
    let hello = match read_client_frame(&mut stream).await {
        Ok(ClientFrame::Hello(hello))
            if hello.schema_version == SCHEMA_VERSION
                && hello.boot_identity == expected_identity =>
        {
            hello
        }
        Ok(_) => {
            safe_error(&mut stream, "authorization").await;
            return Err(());
        }
        Err(_) => {
            safe_error(&mut stream, "framing").await;
            return Err(());
        }
    };
    if write_agent_frame(
        &mut stream,
        &AgentFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: hello.boot_identity,
        }),
    )
    .await
    .is_err()
    {
        return Err(());
    }
    let mut initial_size = None;
    loop {
        match read_client_frame(&mut stream).await {
            Ok(ClientFrame::Ping) => {
                write_agent_frame(&mut stream, &AgentFrame::Pong)
                    .await
                    .map_err(|_| ())?;
                return Ok(());
            }
            // A query connection ends like a ping connection: it reports the count and can
            // never be promoted into a session, so it never counts itself.
            Ok(ClientFrame::Sessions) => {
                write_agent_frame(
                    &mut stream,
                    &AgentFrame::Sessions(SessionCount {
                        sessions: sessions.load(Ordering::Relaxed),
                    }),
                )
                .await
                .map_err(|_| ())?;
                return Ok(());
            }
            // The one request with one meaning (spec/12): touch the trigger the root-owned
            // path unit watches, then acknowledge. The order is deliberate — the ack is the
            // agent's claim that the shutdown is in motion, so it follows the write that
            // makes that true. Like a ping, this connection can never become a session.
            Ok(ClientFrame::Shutdown) => {
                if tokio::fs::write(&triggers.poweroff, b"").await.is_err() {
                    safe_error(&mut stream, "shutdown").await;
                    return Err(());
                }
                write_agent_frame(&mut stream, &AgentFrame::ShutdownAck)
                    .await
                    .map_err(|_| ())?;
                return Ok(());
            }
            // A query connection like `Sessions`: it answers the kernel's own account of guest
            // memory and ends, so the host can derive a trim target it has no way to measure —
            // the scope charge it reads includes the very page cache the trim exists to drop.
            Ok(ClientFrame::Memory) => {
                let report = tokio::fs::read_to_string("/proc/meminfo")
                    .await
                    .ok()
                    .as_deref()
                    .and_then(parse_meminfo);
                let Some(report) = report else {
                    safe_error(&mut stream, "meminfo").await;
                    return Err(());
                };
                write_agent_frame(&mut stream, &AgentFrame::Memory(report))
                    .await
                    .map_err(|_| ())?;
                return Ok(());
            }
            // The disk counterpart of `Shutdown`, with the opposite promise: the acknowledgement
            // claims completion, not motion, because the host reads the image's allocated blocks
            // after it. Like the queries, this connection ends on the answer and can never become
            // a session.
            Ok(ClientFrame::Trim(request)) => {
                if run_trim(&request, triggers).await.is_err() {
                    safe_error(&mut stream, "trim").await;
                    return Err(());
                }
                write_agent_frame(&mut stream, &AgentFrame::TrimAck)
                    .await
                    .map_err(|_| ())?;
                return Ok(());
            }
            Ok(ClientFrame::Resize(size)) if initial_size.is_none() => initial_size = Some(size),
            Ok(ClientFrame::Start(request)) => {
                // A connection becomes a session at its accepted `Start` (spec/12), which is
                // where the count moves.
                sessions.fetch_add(1, Ordering::Relaxed);
                let _live = SessionLive(sessions);
                return Box::pin(session::run(
                    &mut stream,
                    request,
                    initial_size,
                    credentials,
                ))
                .await
                .map_err(|_| ());
            }
            Ok(_) => {
                safe_error(&mut stream, "state").await;
                return Err(());
            }
            Err(_) => {
                safe_error(&mut stream, "framing").await;
                return Err(());
            }
        }
    }
}

/// The per-boot order of trim tokens, which is what makes each request's marker its own.
static TRIM_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Hand the mountpoints to the root-owned fstrim unit and wait for its completion marker.
///
/// Three rules keep the completion promise honest against overlap and interruption. Requests
/// take turns (`trim_serial`), because the request path and the marker are single files with
/// single meanings. The request is staged and renamed into place, because the path unit's
/// trigger can fire on creation and must never see a partial mountpoint list. And the request
/// opens with a token line the unit carries into the marker it writes, so a marker left by a
/// predecessor that timed out is recognized and discarded rather than acknowledged as this
/// trim's completion. The agent owns the runtime directory, so it can unlink the root-written
/// marker after reading it. A request naming an empty or newline-carrying mountpoint is refused
/// rather than written, because the trigger file is line-shaped.
async fn run_trim(request: &TrimRequest, triggers: &Triggers) -> Result<(), ()> {
    if request.mountpoints.is_empty()
        || request
            .mountpoints
            .iter()
            .any(|mount| mount.is_empty() || mount.contains('\n'))
    {
        return Err(());
    }
    let _serial = triggers.trim_serial.lock().await;
    let _ = tokio::fs::remove_file(&triggers.fstrim_done).await;
    let token = format!(
        "# {}-{}",
        std::process::id(),
        TRIM_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut body = token.clone();
    body.push('\n');
    for mount in &request.mountpoints {
        body.push_str(mount);
        body.push('\n');
    }
    let staging = triggers.fstrim_request.with_extension("staging");
    tokio::fs::write(&staging, body).await.map_err(|_| ())?;
    tokio::fs::rename(&staging, &triggers.fstrim_request)
        .await
        .map_err(|_| ())?;
    let deadline = tokio::time::Instant::now() + TRIM_DEADLINE;
    let consumed = triggers.fstrim_done.with_extension("consumed");
    loop {
        // Consumed by rename first, read second: a remove after a plain read could unlink a
        // marker the unit rewrote in between, and a rename takes exactly one version.
        if tokio::fs::rename(&triggers.fstrim_done, &consumed)
            .await
            .is_ok()
        {
            let marker = tokio::fs::read_to_string(&consumed)
                .await
                .unwrap_or_default();
            let _ = tokio::fs::remove_file(&consumed).await;
            if marker.lines().next() == Some(token.as_str()) {
                return Ok(());
            }
            // A different token is a timed-out predecessor's completion arriving late; the
            // serialization above guarantees no live request owns it, so the wait for this
            // trim's own marker continues.
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(());
        }
        tokio::time::sleep(TRIM_POLL).await;
    }
}

/// The two `/proc/meminfo` figures the report carries, both kB lines by kernel contract.
fn parse_meminfo(text: &str) -> Option<MemoryReport> {
    let field = |name: &str| {
        text.lines().find_map(|line| {
            line.strip_prefix(name)?
                .trim()
                .strip_suffix("kB")?
                .trim()
                .parse::<u64>()
                .ok()
        })
    };
    Some(MemoryReport {
        total_bytes: field("MemTotal:")?.checked_mul(1024)?,
        available_bytes: field("MemAvailable:")?.checked_mul(1024)?,
    })
}

async fn safe_error<S: AsyncWrite + Unpin>(stream: &mut S, code: &str) {
    let _ = write_agent_frame(
        stream,
        &AgentFrame::Error(ProtocolErrorMessage {
            code: code.to_owned(),
        }),
    )
    .await;
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, duplex};
    use vivarium::protocol::{
        SCHEMA_VERSION, SessionMode, SignalRequest, StartRequest, UnixBytes, read_agent_frame,
        write_client_frame,
    };

    const ID: &str = "01234567-89ab-cdef-0123-456789abcdef";

    /// Open one connection to a fresh session handler and complete the handshake.
    fn connected() -> (DuplexStream, tokio::task::JoinHandle<Result<(), ()>>) {
        connected_counting(&Arc::new(AtomicU64::new(0)))
    }

    /// [`connected`], sharing the caller's live-session counter the way [`accept_loop`] does.
    fn connected_counting(
        sessions: &Arc<AtomicU64>,
    ) -> (DuplexStream, tokio::task::JoinHandle<Result<(), ()>>) {
        // A handler that is not asked to shut down never touches the trigger, so a fixed
        // never-created path keeps every other test honest about that.
        connected_with_trigger(
            sessions,
            std::env::temp_dir().join("vivarium-agent-untouched"),
        )
    }

    /// [`connected_counting`], with the poweroff trigger at a caller-chosen path.
    fn connected_with_trigger(
        sessions: &Arc<AtomicU64>,
        trigger: PathBuf,
    ) -> (DuplexStream, tokio::task::JoinHandle<Result<(), ()>>) {
        connected_with_triggers(
            sessions,
            Triggers {
                poweroff: trigger,
                fstrim_request: std::env::temp_dir().join("vivarium-agent-untouched"),
                fstrim_done: std::env::temp_dir().join("vivarium-agent-untouched"),
                trim_serial: Arc::new(tokio::sync::Mutex::new(())),
            },
        )
    }

    /// The widest fixture: every trigger file at a caller-chosen path.
    fn connected_with_triggers(
        sessions: &Arc<AtomicU64>,
        triggers: Triggers,
    ) -> (DuplexStream, tokio::task::JoinHandle<Result<(), ()>>) {
        let (client, server) = duplex(1024 * 1024);
        let sessions = Arc::clone(sessions);
        (
            client,
            tokio::spawn(async move { handle(server, ID, &[], sessions, &triggers).await }),
        )
    }

    async fn handshake(client: &mut DuplexStream) {
        write_client_frame(
            client,
            &ClientFrame::Hello(Hello {
                schema_version: SCHEMA_VERSION,
                boot_identity: ID.to_owned(),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(client).await.unwrap(),
            AgentFrame::Hello(_)
        ));
    }

    /// Ask a session handler to run one script and collect its output and exit status.
    async fn exec(client: &mut DuplexStream, script: String, pty: bool) -> (Vec<u8>, u8) {
        write_client_frame(
            client,
            &ClientFrame::Start(StartRequest {
                mode: SessionMode::Exec,
                argv: vec![
                    UnixBytes::new(b"/bin/sh".to_vec()),
                    UnixBytes::new(b"-c".to_vec()),
                    UnixBytes::new(script.into_bytes()),
                ],
                environment: Vec::new(),
                cwd: UnixBytes::new(b"/".to_vec()),
                pty,
            }),
        )
        .await
        .unwrap();
        let mut output = Vec::new();
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(20), read_agent_frame(client))
                .await
                .expect("session produced no exit")
                .unwrap()
            {
                AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes) => output.extend(bytes),
                AgentFrame::Exit(exit) => return (output, exit.status),
                frame => panic!("unexpected frame: {frame:?}"),
            }
        }
    }

    #[tokio::test]
    async fn authenticated_ping_round_trips() {
        let (mut client, task) = connected();
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Ping)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Pong
        );
        assert!(task.await.unwrap().is_ok());
    }

    /// A per-test trigger path in the shared temp directory, unique across tests and runs.
    fn scratch_trigger(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "vivarium-agent-poweroff-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// The shutdown request touches the trigger before the acknowledgement claims it did.
    #[tokio::test]
    async fn shutdown_touches_the_trigger_and_acks() {
        let trigger = scratch_trigger("acks");
        let (mut client, task) =
            connected_with_trigger(&Arc::new(AtomicU64::new(0)), trigger.clone());
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Shutdown)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::ShutdownAck
        );
        assert!(task.await.unwrap().is_ok());
        assert!(trigger.exists());
        std::fs::remove_file(trigger).unwrap();
    }

    /// Like `Ping`, the request is honoured after a stored pre-`Start` `Resize`.
    #[tokio::test]
    async fn shutdown_after_resize_still_acks() {
        let trigger = scratch_trigger("after-resize");
        let (mut client, task) =
            connected_with_trigger(&Arc::new(AtomicU64::new(0)), trigger.clone());
        handshake(&mut client).await;
        write_client_frame(
            &mut client,
            &ClientFrame::Resize(vivarium::protocol::TerminalSize {
                rows: 24,
                columns: 80,
            }),
        )
        .await
        .unwrap();
        write_client_frame(&mut client, &ClientFrame::Shutdown)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::ShutdownAck
        );
        assert!(task.await.unwrap().is_ok());
        assert!(trigger.exists());
        std::fs::remove_file(trigger).unwrap();
    }

    /// An unwritable trigger is an error, never a false acknowledgement.
    #[tokio::test]
    async fn shutdown_with_unwritable_trigger_reports_error() {
        let trigger = scratch_trigger("unwritable").join("missing-directory/trigger");
        let (mut client, task) = connected_with_trigger(&Arc::new(AtomicU64::new(0)), trigger);
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Shutdown)
            .await
            .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Error(ProtocolErrorMessage { code }) if code == "shutdown"
        ));
        assert!(task.await.unwrap().is_err());
    }

    /// The two figures the query answers are the kernel's own, converted from its kB lines.
    #[test]
    fn meminfo_parses_the_two_kernel_lines() {
        let text = "MemTotal:        4030464 kB\nMemFree:          123456 kB\n\
            MemAvailable:    2015232 kB\nBuffers:            1024 kB\n";
        let report = parse_meminfo(text).unwrap();
        assert_eq!(report.total_bytes, 4_030_464 * 1024);
        assert_eq!(report.available_bytes, 2_015_232 * 1024);
        assert!(parse_meminfo("MemTotal: 1 kB\n").is_none());
        assert!(parse_meminfo("").is_none());
    }

    /// A live memory query answers with the running kernel's own report.
    #[tokio::test]
    async fn memory_query_answers_and_ends() {
        let (mut client, task) = connected();
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Memory)
            .await
            .unwrap();
        match read_agent_frame(&mut client).await.unwrap() {
            AgentFrame::Memory(report) => assert!(report.total_bytes > 0),
            frame => panic!("unexpected frame: {frame:?}"),
        }
        assert!(task.await.unwrap().is_ok());
    }

    /// The trim request writes the mountpoints, waits for the done marker, and only then acks.
    #[tokio::test]
    async fn trim_writes_the_request_and_acks_on_the_done_marker() {
        let request = scratch_trigger("trim-request");
        let done = scratch_trigger("trim-done");
        let (mut client, task) = connected_with_triggers(
            &Arc::new(AtomicU64::new(0)),
            Triggers {
                poweroff: scratch_trigger("trim-poweroff"),
                fstrim_request: request.clone(),
                fstrim_done: done.clone(),
                trim_serial: Arc::new(tokio::sync::Mutex::new(())),
            },
        );
        handshake(&mut client).await;
        // Stand in for the root unit: move the processed request into the done marker, the
        // token line included, exactly as `nix/fstrim-run.sh` does.
        let unit = {
            let (request, done) = (request.clone(), done.clone());
            tokio::spawn(async move {
                loop {
                    if let Ok(body) = tokio::fs::read_to_string(&request).await {
                        let mut lines = body.lines();
                        assert!(lines.next().unwrap_or_default().starts_with("# "));
                        assert_eq!(
                            lines.collect::<Vec<_>>(),
                            vec!["/home/vivarium", "/nix/.rw-store"]
                        );
                        tokio::fs::remove_file(&request).await.unwrap();
                        tokio::fs::write(&done, body).await.unwrap();
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
        };
        write_client_frame(
            &mut client,
            &ClientFrame::Trim(TrimRequest {
                mountpoints: vec!["/home/vivarium".to_owned(), "/nix/.rw-store".to_owned()],
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::TrimAck
        );
        unit.await.unwrap();
        assert!(task.await.unwrap().is_ok());
        // The agent consumed the marker after reading it; the unit consumed the request.
        assert!(!done.exists());
        assert!(!request.exists());
    }

    /// A marker carrying another request's token is discarded, never acknowledged as this one's.
    #[tokio::test]
    async fn trim_ignores_a_predecessors_marker() {
        let request = scratch_trigger("trim-stale-request");
        let done = scratch_trigger("trim-stale-done");
        let (mut client, task) = connected_with_triggers(
            &Arc::new(AtomicU64::new(0)),
            Triggers {
                poweroff: scratch_trigger("trim-stale-poweroff"),
                fstrim_request: request.clone(),
                fstrim_done: done.clone(),
                trim_serial: Arc::new(tokio::sync::Mutex::new(())),
            },
        );
        handshake(&mut client).await;
        let unit = {
            let (request, done) = (request.clone(), done.clone());
            tokio::spawn(async move {
                // A timed-out predecessor's marker lands first; the live request's own marker
                // follows only once the unit consumes the live request.
                tokio::fs::write(&done, "# 0-stale\n/somewhere\n")
                    .await
                    .unwrap();
                loop {
                    if let Ok(body) = tokio::fs::read_to_string(&request).await {
                        tokio::fs::remove_file(&request).await.unwrap();
                        tokio::fs::write(&done, body).await.unwrap();
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
        };
        write_client_frame(
            &mut client,
            &ClientFrame::Trim(TrimRequest {
                mountpoints: vec!["/home/vivarium".to_owned()],
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::TrimAck
        );
        unit.await.unwrap();
        assert!(task.await.unwrap().is_ok());
    }

    /// A line-breaking mountpoint is refused before anything is written.
    #[tokio::test]
    async fn trim_refuses_a_malformed_mountpoint() {
        let request = scratch_trigger("trim-malformed-request");
        let (mut client, task) = connected_with_triggers(
            &Arc::new(AtomicU64::new(0)),
            Triggers {
                poweroff: scratch_trigger("trim-malformed-poweroff"),
                fstrim_request: request.clone(),
                fstrim_done: scratch_trigger("trim-malformed-done"),
                trim_serial: Arc::new(tokio::sync::Mutex::new(())),
            },
        );
        handshake(&mut client).await;
        write_client_frame(
            &mut client,
            &ClientFrame::Trim(TrimRequest {
                mountpoints: vec!["/home\nvivarium".to_owned()],
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Error(ProtocolErrorMessage { code }) if code == "trim"
        ));
        assert!(task.await.unwrap().is_err());
        assert!(!request.exists());
    }

    #[tokio::test]
    async fn pre_spawn_failure_reports_error() {
        let (mut client, task) = connected();
        handshake(&mut client).await;
        write_client_frame(
            &mut client,
            &ClientFrame::Start(StartRequest {
                mode: SessionMode::Exec,
                argv: vec![UnixBytes::new(b"/vivarium/definitely-missing".to_vec())],
                environment: Vec::new(),
                cwd: UnixBytes::new(b"/".to_vec()),
                pty: false,
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Error(_)
        ));
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn invalid_identity_fails_closed_repeatedly() {
        for _ in 0..20 {
            let (mut client, task) = connected();
            write_client_frame(
                &mut client,
                &ClientFrame::Hello(Hello {
                    schema_version: SCHEMA_VERSION,
                    boot_identity: "ffffffff-ffff-ffff-ffff-ffffffffffff".to_owned(),
                }),
            )
            .await
            .unwrap();
            assert!(matches!(
                read_agent_frame(&mut client).await.unwrap(),
                AgentFrame::Error(_)
            ));
            assert!(task.await.unwrap().is_err());
        }
    }

    /// Whether a process is still running, as opposed to gone or awaiting reaping.
    ///
    /// A killed child stays visible in `/proc` as a zombie until someone reaps it, and the
    /// session leader exits without doing so, so presence alone would not distinguish a
    /// process that survived the signal from one that did not.
    fn is_running(pid: &str) -> bool {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        // The `comm` field is parenthesised and may itself contain spaces and brackets, so
        // the state that follows it is read after the last `)` rather than by counting
        // fields from the left.
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z")
    }

    /// A signal frame reaches the session's whole process group, not just its leader.
    ///
    /// The children are observed in the host's own process table rather than asked to
    /// report for themselves. An earlier version had them echo from a `TERM` trap and was
    /// flaky at roughly two runs in five, because a trap runs only once the `sleep` it
    /// interrupted returns and nothing made the leader outlive that. Whether a process
    /// exists is not a race; whether it managed to write first is.
    #[tokio::test]
    async fn a_signal_frame_reaches_the_whole_process_group() {
        for _ in 0..10 {
            let (mut client, task) = connected();
            handshake(&mut client).await;
            write_client_frame(
                &mut client,
                &ClientFrame::Start(StartRequest {
                    mode: SessionMode::Exec,
                    argv: vec![
                        UnixBytes::new(b"/bin/sh".to_vec()),
                        UnixBytes::new(b"-c".to_vec()),
                        UnixBytes::new(
                            b"trap 'echo leader; exit 143' TERM
                            sleep 300 & echo $!
                            sleep 300 & echo $!
                            echo ready
                            while :; do sleep 0.1; done"
                                .to_vec(),
                        ),
                    ],
                    environment: Vec::new(),
                    cwd: UnixBytes::new(b"/".to_vec()),
                    pty: false,
                }),
            )
            .await
            .unwrap();
            let mut output = Vec::new();
            while !String::from_utf8_lossy(&output).contains("ready") {
                match read_agent_frame(&mut client).await.unwrap() {
                    AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes) => output.extend(bytes),
                    frame => panic!("unexpected frame: {frame:?}"),
                }
            }
            let text = String::from_utf8_lossy(&output).into_owned();
            let children: Vec<&str> = text.lines().take(2).collect();
            assert_eq!(children.len(), 2);
            // Without this the two checks below would pass on children that were never
            // there: an assertion that a process is gone is vacuous until it was present.
            for pid in &children {
                assert!(
                    is_running(pid),
                    "child {pid} was not running before the signal"
                );
            }

            write_client_frame(
                &mut client,
                &ClientFrame::Signal(SignalRequest { signal: 15 }),
            )
            .await
            .unwrap();
            let status = loop {
                match read_agent_frame(&mut client).await.unwrap() {
                    AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes) => output.extend(bytes),
                    AgentFrame::Exit(exit) => break exit.status,
                    frame => panic!("unexpected frame: {frame:?}"),
                }
            };
            assert_eq!(status, 143);
            assert!(
                String::from_utf8_lossy(&output).contains("leader"),
                "the leader missed the signal"
            );
            for pid in &children {
                let mut gone = false;
                for _ in 0..100 {
                    if !is_running(pid) {
                        gone = true;
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                assert!(
                    gone,
                    "child {pid} outlived the signal, so it reached the leader alone"
                );
            }
            assert!(task.await.unwrap().is_ok());
        }
    }

    /// Concurrent sessions keep their own streams and status.
    ///
    /// Every session carries a sentinel unique to its round and index, so any cross-talk
    /// between the independent `handle` tasks shows up as a mismatched payload rather than
    /// as a merely plausible pass. The repeat count is what makes an intermittent race
    /// visible; one clean round would only show the outcome is possible.
    /// One query round trip against a shared counter, on a connection that then ends.
    async fn count(sessions: &Arc<AtomicU64>) -> u64 {
        let (mut client, task) = connected_counting(sessions);
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Sessions)
            .await
            .unwrap();
        let AgentFrame::Sessions(count) = read_agent_frame(&mut client).await.unwrap() else {
            panic!("expected a session count");
        };
        assert!(task.await.unwrap().is_ok());
        count.sessions
    }

    /// The count is sessions, not connections: it moves at `Start`, falls when the session
    /// ends, and neither a ping nor the query itself is ever in it (spec/12).
    #[tokio::test]
    async fn sessions_are_counted_not_tracked() {
        let sessions = Arc::new(AtomicU64::new(0));
        assert_eq!(count(&sessions).await, 0);

        // Two live sessions, parked on stdin so they stay live while counted. The printed
        // sentinel is the synchronization: stdout implies the session spawned, which implies
        // the count already moved.
        let mut live = Vec::new();
        for _ in 0..2 {
            let (mut client, task) = connected_counting(&sessions);
            handshake(&mut client).await;
            write_client_frame(
                &mut client,
                &ClientFrame::Start(StartRequest {
                    mode: SessionMode::Exec,
                    argv: vec![
                        UnixBytes::new(b"/bin/sh".to_vec()),
                        UnixBytes::new(b"-c".to_vec()),
                        UnixBytes::new(b"printf up; cat >/dev/null".to_vec()),
                    ],
                    environment: Vec::new(),
                    cwd: UnixBytes::new(b"/".to_vec()),
                    pty: false,
                }),
            )
            .await
            .unwrap();
            assert!(matches!(
                read_agent_frame(&mut client).await.unwrap(),
                AgentFrame::Stdout(_)
            ));
            live.push((client, task));
        }
        assert_eq!(count(&sessions).await, 2);

        for (mut client, task) in live {
            write_client_frame(&mut client, &ClientFrame::StdinEnd)
                .await
                .unwrap();
            loop {
                if let AgentFrame::Exit(_) = read_agent_frame(&mut client).await.unwrap() {
                    break;
                }
            }
            assert!(task.await.unwrap().is_ok());
        }
        assert_eq!(count(&sessions).await, 0);

        // A ping connection was never a session; the counter does not move for it.
        let (mut client, task) = connected_counting(&sessions);
        handshake(&mut client).await;
        write_client_frame(&mut client, &ClientFrame::Ping)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Pong
        );
        assert!(task.await.unwrap().is_ok());
        assert_eq!(count(&sessions).await, 0);
    }

    #[tokio::test]
    async fn concurrent_sessions_keep_their_own_streams_and_status() {
        for round in 0..20 {
            let mut sessions = tokio::task::JoinSet::new();
            for index in 0..6_u8 {
                sessions.spawn(async move {
                    let sentinel = format!("round-{round}-session-{index}");
                    let (mut client, task) = connected();
                    match index {
                        // A connection that never authenticates must not disturb the rest.
                        5 => {
                            write_client_frame(
                                &mut client,
                                &ClientFrame::Hello(Hello {
                                    schema_version: SCHEMA_VERSION,
                                    boot_identity: "ffffffff-ffff-ffff-ffff-ffffffffffff"
                                        .to_owned(),
                                }),
                            )
                            .await
                            .unwrap();
                            assert!(matches!(
                                read_agent_frame(&mut client).await.unwrap(),
                                AgentFrame::Error(_)
                            ));
                            assert!(task.await.unwrap().is_err());
                        }
                        4 => {
                            handshake(&mut client).await;
                            write_client_frame(&mut client, &ClientFrame::Ping)
                                .await
                                .unwrap();
                            assert_eq!(
                                read_agent_frame(&mut client).await.unwrap(),
                                AgentFrame::Pong
                            );
                            assert!(task.await.unwrap().is_ok());
                        }
                        _ => {
                            handshake(&mut client).await;
                            let (output, status) = exec(
                                &mut client,
                                format!("printf %s {sentinel}; exit {index}"),
                                index >= 2,
                            )
                            .await;
                            assert_eq!(
                                output,
                                sentinel.as_bytes(),
                                "session {index} received another session's output"
                            );
                            assert_eq!(status, index);
                            assert!(task.await.unwrap().is_ok());
                        }
                    }
                });
            }
            while let Some(finished) = sessions.join_next().await {
                finished.unwrap();
            }
        }
    }
}
