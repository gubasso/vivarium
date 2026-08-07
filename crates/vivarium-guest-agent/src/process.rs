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

pub fn signal_group(group: Pid, raw: u8) -> Result<(), ProcessError> {
    let signal = match raw {
        1 => Signal::HUP,
        2 => Signal::INT,
        3 => Signal::QUIT,
        9 => Signal::KILL,
        10 => Signal::USR1,
        12 => Signal::USR2,
        15 => Signal::TERM,
        _ => return Err(ProcessError::Signal),
    };
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
}
