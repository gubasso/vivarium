use crate::session;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_HOST, VsockListener};
use vivarium::protocol::{
    AgentFrame, ClientFrame, CredentialId, Hello, ProtocolErrorMessage, SCHEMA_VERSION,
    SessionCount, read_client_frame, write_agent_frame,
};

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
                tokio::spawn(async move {
                    let _ = Box::pin(handle(stream, &identity, &credentials, sessions)).await;
                });
            }
        }
    }
}

pub async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    expected_identity: &str,
    credentials: &[CredentialId],
    sessions: Arc<AtomicU64>,
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
        let (client, server) = duplex(1024 * 1024);
        (
            client,
            tokio::spawn(handle(server, ID, &[], Arc::clone(sessions))),
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
