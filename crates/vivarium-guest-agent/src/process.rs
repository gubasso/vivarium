use pty_process::{OwnedReadPty, OwnedWritePty};
use rustix::process::{Pid, Signal, kill_process_group};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use vivarium::protocol::{CredentialId, SessionMode, StartRequest, TerminalSize};

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("invalid process request")]
    InvalidRequest,
    #[error("process operation failed")]
    Io(#[source] std::io::Error),
    #[error("PTY operation failed")]
    Pty(#[source] pty_process::Error),
    #[error("unsupported signal")]
    Signal,
}

pub struct NonPtyProcess {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
    pub process_group: Pid,
}

pub struct PtyProcess {
    pub child: Child,
    pub reader: OwnedReadPty,
    pub writer: Option<OwnedWritePty>,
    pub process_group: Pid,
}

fn bytes(value: &[u8]) -> OsString {
    OsString::from_vec(value.to_vec())
}

fn resolve_program(
    request: &StartRequest,
) -> Result<(OsString, Vec<OsString>, Option<OsString>), ProcessError> {
    match request.mode {
        SessionMode::Exec => {
            let (program, arguments) = request
                .argv
                .split_first()
                .ok_or(ProcessError::InvalidRequest)?;
            if program.as_bytes().is_empty() {
                return Err(ProcessError::InvalidRequest);
            }
            Ok((
                bytes(program.as_bytes()),
                arguments.iter().map(|arg| bytes(arg.as_bytes())).collect(),
                None,
            ))
        }
        SessionMode::Shell => {
            let passwd = std::fs::read("/etc/passwd").map_err(ProcessError::Io)?;
            let uid = rustix::process::getuid().as_raw();
            let prefix = format!("vivarium:x:{uid}:");
            let line = passwd
                .split(|byte| *byte == b'\n')
                .find(|line| line.starts_with(prefix.as_bytes()))
                .ok_or(ProcessError::InvalidRequest)?;
            let shell = line
                .split(|byte| *byte == b':')
                .nth(6)
                .filter(|value| !value.is_empty())
                .ok_or(ProcessError::InvalidRequest)?;
            let name = PathBuf::from(OsString::from_vec(shell.to_vec()))
                .file_name()
                .ok_or(ProcessError::InvalidRequest)?
                .as_bytes()
                .to_vec();
            let mut arg0 = Vec::with_capacity(name.len() + 1);
            arg0.push(b'-');
            arg0.extend(name);
            Ok((bytes(shell), Vec::new(), Some(bytes(&arg0))))
        }
    }
}

fn apply_environment(command: &mut Command, request: &StartRequest, credentials: &[CredentialId]) {
    command.env_clear();
    for variable in &request.environment {
        command.env(
            bytes(variable.name.as_bytes()),
            bytes(variable.value.as_bytes()),
        );
    }
    if credentials.contains(&CredentialId::Ssh) {
        command.env("SSH_AUTH_SOCK", CredentialId::Ssh.guest_socket());
    }
    command.current_dir(bytes(request.cwd.as_bytes()));
}

pub fn spawn_non_pty(
    request: &StartRequest,
    credentials: &[CredentialId],
) -> Result<NonPtyProcess, ProcessError> {
    let (program, arguments, arg0) = resolve_program(request)?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    if let Some(value) = arg0 {
        command.arg0(value);
    }
    apply_environment(&mut command, request, credentials);
    let mut child = command.spawn().map_err(ProcessError::Io)?;
    let raw_pid = i32::try_from(child.id().ok_or(ProcessError::InvalidRequest)?)
        .map_err(|_| ProcessError::InvalidRequest)?;
    let process_group = Pid::from_raw(raw_pid).ok_or(ProcessError::InvalidRequest)?;
    Ok(NonPtyProcess {
        stdin: child.stdin.take(),
        stdout: child.stdout.take().ok_or(ProcessError::InvalidRequest)?,
        stderr: child.stderr.take().ok_or(ProcessError::InvalidRequest)?,
        child,
        process_group,
    })
}

pub fn spawn_pty(
    request: &StartRequest,
    credentials: &[CredentialId],
    initial: Option<TerminalSize>,
) -> Result<PtyProcess, ProcessError> {
    let (program, arguments, arg0) = resolve_program(request)?;
    let (pty, pts) = pty_process::open().map_err(ProcessError::Pty)?;
    if let Some(size) = initial {
        pty.resize(pty_process::Size::new(size.rows, size.columns))
            .map_err(ProcessError::Pty)?;
    }
    let mut command = pty_process::Command::new(program)
        .args(arguments)
        .env_clear()
        .current_dir(bytes(request.cwd.as_bytes()))
        .kill_on_drop(true);
    for variable in &request.environment {
        command = command.env(
            bytes(variable.name.as_bytes()),
            bytes(variable.value.as_bytes()),
        );
    }
    if credentials.contains(&CredentialId::Ssh) {
        command = command.env("SSH_AUTH_SOCK", CredentialId::Ssh.guest_socket());
    }
    if let Some(value) = arg0 {
        command = command.arg0(value);
    }
    let child = command.spawn(pts).map_err(ProcessError::Pty)?;
    let raw_pid = i32::try_from(child.id().ok_or(ProcessError::InvalidRequest)?)
        .map_err(|_| ProcessError::InvalidRequest)?;
    let process_group = Pid::from_raw(raw_pid).ok_or(ProcessError::InvalidRequest)?;
    let (reader, writer) = pty.into_split();
    Ok(PtyProcess {
        child,
        reader,
        writer: Some(writer),
        process_group,
    })
}

pub fn resize(writer: &OwnedWritePty, size: TerminalSize) -> Result<(), ProcessError> {
    writer
        .resize(pty_process::Size::new(size.rows, size.columns))
        .map_err(ProcessError::Pty)
}

/// Deliver an explicit signal frame to the session's process group.
///
/// `spec/12` carries the signal as a bare number and names the accepted set as whatever
/// the guest kernel defines, so that definition is the contract rather than a hand-picked
/// subset: `from_named_raw` admits `1..=31` on Linux and rejects both `0` and the
/// libc-reserved real-time range, which a session may not send. A group stopped by
/// `SIGSTOP` still tears down, because the disconnect escalation ends in `SIGKILL`.
pub fn signal_group(group: Pid, raw: u8) -> Result<(), ProcessError> {
    let signal = Signal::from_named_raw(i32::from(raw)).ok_or(ProcessError::Signal)?;
    kill_process_group(group, signal).map_err(|error| ProcessError::Io(error.into()))
}

pub fn terminate_group(group: Pid) {
    let _ = kill_process_group(group, Signal::TERM);
}

/// End a process group that did not answer `SIGTERM`.
///
/// `SIGKILL` cannot be caught or ignored, so this is the escalation that makes session
/// teardown terminate.
pub fn kill_group(group: Pid) {
    let _ = kill_process_group(group, Signal::KILL);
}

#[must_use]
pub fn normalized_status(status: std::process::ExitStatus) -> u8 {
    status.signal().map_or_else(
        || status.code().map_or(255, |code| code.to_le_bytes()[0]),
        |signal| {
            u8::try_from(
                128_u16
                    .saturating_add(u16::try_from(signal).unwrap_or(u16::MAX))
                    .min(255),
            )
            .unwrap_or(255)
        },
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use vivarium::protocol::{SessionMode, UnixBytes};

    fn request(arguments: &[&[u8]]) -> StartRequest {
        StartRequest {
            mode: SessionMode::Exec,
            argv: arguments
                .iter()
                .map(|value| UnixBytes::new(value.to_vec()))
                .collect(),
            environment: Vec::new(),
            cwd: UnixBytes::new(b"/".to_vec()),
            pty: false,
        }
    }

    #[tokio::test]
    async fn preserves_exit_status() {
        for code in [0, 42] {
            let mut process = spawn_non_pty(
                &request(&[b"/bin/sh", b"-c", format!("exit {code}").as_bytes()]),
                &[],
            )
            .unwrap();
            assert_eq!(normalized_status(process.child.wait().await.unwrap()), code);
        }
    }

    #[tokio::test]
    async fn signal_status_is_normalized() {
        let mut process =
            spawn_non_pty(&request(&[b"/bin/sh", b"-c", b"kill -TERM $$"]), &[]).unwrap();
        assert_eq!(normalized_status(process.child.wait().await.unwrap()), 143);
    }

    /// Argument and environment bytes survive the spawn, not merely the codec.
    ///
    /// `frame.rs` proves the wire round-trips arbitrary bytes; this is the other half,
    /// because `execve` is where a `String`-shaped conversion would silently corrupt them.
    #[tokio::test]
    async fn byte_values_survive_the_spawn() {
        use tokio::io::AsyncReadExt as _;
        use vivarium::protocol::EnvironmentVariable;

        let mut spawn_request = request(&[
            b"/bin/sh",
            b"-c",
            b"printf %s \"$V\"; printf %s \"$1\"",
            b"sh",
            b"\xff\xfe",
        ]);
        spawn_request.environment = vec![EnvironmentVariable {
            name: UnixBytes::new(b"V".to_vec()),
            value: UnixBytes::new(vec![0xfe, 0xff]),
        }];
        let mut process = spawn_non_pty(&spawn_request, &[]).unwrap();
        let mut output = Vec::new();
        process.stdout.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, vec![0xfe, 0xff, 0xff, 0xfe]);
        assert_eq!(normalized_status(process.child.wait().await.unwrap()), 0);
    }

    /// The accepted signal set is exactly what the guest kernel names.
    ///
    /// A fresh group per signal keeps the sweep honest: several of these end or stop the
    /// group, so reusing one victim would make every later iteration test nothing.
    #[tokio::test]
    async fn every_named_signal_is_accepted_and_the_rest_are_rejected() {
        fn victim() -> NonPtyProcess {
            spawn_non_pty(&request(&[b"/bin/sh", b"-c", b"sleep 5"]), &[]).unwrap()
        }

        for signal in 1..=31_u8 {
            let mut process = victim();
            assert!(
                signal_group(process.process_group, signal).is_ok(),
                "named signal {signal} was rejected"
            );
            kill_group(process.process_group);
            let _ = process.child.wait().await;
        }
        // `0` is not a signal, and the range above the named set is reserved by the guest's
        // own C library for real-time signals a session may not send.
        for raw in [0_u8, 32, 33, 64, 255] {
            let mut process = victim();
            assert!(
                matches!(
                    signal_group(process.process_group, raw),
                    Err(ProcessError::Signal)
                ),
                "signal number {raw} was accepted"
            );
            kill_group(process.process_group);
            let _ = process.child.wait().await;
        }
    }
}
