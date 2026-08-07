//! Direction-specific wire messages and closed protocol constants.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
pub const VSOCK_CID: u32 = 3;
pub const CONTROL_PORT: u32 = 52_000;
pub const CREDENTIAL_PORT: u32 = 52_001;
pub const CREDENTIAL_POOL_SIZE: usize = 4;
pub const STREAM_PAYLOAD_MAX: usize = 64 * 1024;
pub const CONTROL_PAYLOAD_MAX: usize = 1024 * 1024;
pub const CREDENTIAL_PARKED_ACK: u8 = 0;

pub const TAG_CLIENT_HELLO: u8 = 0x01;
pub const TAG_AGENT_HELLO: u8 = 0x02;
pub const TAG_PING: u8 = 0x03;
pub const TAG_PONG: u8 = 0x04;
pub const TAG_START: u8 = 0x05;
pub const TAG_STDIN: u8 = 0x06;
pub const TAG_STDIN_END: u8 = 0x07;
pub const TAG_RESIZE: u8 = 0x08;
pub const TAG_SIGNAL: u8 = 0x09;
pub const TAG_STDOUT: u8 = 0x0a;
pub const TAG_STDERR: u8 = 0x0b;
pub const TAG_EXIT: u8 = 0x0c;
pub const TAG_ERROR: u8 = 0x0d;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialId {
    Ssh,
    Gpg,
}

impl CredentialId {
    #[must_use]
    pub const fn setup_byte(self) -> u8 {
        match self {
            Self::Ssh => 0x01,
            Self::Gpg => 0x02,
        }
    }

    #[must_use]
    pub const fn guest_socket(self) -> &'static str {
        match self {
            Self::Ssh => "/run/vivarium/ssh-agent.sock",
            Self::Gpg => "/run/vivarium/gpg-agent.sock",
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Gpg => "gpg",
        }
    }

    /// Parse the closed credential setup-byte vocabulary.
    ///
    /// # Errors
    /// Returns an error for every byte outside `ssh` and `gpg`.
    pub const fn from_setup_byte(byte: u8) -> Result<Self, ProtocolValueError> {
        match byte {
            0x01 => Ok(Self::Ssh),
            0x02 => Ok(Self::Gpg),
            _ => Err(ProtocolValueError::CredentialId),
        }
    }
}

impl fmt::Display for CredentialId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for CredentialId {
    type Err = ProtocolValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ssh" => Ok(Self::Ssh),
            "gpg" => Ok(Self::Gpg),
            _ => Err(ProtocolValueError::CredentialId),
        }
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct UnixBytes(Vec<u8>);

impl UnixBytes {
    #[must_use]
    pub const fn new(value: Vec<u8>) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for UnixBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for UnixBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub schema_version: u32,
    pub boot_identity: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Exec,
    Shell,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentVariable {
    pub name: UnixBytes,
    pub value: UnixBytes,
}

impl fmt::Debug for EnvironmentVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub mode: SessionMode,
    pub argv: Vec<UnixBytes>,
    pub environment: Vec<EnvironmentVariable>,
    pub cwd: UnixBytes,
    pub pty: bool,
}

impl fmt::Debug for StartRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StartRequest([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSize {
    pub rows: u16,
    pub columns: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignalRequest {
    pub signal: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExitStatus {
    pub status: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolErrorMessage {
    pub code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientFrame {
    Hello(Hello),
    Ping,
    Start(StartRequest),
    Stdin(Vec<u8>),
    StdinEnd,
    Resize(TerminalSize),
    Signal(SignalRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentFrame {
    Hello(Hello),
    Pong,
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(ExitStatus),
    Error(ProtocolErrorMessage),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ProtocolValueError {
    #[error("invalid credential id")]
    CredentialId,
    #[error("invalid boot identity")]
    BootIdentity,
}

/// Validate a bounded lowercase UUID-shaped boot identity.
///
/// # Errors
/// Returns an error unless the value has the exact non-secret identity shape.
pub fn validate_boot_identity(value: &str) -> Result<(), ProtocolValueError> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || ![8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        || bytes.iter().enumerate().any(|(index, byte)| {
            !([8, 13, 18, 23].contains(&index)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(byte))
        })
    {
        return Err(ProtocolValueError::BootIdentity);
    }
    Ok(())
}

#[must_use]
pub fn path_to_unix_bytes(path: &Path) -> UnixBytes {
    use std::os::unix::ffi::OsStrExt;
    UnixBytes::new(path.as_os_str().as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_values_are_redacted() {
        let secret = b"sentinel-secret".to_vec();
        let request = StartRequest {
            mode: SessionMode::Exec,
            argv: vec![UnixBytes::new(secret.clone())],
            environment: vec![EnvironmentVariable {
                name: UnixBytes::new(b"TOKEN".to_vec()),
                value: UnixBytes::new(secret),
            }],
            cwd: UnixBytes::new(b"/tmp".to_vec()),
            pty: false,
        };
        assert!(!format!("{request:?}").contains("sentinel-secret"));
        assert_eq!(format!("{:?}", request.argv[0]), "[REDACTED]");
    }

    #[test]
    fn boot_identity_is_bounded_lowercase_uuid_shape() {
        assert!(validate_boot_identity("01234567-89ab-cdef-0123-456789abcdef").is_ok());
        assert!(validate_boot_identity("01234567-89AB-cdef-0123-456789abcdef").is_err());
        assert!(validate_boot_identity("short").is_err());
    }
}
