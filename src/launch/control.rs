//! The host end of the guest control plane: readiness, and one connection per session.
//!
//! Both halves speak the same opening: connect through the backend's hybrid preamble, exchange
//! `Hello`, and compare the boot identity the agent answers with against the one `boot.json`
//! records. What follows that differs — a `Ping` that proves the agent is current and closes, or a
//! `Start` and the streams it produces — and spec/12 fixes that a ping connection is not a session
//! and cannot be promoted into one.
//!
//! The guest half of this protocol is settled and host-proved (ADR-0065, slice 003), so everything
//! here is written to match it rather than to negotiate with it. Three of its rules are not
//! discoverable from the frame types and are load-bearing: at most one `Resize` may precede
//! `Start`, a standard-stream payload is bounded at 64 KiB and the encoder refuses rather than
//! splits an oversized one, and the exit status arrives already normalized — a guest killed by
//! signal `S` is reported as `128+S` by the agent, so the host returns that byte and derives none.

use crate::launch::{BootMetadata, LaunchError};
use crate::protocol::hybrid;
use crate::protocol::{
    AgentFrame, CONTROL_PORT, ClientFrame, Hello, SCHEMA_VERSION, STREAM_PAYLOAD_MAX, StartRequest,
    TerminalSize, read_agent_frame, write_client_frame,
};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::mpsc;

const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// How many agent frames may be buffered ahead of the loop that writes them out.
///
/// Bounded so a guest producing faster than the host terminal can accept it blocks the reader task
/// rather than growing without limit; the guest's own emitter is bounded the same way.
const FRAME_QUEUE: usize = 32;

/// Waits until the agent for this exact boot answers, or the deadline passes.
///
/// # Errors
///
/// Returns [`LaunchError::Readiness`] when the deadline passes with no answer, or when an agent
/// answers for a different boot — the latter immediately, because retrying cannot change it.
pub async fn wait_for_agent(
    control_socket: &Path,
    metadata: &BootMetadata,
    timeout: Duration,
) -> Result<(), LaunchError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(LaunchError::Readiness("guest agent"));
        }
        match handshake(control_socket, metadata, remaining).await {
            Ok(()) => return Ok(()),
            Err(HandshakeError::Identity) => {
                return Err(LaunchError::Readiness("current guest identity"));
            }
            Err(HandshakeError::Retry) => {
                tokio::time::sleep(RETRY_INTERVAL.min(remaining)).await;
            }
        }
    }
}

enum HandshakeError {
    Identity,
    Retry,
}

async fn handshake(
    socket: &Path,
    metadata: &BootMetadata,
    timeout: Duration,
) -> Result<(), HandshakeError> {
    let mut stream = hybrid::connect(socket, CONTROL_PORT, timeout)
        .await
        .map_err(|_| HandshakeError::Retry)?;
    write_client_frame(
        &mut stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .map_err(|_| HandshakeError::Retry)?;
    match read_agent_frame(&mut stream)
        .await
        .map_err(|_| HandshakeError::Retry)?
    {
        AgentFrame::Hello(hello)
            if hello.schema_version == SCHEMA_VERSION
                && hello.boot_identity == metadata.boot_identity => {}
        AgentFrame::Hello(_) => return Err(HandshakeError::Identity),
        _ => return Err(HandshakeError::Retry),
    }
    write_client_frame(&mut stream, &ClientFrame::Ping)
        .await
        .map_err(|_| HandshakeError::Retry)?;
    match read_agent_frame(&mut stream).await {
        Ok(AgentFrame::Pong) => Ok(()),
        _ => Err(HandshakeError::Retry),
    }
}

/// How one session ended.
///
/// The three are not degrees of the same thing. A status is the guest's own answer and vivarium
/// reports it unchanged; a refusal is the agent declining before any guest process existed; a loss
/// is vivarium's channel failing, and spec/12 forbids guessing a guest code from one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutcome {
    /// The guest process ran and exited with this status, already normalized by the agent.
    Exited(u8),
    /// The agent refused before spawning anything, naming one of its closed codes.
    Refused(String),
    /// The connection ended without an `Exit`, with what was expected when it did.
    Lost(&'static str),
}

/// Why a session could not be carried far enough to have an outcome.
///
/// Separated from [`SessionOutcome`] because the two map to different exit categories and the
/// distinction is the whole of spec/12's guest-process-start boundary: nothing here can be confused
/// with something the guest did, because none of it happened after a guest process existed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionError {
    /// The transport would not complete — the agent is not listening yet.
    NotListening,
    /// An agent answered, for a different boot than `boot.json` records.
    Identity,
    /// The control socket failed as a channel, at the named step.
    Transport(&'static str),
}

/// One `exec` or `shell` session over its own connection to the control socket.
///
/// Generic over its two output sinks so the pump can be exercised without a terminal; the process
/// boundary passes the host's own stdout and stderr, which is what makes guest output raw on the
/// host's own streams (ADR-0015).
pub struct Session<'a, O, E> {
    pub control_socket: &'a Path,
    /// Read from `boot.json`. The agent's `Hello` is compared against it, which is what identifies
    /// *which* agent answered — spec/12's whole authorization story for an unauthenticated socket.
    pub metadata: &'a BootMetadata,
    pub start: StartRequest,
    /// Sent before `Start` when known, so the session never briefly renders at the wrong size.
    /// At most one, because a second pre-`Start` resize is a protocol fault to the agent.
    pub initial_size: Option<TerminalSize>,
    /// Host standard input, already read into chunks. `None` sends `StdinEnd` immediately.
    pub input: Option<mpsc::Receiver<Vec<u8>>>,
    /// Host terminal size changes for the life of the session.
    pub resize: Option<mpsc::Receiver<TerminalSize>>,
    pub stdout: O,
    pub stderr: E,
    pub connect_timeout: Duration,
}

impl<O: AsyncWrite + Unpin, E: AsyncWrite + Unpin> Session<'_, O, E> {
    /// Establishes the session and carries it to an outcome.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] for everything that happens before the guest process could exist.
    /// Once it does, the answer is a [`SessionOutcome`] and never an error, because a guest that
    /// failed is not vivarium failing.
    pub async fn run(mut self) -> Result<SessionOutcome, SessionError> {
        let mut stream = hybrid::connect(self.control_socket, CONTROL_PORT, self.connect_timeout)
            .await
            .map_err(|_| SessionError::NotListening)?;
        greet(&mut stream, self.metadata).await?;

        if let Some(size) = self.initial_size {
            write_client_frame(&mut stream, &ClientFrame::Resize(size))
                .await
                .map_err(|_| SessionError::Transport("send the terminal size"))?;
        }
        write_client_frame(&mut stream, &ClientFrame::Start(self.start.clone()))
            .await
            .map_err(|_| SessionError::Transport("start the guest process"))?;

        let (reader, mut writer) = stream.into_split();
        let (frames_tx, mut frames) = mpsc::channel(FRAME_QUEUE);
        // A task rather than a branch of the loop below: reading a frame is not cancel-safe, and a
        // half-read frame abandoned by `select!` would desynchronise the stream rather than fail.
        let pump = tokio::spawn(read_frames(reader, frames_tx));

        let mut input = self.input;
        let mut resize = self.resize;
        if input.is_none() {
            // No input to forward at all, so the end of it is known now. Under a PTY this only
            // stops the host writing — the terminal's own end-of-file character is what a guest
            // reads, and the agent will not synthesize one.
            let _ = write_client_frame(&mut writer, &ClientFrame::StdinEnd).await;
        }

        // Only the agent's own frames decide the outcome. A write toward a guest that has already
        // exited is expected to fail — its session is gone — and treating that as a transport
        // failure would report `74` for a command that ran and produced a status. So a failed send
        // stops the sending and nothing more; what the guest did is still on its way in.
        //
        // Deliberately unbiased, for the symmetric reason: preferring the agent's frames lets a
        // guest that writes without pause starve host input, and the input it would starve is the
        // interrupt byte the user is pressing to stop it.
        let outcome = loop {
            tokio::select! {
                frame = frames.recv() => match frame {
                    Some(AgentFrame::Stdout(bytes)) => write_out(&mut self.stdout, &bytes).await?,
                    Some(AgentFrame::Stderr(bytes)) => write_out(&mut self.stderr, &bytes).await?,
                    Some(AgentFrame::Exit(status)) => break SessionOutcome::Exited(status.status),
                    Some(AgentFrame::Error(error)) => break SessionOutcome::Refused(error.code),
                    // `Hello` and `Pong` belong to the opening and to a ping connection; either one
                    // arriving mid-session means the ends disagree about what this connection is.
                    Some(AgentFrame::Hello(_) | AgentFrame::Pong) => {
                        break SessionOutcome::Lost("a session frame");
                    }
                    None => break SessionOutcome::Lost("the guest's exit status"),
                },
                chunk = recv(&mut input) => {
                    let sent = if let Some(bytes) = chunk {
                        send_input(&mut writer, &bytes).await.is_ok()
                    } else {
                        input = None;
                        write_client_frame(&mut writer, &ClientFrame::StdinEnd)
                            .await
                            .is_ok()
                    };
                    if !sent {
                        input = None;
                    }
                }
                size = recv(&mut resize) => match size {
                    Some(size) => {
                        let frame = ClientFrame::Resize(size);
                        if write_client_frame(&mut writer, &frame).await.is_err() {
                            resize = None;
                        }
                    }
                    None => resize = None,
                },
            }
        };
        pump.abort();
        Ok(outcome)
    }
}

/// Opens a connection: `Hello` out, `Hello` back, and the boot identity compared.
async fn greet<S>(stream: &mut S, metadata: &BootMetadata) -> Result<(), SessionError>
where
    S: tokio::io::AsyncRead + AsyncWrite + Unpin,
{
    write_client_frame(
        stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .map_err(|_| SessionError::Transport("greet the guest agent"))?;
    match read_agent_frame(stream)
        .await
        .map_err(|_| SessionError::Transport("read the guest agent's greeting"))?
    {
        AgentFrame::Hello(hello)
            if hello.schema_version == SCHEMA_VERSION
                && hello.boot_identity == metadata.boot_identity =>
        {
            Ok(())
        }
        // Two spellings of the same answer. A `Hello` for another boot is the agent stating whose
        // it is; an `Error` here is the agent having already decided the greeting was not for it
        // and closing — its `authorization` code is exactly this case seen from the other side.
        AgentFrame::Hello(_) | AgentFrame::Error(_) => Err(SessionError::Identity),
        _ => Err(SessionError::Transport("read the guest agent's greeting")),
    }
}

/// Forwards agent frames until the stream ends or faults, dropping the sender either way.
async fn read_frames(mut reader: OwnedReadHalf, frames: mpsc::Sender<AgentFrame>) {
    while let Ok(frame) = read_agent_frame(&mut reader).await {
        if frames.send(frame).await.is_err() {
            return;
        }
    }
}

/// Sends one chunk of host input, split to what the encoder will accept.
///
/// The encoder refuses an oversized standard-stream payload rather than splitting it, so a reader
/// that handed over more than the bound in one read would otherwise fail the session outright.
async fn send_input<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> Result<(), crate::protocol::FrameError> {
    for piece in bytes.chunks(STREAM_PAYLOAD_MAX) {
        write_client_frame(writer, &ClientFrame::Stdin(piece.to_vec())).await?;
    }
    Ok(())
}

/// Writes guest output through, flushing so a prompt with no newline still appears.
async fn write_out<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> Result<(), SessionError> {
    writer
        .write_all(bytes)
        .await
        .map_err(|_| SessionError::Transport("write guest output"))?;
    writer
        .flush()
        .await
        .map_err(|_| SessionError::Transport("write guest output"))
}

/// Awaits a channel that may already be finished, without letting a finished one win the race.
async fn recv<T>(channel: &mut Option<mpsc::Receiver<T>>) -> Option<T> {
    match channel {
        Some(receiver) => receiver.recv().await,
        // `select!` polls every branch, so a closed channel has to park rather than return: a
        // ready `None` here would spin the loop instead of waiting for the guest.
        None => std::future::pending().await,
    }
}

#[cfg(test)]
// A panic here is the failure report: these assert the shape of frames a fake agent received, and
// the wrong shape has nothing to return.
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::protocol::{read_client_frame, write_agent_frame};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    /// A bindable socket path. See the note on the matching helper in `credentials.rs`: a
    /// `TMPDIR` on another drive overruns the 108-byte socket limit, so the per-user runtime
    /// tmpfs is preferred and `TMPDIR` is the fallback.
    fn socket_path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base =
            std::path::PathBuf::from(format!("/run/user/{}", crate::config::effective_uid()));
        let base = if base.is_dir() {
            base
        } else {
            std::env::temp_dir()
        };
        base.join(format!(
            "viv-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn metadata(identity: &str) -> BootMetadata {
        BootMetadata {
            schema_version: SCHEMA_VERSION,
            boot_identity: identity.to_owned(),
            sandbox_id: "p".to_owned(),
            target: "t".to_owned(),
            backend: "cloud-hypervisor".to_owned(),
            workspace_host_paths: std::collections::BTreeMap::from([(
                "ws0".to_owned(),
                "/workspace".into(),
            )]),
        }
    }

    #[tokio::test]
    async fn readiness_requires_current_identity_and_pong() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let identity = "01234567-89ab-cdef-0123-456789abcdef";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = [0; 14];
            stream.read_exact(&mut line).await.unwrap();
            assert_eq!(&line, b"CONNECT 52000\n");
            stream.write_all(b"OK 40000\n").await.unwrap();
            let ClientFrame::Hello(hello) = read_client_frame(&mut stream).await.unwrap() else {
                unreachable!()
            };
            write_agent_frame(
                &mut stream,
                &AgentFrame::Hello(Hello {
                    schema_version: SCHEMA_VERSION,
                    boot_identity: hello.boot_identity,
                }),
            )
            .await
            .unwrap();
            assert_eq!(
                read_client_frame(&mut stream).await.unwrap(),
                ClientFrame::Ping
            );
            write_agent_frame(&mut stream, &AgentFrame::Pong)
                .await
                .unwrap();
        });
        wait_for_agent(&path, &metadata(identity), Duration::from_secs(1))
            .await
            .unwrap();
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    const IDENTITY: &str = "01234567-89ab-cdef-0123-456789abcdef";

    fn request(pty: bool) -> StartRequest {
        StartRequest {
            mode: crate::protocol::SessionMode::Exec,
            argv: vec![crate::protocol::UnixBytes::new(b"true".to_vec())],
            environment: Vec::new(),
            // Shaped like the mirrored path a session really starts in (ADR-0100), not like the
            // share's internal mount point: this stands in for what `viv exec` sends.
            cwd: crate::protocol::UnixBytes::new(b"/home/u/Projects/demo".to_vec()),
            pty,
        }
    }

    fn session<'a>(
        path: &'a Path,
        metadata: &'a BootMetadata,
        pty: bool,
    ) -> Session<'a, Vec<u8>, Vec<u8>> {
        Session {
            control_socket: path,
            metadata,
            start: request(pty),
            initial_size: None,
            input: None,
            resize: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            connect_timeout: Duration::from_secs(2),
        }
    }

    /// Accepts one connection and completes the backend preamble and the greeting.
    async fn accept_greeted(listener: &UnixListener) -> tokio::net::UnixStream {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut line = [0; 14];
        stream.read_exact(&mut line).await.unwrap();
        assert_eq!(&line, b"CONNECT 52000\n");
        stream.write_all(b"OK 40000\n").await.unwrap();
        let ClientFrame::Hello(hello) = read_client_frame(&mut stream).await.unwrap() else {
            unreachable!()
        };
        write_agent_frame(
            &mut stream,
            &AgentFrame::Hello(Hello {
                schema_version: SCHEMA_VERSION,
                boot_identity: hello.boot_identity,
            }),
        )
        .await
        .unwrap();
        stream
    }

    /// Pins the whole opening a session performs, and that a status comes back as the guest's byte.
    ///
    /// The agent's ordering rules are asserted from the agent's side, because they are the ones a
    /// host client gets wrong: exactly one `Resize` may precede `Start`, and a second one closes
    /// the connection with a state error rather than resizing anything.
    #[tokio::test]
    async fn a_session_sizes_the_terminal_before_it_starts_the_process() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept_greeted(&listener).await;
            assert_eq!(
                read_client_frame(&mut stream).await.unwrap(),
                ClientFrame::Resize(TerminalSize {
                    rows: 31,
                    columns: 97
                })
            );
            let ClientFrame::Start(start) = read_client_frame(&mut stream).await.unwrap() else {
                panic!("the size did not precede the start");
            };
            assert!(start.pty);
            assert_eq!(
                read_client_frame(&mut stream).await.unwrap(),
                ClientFrame::StdinEnd
            );
            write_agent_frame(&mut stream, &AgentFrame::Stdout(b"out".to_vec()))
                .await
                .unwrap();
            write_agent_frame(&mut stream, &AgentFrame::Stderr(b"err".to_vec()))
                .await
                .unwrap();
            write_agent_frame(
                &mut stream,
                &AgentFrame::Exit(crate::protocol::ExitStatus { status: 143 }),
            )
            .await
            .unwrap();
        });

        let metadata = metadata(IDENTITY);
        let mut session = session(&path, &metadata, true);
        session.initial_size = Some(TerminalSize {
            rows: 31,
            columns: 97,
        });
        let outcome = session.run().await.unwrap();
        // The agent normalizes a signal death into `128+S` itself, so the host returns the byte it
        // was given. Deriving it again here is how the two ends would come to disagree.
        assert_eq!(outcome, SessionOutcome::Exited(143));
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    /// Pins that host input is chunked to what the encoder accepts rather than refused by it.
    #[tokio::test]
    async fn host_input_is_split_to_the_payload_bound() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept_greeted(&listener).await;
            let ClientFrame::Start(_) = read_client_frame(&mut stream).await.unwrap() else {
                panic!("expected a start")
            };
            let mut received = 0;
            let mut frames = 0;
            loop {
                match read_client_frame(&mut stream).await.unwrap() {
                    ClientFrame::Stdin(bytes) => {
                        assert!(bytes.len() <= STREAM_PAYLOAD_MAX);
                        received += bytes.len();
                        frames += 1;
                    }
                    ClientFrame::StdinEnd => break,
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert_eq!(received, STREAM_PAYLOAD_MAX + 1);
            assert_eq!(frames, 2, "one oversized write must become two frames");
            write_agent_frame(
                &mut stream,
                &AgentFrame::Exit(crate::protocol::ExitStatus { status: 0 }),
            )
            .await
            .unwrap();
        });

        let (tx, rx) = mpsc::channel(1);
        tx.send(vec![b'x'; STREAM_PAYLOAD_MAX + 1]).await.unwrap();
        drop(tx);
        let metadata = metadata(IDENTITY);
        let mut session = session(&path, &metadata, false);
        session.input = Some(rx);
        assert_eq!(session.run().await.unwrap(), SessionOutcome::Exited(0));
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    /// Pins the guest-process-start boundary in both directions.
    ///
    /// A refusal and a lost connection are the two things that must never be reported as a guest
    /// status, because a guest status is a number the caller propagates.
    #[tokio::test]
    async fn a_refusal_and_a_loss_are_not_guest_statuses() {
        for (script, expected) in [
            ("refuse", SessionOutcome::Refused("spawn".to_owned())),
            ("hang-up", SessionOutcome::Lost("the guest's exit status")),
        ] {
            let path = socket_path();
            let listener = UnixListener::bind(&path).unwrap();
            let server = tokio::spawn(async move {
                let mut stream = accept_greeted(&listener).await;
                let ClientFrame::Start(_) = read_client_frame(&mut stream).await.unwrap() else {
                    panic!("expected a start")
                };
                if script == "refuse" {
                    write_agent_frame(
                        &mut stream,
                        &AgentFrame::Error(crate::protocol::ProtocolErrorMessage {
                            code: "spawn".to_owned(),
                        }),
                    )
                    .await
                    .unwrap();
                }
            });
            let metadata = metadata(IDENTITY);
            assert_eq!(
                session(&path, &metadata, false).run().await.unwrap(),
                expected
            );
            server.await.unwrap();
            let _ = std::fs::remove_file(path);
        }
    }

    /// Pins that an agent answering for another boot cannot become a session.
    ///
    /// `boot.json` is host-written metadata rather than a secret, so this comparison is the whole
    /// of what identifies which agent is on the other end of an unauthenticated socket.
    #[tokio::test]
    async fn a_session_refuses_an_agent_from_another_boot() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = [0; 14];
            stream.read_exact(&mut line).await.unwrap();
            stream.write_all(b"OK 40000\n").await.unwrap();
            let ClientFrame::Hello(_) = read_client_frame(&mut stream).await.unwrap() else {
                unreachable!()
            };
            write_agent_frame(
                &mut stream,
                &AgentFrame::Hello(Hello {
                    schema_version: SCHEMA_VERSION,
                    boot_identity: "ffffffff-89ab-cdef-0123-456789abcdef".to_owned(),
                }),
            )
            .await
            .unwrap();
            // Nothing may follow, and in particular no `Start` may be written to a stale agent.
            assert!(read_client_frame(&mut stream).await.is_err());
        });
        let metadata = metadata(IDENTITY);
        assert_eq!(
            session(&path, &metadata, false).run().await.unwrap_err(),
            SessionError::Identity
        );
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }
}
