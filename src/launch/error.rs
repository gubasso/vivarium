//! Error values intentionally carry only classified, non-secret context.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("invalid launch specification: {0}")]
    InvalidSpec(&'static str),
    #[error("runtime path is invalid: {0}")]
    InvalidRuntimePath(&'static str),
    #[error("failed to construct launch policy: {0}")]
    Policy(&'static str),
    #[error("I/O failure while handling {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("timed out waiting for {0}")]
    Readiness(&'static str),
    #[error("child {kind} exited unexpectedly with status {status:?}")]
    ChildExit {
        kind: &'static str,
        status: Option<i32>,
    },
    #[error("supervision was cancelled")]
    Cancelled,
    #[error("unsafe cleanup refused because an unknown runtime artifact exists")]
    UnknownRuntimeArtifact(PathBuf),
    #[error("a supervised task failed")]
    Task,
}

impl LaunchError {
    pub(crate) const fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}
