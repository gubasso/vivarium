//! Error values intentionally carry only classified, non-secret context.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("invalid launch specification: {0}")]
    InvalidSpec(&'static str),
    /// A record another vivarium version wrote: skew, not corruption (spec/10, spec/14).
    ///
    /// Its own variant rather than an `InvalidSpec` string because the two exit differently — a
    /// record this binary cannot accept is `78`, a record nothing could read is corruption — and
    /// the diagnostic must name both numbers and the remedy.
    #[error(
        "the launch record carries schema version {record} and this vivarium speaks {current}; \
        `viv start --rebuild` relaunches with this version, or run the vivarium generation that \
        wrote the record"
    )]
    SchemaSkew { record: u32, current: u32 },
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
