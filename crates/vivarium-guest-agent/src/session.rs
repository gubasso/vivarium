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
                // `spec/12` lists `Signal` as a client tag without qualifying it by session
                // kind, and a terminal session is still a process group. The line discipline
                // is what `-t` uses for interrupts, but it is not the only way in.
                Ok(ClientFrame::Signal(request)) => {
                    process::signal_group(process.process_group, request.signal)
                        .map_err(process_error)?;
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
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, duplex};
    use vivarium::protocol::{
        EnvironmentVariable, SessionMode, SignalRequest, TerminalSize, UnixBytes, read_agent_frame,
        write_client_frame,
    };

    /// Build the request the interactive tests share.
    ///
    /// The session clears the environment, so a descendant binary is only reachable when
    /// the request names the search path itself.
    fn script_request(script: &[u8], pty: bool) -> StartRequest {
        StartRequest {
            mode: SessionMode::Exec,
            argv: vec![
                UnixBytes::new(b"/bin/sh".to_vec()),
                UnixBytes::new(b"-c".to_vec()),
                UnixBytes::new(script.to_vec()),
            ],
            environment: vec![EnvironmentVariable {
                name: UnixBytes::new(b"PATH".to_vec()),
                value: UnixBytes::new(std::env::var("PATH").unwrap().into_bytes()),
            }],
            cwd: UnixBytes::new(b"/".to_vec()),
            pty,
        }
    }

    /// Start one session and hand back the live client end.
    ///
    /// The tests below send frames while the guest process runs, so unlike
    /// `run_non_pty_session` this deliberately does not read to completion.
    fn start_session(
        script: &[u8],
        pty: bool,
    ) -> (
        DuplexStream,
        tokio::task::JoinHandle<Result<(), FrameError>>,
    ) {
        let (client, server) = duplex(4 * 1024 * 1024);
        let request = script_request(script, pty);
        let task = tokio::spawn(async move {
            let mut server = server;
            run(&mut server, request, None, &[]).await
        });
        (client, task)
    }

    /// Read stream frames until the accumulated output contains `needle`.
    ///
    /// The timeout is what turns a session that never produces the marker into a failure
    /// rather than a hung test.
    async fn read_until(client: &mut DuplexStream, needle: &str) -> Vec<u8> {
        let mut output = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(10), read_agent_frame(client)).await {
                Ok(Ok(AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes))) => {
                    output.extend(bytes);
                    if String::from_utf8_lossy(&output).contains(needle) {
                        return output;
                    }
                }
                other => panic!(
                    "waiting for {needle:?}, got {other:?} after {:?}",
                    String::from_utf8_lossy(&output)
                ),
            }
        }
    }

    /// Read to the single `Exit`, collecting every stream frame that precedes it.
    async fn read_to_exit(client: &mut DuplexStream) -> (Vec<u8>, u8) {
        let mut output = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(10), read_agent_frame(client))
                .await
                .expect("session produced no exit")
            {
                Ok(AgentFrame::Stdout(bytes) | AgentFrame::Stderr(bytes)) => output.extend(bytes),
                Ok(AgentFrame::Exit(exit)) => return (output, exit.status),
                other => panic!("unexpected frame: {other:?}"),
            }
        }
    }

    /// A resize after `Start` reaches the terminal, not only one that precedes it.
    ///
    /// The pre-`Start` path sets the initial size through `spawn_pty`; this is the other
    /// half of `spec/12`'s resize contract and runs through the session loop instead.
    #[tokio::test]
    async fn pty_resize_after_start_reaches_the_terminal() {
        let (mut client, task) =
            start_session(b"echo ready; while read -r _; do stty size; done", true);
        read_until(&mut client, "ready").await;
        write_client_frame(
            &mut client,
            &ClientFrame::Resize(TerminalSize {
                rows: 11,
                columns: 53,
            }),
        )
        .await
        .unwrap();
        write_client_frame(&mut client, &ClientFrame::Stdin(b"\n".to_vec()))
            .await
            .unwrap();
        read_until(&mut client, "11 53").await;
        // The terminal's own end-of-file character is what ends `read`; see
        // `pty_end_of_input_is_the_terminal_eof_character`.
        write_client_frame(&mut client, &ClientFrame::Stdin(vec![0x04]))
            .await
            .unwrap();
        let (_, status) = read_to_exit(&mut client).await;
        assert_eq!(status, 0);
        assert!(task.await.unwrap().is_ok());
    }

    /// The interrupt byte raises the signal through the guest terminal's line discipline.
    ///
    /// `spec/12` makes this the mechanism `-t` relies on rather than a synthesized signal,
    /// so the byte path is asserted directly.
    #[tokio::test]
    async fn pty_interrupt_byte_raises_the_signal() {
        let (mut client, task) = start_session(
            b"trap 'echo caught; exit 130' INT; echo ready; while :; do sleep 0.1; done",
            true,
        );
        read_until(&mut client, "ready").await;
        write_client_frame(&mut client, &ClientFrame::Stdin(vec![0x03]))
            .await
            .unwrap();
        let (output, status) = read_to_exit(&mut client).await;
        assert!(
            String::from_utf8_lossy(&output).contains("caught"),
            "trap did not run: {:?}",
            String::from_utf8_lossy(&output)
        );
        assert_eq!(status, 130);
        assert!(task.await.unwrap().is_ok());
    }

    /// A signal frame is valid in a terminal session, which `spec/12`'s tag table does not
    /// qualify by session kind.
    #[tokio::test]
    async fn pty_signal_frame_reaches_the_process_group() {
        let (mut client, task) = start_session(b"echo ready; while :; do sleep 0.1; done", true);
        read_until(&mut client, "ready").await;
        write_client_frame(
            &mut client,
            &ClientFrame::Signal(SignalRequest { signal: 15 }),
        )
        .await
        .unwrap();
        let (_, status) = read_to_exit(&mut client).await;
        assert_eq!(status, 143);
        assert!(task.await.unwrap().is_ok());
    }

    /// Without a PTY, `StdinEnd` closes standard input and the guest reads end-of-file.
    #[tokio::test]
    async fn non_pty_stdin_end_delivers_end_of_file() {
        let (mut client, task) = start_session(b"cat", false);
        write_client_frame(&mut client, &ClientFrame::Stdin(b"abc".to_vec()))
            .await
            .unwrap();
        write_client_frame(&mut client, &ClientFrame::StdinEnd)
            .await
            .unwrap();
        let (output, status) = read_to_exit(&mut client).await;
        assert_eq!(output, b"abc".to_vec());
        assert_eq!(status, 0);
        assert!(task.await.unwrap().is_ok());
    }

    /// Under a PTY the terminal's own end-of-file character is what ends the guest's read.
    #[tokio::test]
    async fn pty_end_of_input_is_the_terminal_eof_character() {
        let (mut client, task) = start_session(b"cat", true);
        write_client_frame(&mut client, &ClientFrame::Stdin(b"abc\n".to_vec()))
            .await
            .unwrap();
        read_until(&mut client, "abc").await;
        write_client_frame(&mut client, &ClientFrame::Stdin(vec![0x04]))
            .await
            .unwrap();
        let (_, status) = read_to_exit(&mut client).await;
        assert_eq!(status, 0);
        assert!(task.await.unwrap().is_ok());
    }

    /// `StdinEnd` under a PTY stops the host writing and nothing more: the master stays
    /// open, so the guest observes no end-of-file. The agent does not synthesize one,
    /// which is why the preceding test has to send the byte itself.
    #[tokio::test]
    async fn pty_stdin_end_is_not_an_end_of_file_for_the_guest() {
        let (mut client, task) = start_session(b"cat", true);
        write_client_frame(&mut client, &ClientFrame::Stdin(b"abc\n".to_vec()))
            .await
            .unwrap();
        read_until(&mut client, "abc").await;
        write_client_frame(&mut client, &ClientFrame::StdinEnd)
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(500), task)
                .await
                .is_err(),
            "the guest process ended, so `StdinEnd` delivered an end-of-file"
        );
    }

    /// Standard input after an explicit end-of-input is a protocol fault, not a second
    /// stream. The guest process outlives the frame so the session loop, not the child's
    /// exit, is what ends the session.
    #[tokio::test]
    async fn stdin_after_end_of_input_ends_the_session() {
        let (mut client, task) = start_session(b"sleep 5", false);
        write_client_frame(&mut client, &ClientFrame::StdinEnd)
            .await
            .unwrap();
        write_client_frame(&mut client, &ClientFrame::Stdin(b"late".to_vec()))
            .await
            .unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(FrameError::MalformedControl)
        ));
    }

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
        let (mut client, task) = start_session(script, false);
        let (output, status) = read_to_exit(&mut client).await;
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
        let (client, task) = start_session(b"trap '' TERM; while :; do sleep 1; done", false);
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
            let (mut client, task) = start_session(
                b"i=1; while [ $i -le 2000 ]; do echo $i; i=$((i+1)); done; exit 7",
                true,
            );
            let (output, status) = read_to_exit(&mut client).await;
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
