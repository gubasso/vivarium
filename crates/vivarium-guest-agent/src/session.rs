use crate::process::{self, NonPtyProcess, PtyProcess};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use vivarium::protocol::{
    AgentFrame, ClientFrame, CredentialId, ExitStatus, FrameError, ProtocolErrorMessage,
    StartRequest, read_client_frame, write_agent_frame,
};

/// Upper bound on draining terminal output after the session leader exits.
///
/// `spec/12` requires a successful spawn to produce its streams before the single
/// `Exit`, but a process the session never reaps can hold the terminal open, so the
/// drain is bounded rather than run to EOF.
const EXIT_DRAIN_LIMIT: Duration = Duration::from_millis(250);

/// Grace period between the disconnect `SIGTERM` and the `SIGKILL` that follows it.
///
/// The sandbox is disposable (ADR-0080), so an abandoned session ends promptly rather
/// than waiting on a process that declines to handle the first signal.
const DISCONNECT_GRACE: Duration = Duration::from_secs(2);

pub async fn run<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    request: StartRequest,
    initial_size: Option<vivarium::protocol::TerminalSize>,
    credentials: &[CredentialId],
) -> Result<(), FrameError> {
    if request.pty {
        let Ok(process) = process::spawn_pty(&request, credentials, initial_size) else {
            return spawn_failed(stream).await;
        };
        Box::pin(run_pty(stream, process, initial_size)).await
    } else {
        let Ok(process) = process::spawn_non_pty(&request, credentials) else {
            return spawn_failed(stream).await;
        };
        Box::pin(run_non_pty(stream, process)).await
    }
}

/// Report a pre-spawn failure as `spec/12` requires, then end the session.
///
/// The code is a fixed vocabulary word: the underlying error can name request bytes,
/// which are secret-class and never cross the wire.
async fn spawn_failed<S: AsyncWrite + Unpin>(stream: &mut S) -> Result<(), FrameError> {
    write_agent_frame(
        stream,
        &AgentFrame::Error(ProtocolErrorMessage {
            code: "spawn".to_owned(),
        }),
    )
    .await?;
    Err(process_error(process::ProcessError::InvalidRequest))
}

fn process_error(_: process::ProcessError) -> FrameError {
    FrameError::Io(std::io::Error::other("process operation failed"))
}

async fn run_non_pty<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    mut process: NonPtyProcess,
) -> Result<(), FrameError> {
    let mut stdout = [0; 16 * 1024];
    let mut stderr = [0; 16 * 1024];
    let mut stdout_open = true;
    let mut stderr_open = true;
    let mut input_ended = false;
    loop {
        tokio::select! {
            frame = read_client_frame(stream) => match frame {
                Ok(ClientFrame::Stdin(bytes)) => {
                    if input_ended {
                        return Err(FrameError::MalformedControl);
                    }
                    if let Some(stdin) = &mut process.stdin {
                        stdin.write_all(&bytes).await.map_err(FrameError::Io)?;
                    }
                }
                Ok(ClientFrame::StdinEnd) if !input_ended => {
                    input_ended = true;
                    process.stdin.take();
                }
                Ok(ClientFrame::Signal(request)) => {
                    process::signal_group(process.process_group, request.signal)
                        .map_err(process_error)?;
                }
                Ok(_) => return Err(FrameError::MalformedControl),
                Err(FrameError::Eof | FrameError::Truncated) => {
                    process.stdin.take();
                    end_session(&mut process.child, process.process_group).await;
                    return Err(FrameError::Eof);
                }
                Err(error) => return Err(error),
            },
            read = process.stdout.read(&mut stdout), if stdout_open => {
                let count = read.map_err(FrameError::Io)?;
                stdout_open = count != 0;
                if count != 0 {
                    write_agent_frame(stream, &AgentFrame::Stdout(stdout[..count].to_vec())).await?;
                }
            },
            read = process.stderr.read(&mut stderr), if stderr_open => {
                let count = read.map_err(FrameError::Io)?;
                stderr_open = count != 0;
                if count != 0 {
                    write_agent_frame(stream, &AgentFrame::Stderr(stderr[..count].to_vec())).await?;
                }
            },
            status = process.child.wait() => {
                let status = status.map_err(FrameError::Io)?;
                // `select!` cancels the stream branches, so what the pipes still hold is
                // unread here and must precede the single `Exit`.
                Box::pin(drain_pipes(stream, &mut process, stdout_open, stderr_open)).await?;
                let status = process::normalized_status(status);
                return write_agent_frame(stream, &AgentFrame::Exit(ExitStatus { status })).await;
            }
        }
    }
}

async fn run_pty<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    mut process: PtyProcess,
    initial_size: Option<vivarium::protocol::TerminalSize>,
) -> Result<(), FrameError> {
    if let (Some(writer), Some(size)) = (&process.writer, initial_size) {
        process::resize(writer, size).map_err(process_error)?;
    }
    let mut output = [0; 16 * 1024];
    let mut input_ended = false;
    let mut output_open = true;
    loop {
        tokio::select! {
            frame = read_client_frame(stream) => match frame {
                Ok(ClientFrame::Stdin(bytes)) => {
                    if input_ended {
                        return Err(FrameError::MalformedControl);
                    }
                    if let Some(writer) = &mut process.writer {
                        writer.write_all(&bytes).await.map_err(FrameError::Io)?;
                    }
                }
                Ok(ClientFrame::StdinEnd) if !input_ended => {
                    input_ended = true;
                    process.writer.take();
                },
                Ok(ClientFrame::Resize(size)) => {
                    if let Some(writer) = &process.writer {
                        process::resize(writer, size).map_err(process_error)?;
                    }
                }
                Ok(_) => return Err(FrameError::MalformedControl),
                Err(FrameError::Eof | FrameError::Truncated) => {
                    process.writer.take();
                    end_session(&mut process.child, process.process_group).await;
                    return Err(FrameError::Eof);
                }
                Err(error) => return Err(error),
            },
            read = process.reader.read(&mut output), if output_open => match read {
                // A closed terminal reports EOF or EIO forever; disarming the branch keeps
                // the remaining wait from spinning on an always-ready read.
                Ok(0) => output_open = false,
                Ok(count) => {
                    write_agent_frame(stream, &AgentFrame::Stdout(output[..count].to_vec())).await?;
                }
                Err(error) if error.raw_os_error() == Some(5) => output_open = false,
                Err(error) => return Err(FrameError::Io(error)),
            },
            status = process.child.wait() => {
                let status = status.map_err(FrameError::Io)?;
                // `select!` cancels the read branch, so output already buffered in the
                // terminal is still unread here and must precede the single `Exit`.
                if output_open {
                    Box::pin(drain_terminal(stream, &mut process.reader)).await?;
                }
                let status = process::normalized_status(status);
                return write_agent_frame(stream, &AgentFrame::Exit(ExitStatus { status })).await;
            }
        }
    }
}

/// End the guest process group after the client disconnected.
///
/// A guest process may ignore `SIGTERM`, so the wait is bounded and escalates; without
/// the escalation an abandoned session would hold its child and its task forever.
async fn end_session(child: &mut tokio::process::Child, group: rustix::process::Pid) {
    process::terminate_group(group);
    if tokio::time::timeout(DISCONNECT_GRACE, child.wait())
        .await
        .is_err()
    {
        process::kill_group(group);
        let _ = child.wait().await;
    }
}

/// Drain what the guest process left in its pipes before the single `Exit`.
///
/// A descendant can inherit either pipe and hold it open past the session leader's exit,
/// so the drain is bounded in time, and it emits fixed-size frames because `spec/12`
/// rejects a standard-stream payload above 64 KiB.
async fn drain_pipes<S: AsyncWrite + Unpin>(
    stream: &mut S,
    process: &mut NonPtyProcess,
    mut stdout_open: bool,
    mut stderr_open: bool,
) -> Result<(), FrameError> {
    let mut stdout = [0; 16 * 1024];
    let mut stderr = [0; 16 * 1024];
    let deadline = tokio::time::Instant::now() + EXIT_DRAIN_LIMIT;
    while stdout_open || stderr_open {
        let step = tokio::time::timeout_at(deadline, async {
            tokio::select! {
                read = process.stdout.read(&mut stdout), if stdout_open => (true, read),
                read = process.stderr.read(&mut stderr), if stderr_open => (false, read),
            }
        })
        .await;
        let Ok((is_stdout, read)) = step else {
            return Ok(());
        };
        let count = read.map_err(FrameError::Io)?;
        if count == 0 {
            if is_stdout {
                stdout_open = false;
            } else {
                stderr_open = false;
            }
            continue;
        }
        let frame = if is_stdout {
            AgentFrame::Stdout(stdout[..count].to_vec())
        } else {
            AgentFrame::Stderr(stderr[..count].to_vec())
        };
        write_agent_frame(stream, &frame).await?;
    }
    Ok(())
}

async fn drain_terminal<S: AsyncWrite + Unpin, R: AsyncRead + Unpin>(
    stream: &mut S,
    reader: &mut R,
) -> Result<(), FrameError> {
    let mut output = [0; 16 * 1024];
    let deadline = tokio::time::Instant::now() + EXIT_DRAIN_LIMIT;
    loop {
        match tokio::time::timeout_at(deadline, reader.read(&mut output)).await {
            Err(_) | Ok(Ok(0)) => return Ok(()),
            Ok(Err(error)) if error.raw_os_error() == Some(5) => return Ok(()),
            Ok(Err(error)) => return Err(FrameError::Io(error)),
            Ok(Ok(count)) => {
                write_agent_frame(stream, &AgentFrame::Stdout(output[..count].to_vec())).await?;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use vivarium::protocol::{SessionMode, UnixBytes, read_agent_frame};

    /// The post-exit drain is what makes the streams-before-`Exit` order independent of
    /// which `select!` branch wins, so it is exercised directly: the racing selection it
    /// compensates for cannot be forced from a test.
    #[tokio::test]
    async fn exit_drain_emits_buffered_output_then_stops() {
        let (mut client, server) = duplex(1024 * 1024);
        let mut buffered = b"buffered-terminal-tail".as_slice();
        let mut server = server;
        Box::pin(drain_terminal(&mut server, &mut buffered))
            .await
            .unwrap();
        drop(server);
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Stdout(b"buffered-terminal-tail".to_vec())
        );
        assert!(read_agent_frame(&mut client).await.is_err());
    }

    /// A terminal another process still holds open must not stall the session.
    #[tokio::test]
    async fn exit_drain_is_bounded_when_the_terminal_stays_open() {
        let (_held, reader) = duplex(64);
        let (_client, mut server) = duplex(1024);
        let mut reader = reader;
        let started = std::time::Instant::now();
        Box::pin(drain_terminal(&mut server, &mut reader))
            .await
            .unwrap();
        let elapsed = started.elapsed();
        assert!(
            elapsed >= EXIT_DRAIN_LIMIT,
            "drain returned early: {elapsed:?}"
        );
        assert!(elapsed < EXIT_DRAIN_LIMIT * 8, "drain overran: {elapsed:?}");
    }

    /// Run one non-PTY session to completion and collect its streams and exit status.
    async fn run_non_pty_session(script: &[u8]) -> (Vec<u8>, u8) {
        let (mut client, server) = duplex(4 * 1024 * 1024);
        let request = StartRequest {
            mode: SessionMode::Exec,
            argv: vec![
                UnixBytes::new(b"/bin/sh".to_vec()),
                UnixBytes::new(b"-c".to_vec()),
                UnixBytes::new(script.to_vec()),
            ],
            // The session clears the environment, so a descendant binary is only
            // reachable when the request names the search path itself.
            environment: vec![vivarium::protocol::EnvironmentVariable {
                name: UnixBytes::new(b"PATH".to_vec()),
                value: UnixBytes::new(std::env::var("PATH").unwrap().into_bytes()),
            }],
            cwd: UnixBytes::new(b"/".to_vec()),
            pty: false,
        };
        let task = tokio::spawn(async move {
            let mut server = server;
            run(&mut server, request, None, &[]).await
        });
        let mut output = Vec::new();
        let status = loop {
            match read_agent_frame(&mut client).await.unwrap() {
                AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes) => output.extend(bytes),
                AgentFrame::Exit(exit) => break exit.status,
                frame => panic!("unexpected frame: {frame:?}"),
            }
        };
        assert!(task.await.unwrap().is_ok());
        (output, status)
    }

    /// A descendant that inherits the pipes must not hold the session open past the
    /// session leader's exit.
    #[tokio::test]
    async fn non_pty_exit_is_bounded_when_a_descendant_holds_the_pipes() {
        let started = std::time::Instant::now();
        let (output, status) = run_non_pty_session(b"sleep 30 & printf early; exit 5").await;
        let elapsed = started.elapsed();
        assert_eq!(status, 5);
        assert_eq!(output, b"early".to_vec());
        assert!(
            elapsed < EXIT_DRAIN_LIMIT * 20,
            "session stalled: {elapsed:?}"
        );
    }

    /// Inherited output beyond the 64 KiB stream bound must still reach the client, which
    /// requires the drain to emit fixed-size frames rather than one payload.
    #[tokio::test]
    async fn non_pty_exit_drain_chunks_output_past_the_stream_bound() {
        let (output, status) =
            run_non_pty_session(b"head -c 98304 /dev/zero | tr '\\0' x & exit 5").await;
        assert_eq!(status, 5);
        assert!(
            output.len() > vivarium::protocol::STREAM_PAYLOAD_MAX,
            "inherited output was truncated at {} bytes",
            output.len()
        );
    }

    /// A client that disconnects must end the session even when the guest declines the
    /// first signal, so the wait after `SIGTERM` escalates instead of running forever.
    #[tokio::test]
    async fn disconnect_ends_a_term_resistant_session() {
        let (client, server) = duplex(4096);
        let request = StartRequest {
            mode: SessionMode::Exec,
            argv: vec![
                UnixBytes::new(b"/bin/sh".to_vec()),
                UnixBytes::new(b"-c".to_vec()),
                UnixBytes::new(b"trap '' TERM; while :; do sleep 1; done".to_vec()),
            ],
            environment: vec![vivarium::protocol::EnvironmentVariable {
                name: UnixBytes::new(b"PATH".to_vec()),
                value: UnixBytes::new(std::env::var("PATH").unwrap().into_bytes()),
            }],
            cwd: UnixBytes::new(b"/".to_vec()),
            pty: false,
        };
        let task = tokio::spawn(async move {
            let mut server = server;
            run(&mut server, request, None, &[]).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let started = std::time::Instant::now();
        drop(client);
        assert!(task.await.unwrap().is_err());
        let elapsed = started.elapsed();
        assert!(
            elapsed >= DISCONNECT_GRACE,
            "session ended before the grace period: {elapsed:?}"
        );
        assert!(
            elapsed < DISCONNECT_GRACE * 5,
            "session did not end: {elapsed:?}"
        );
    }

    /// A terminal session reports its streams and exactly one `Exit` carrying the guest
    /// status; the repeat count guards against an intermittent loss of trailing output.
    #[tokio::test]
    async fn pty_output_precedes_exit_repeatedly() {
        for _ in 0..20 {
            let (mut client, server) = duplex(1024 * 1024);
            let request = StartRequest {
                mode: SessionMode::Exec,
                argv: vec![
                    UnixBytes::new(b"/bin/sh".to_vec()),
                    UnixBytes::new(b"-c".to_vec()),
                    UnixBytes::new(
                        b"i=1; while [ $i -le 2000 ]; do echo $i; i=$((i+1)); done; exit 7"
                            .to_vec(),
                    ),
                ],
                environment: Vec::new(),
                cwd: UnixBytes::new(b"/".to_vec()),
                pty: true,
            };
            let task = tokio::spawn(async move {
                let mut server = server;
                run(&mut server, request, None, &[]).await
            });
            let mut output = Vec::new();
            let status = loop {
                match read_agent_frame(&mut client).await.unwrap() {
                    AgentFrame::Stdout(bytes) => output.extend(bytes),
                    AgentFrame::Exit(exit) => break exit.status,
                    frame => panic!("unexpected frame: {frame:?}"),
                }
            };
            assert_eq!(status, 7);
            let text = String::from_utf8_lossy(&output);
            assert!(
                text.contains("\r\n2000\r\n"),
                "terminal output was truncated"
            );
            assert!(task.await.unwrap().is_ok());
        }
    }
}
