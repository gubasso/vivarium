use std::path::PathBuf;

use thiserror::Error;

use super::ArtifactKind;
use crate::exit::ExitKind;

/// A failure while resolving an item-1 configuration path or name.
#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("{variable} is unset, empty, or relative and cannot supply the required default")]
    MissingHome { variable: &'static str },
    #[error("XDG_RUNTIME_DIR is unset or empty")]
    MissingRuntimeDirectory,
    #[error("XDG_RUNTIME_DIR is not absolute: {path}", path = path.display())]
    RelativeRuntimeDirectory { path: PathBuf },
    #[error("could not inspect runtime directory {path}", path = path.display())]
    InspectRuntimeDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("runtime path is not a directory: {path}", path = path.display())]
    RuntimePathNotDirectory { path: PathBuf },
    #[error(
        "runtime directory {path} is owned by uid {actual_uid}, expected uid {expected_uid}",
        path = path.display()
    )]
    RuntimeDirectoryWrongOwner {
        path: PathBuf,
        expected_uid: u32,
        actual_uid: u32,
    },
    #[error(
        "runtime directory {path} is accessible by group or other users (mode {mode:#o})",
        path = path.display()
    )]
    RuntimeDirectoryAccessibleByOthers { path: PathBuf, mode: u32 },
    #[error("invalid {name:?} artifact name; expected kebab-case ASCII")]
    InvalidArtifactName { name: String },
    #[error("could not inspect artifact candidate {path}", path = path.display())]
    InspectArtifact {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "{kind} {name:?} was not found; tried {flat} and {directory}",
        flat = flat.display(),
        directory = directory.display()
    )]
    ArtifactNotFound {
        kind: ArtifactKind,
        name: String,
        flat: PathBuf,
        directory: PathBuf,
    },
    #[error(
        "{kind} {name:?} is ambiguous; both {flat} and {directory} exist",
        flat = flat.display(),
        directory = directory.display()
    )]
    ArtifactAmbiguous {
        kind: ArtifactKind,
        name: String,
        flat: PathBuf,
        directory: PathBuf,
    },
}

impl ResolutionError {
    /// Classifies this failure at the one boundary fixed by ADR-0033 and spec/14.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        match self {
            Self::MissingHome { .. }
            | Self::InvalidArtifactName { .. }
            | Self::ArtifactNotFound { .. }
            | Self::ArtifactAmbiguous { .. } => ExitKind::Config,
            Self::MissingRuntimeDirectory
            | Self::RelativeRuntimeDirectory { .. }
            | Self::InspectRuntimeDirectory { .. }
            | Self::RuntimePathNotDirectory { .. }
            | Self::RuntimeDirectoryWrongOwner { .. }
            | Self::RuntimeDirectoryAccessibleByOthers { .. } => ExitKind::NoPerm,
            Self::InspectArtifact { .. } => ExitKind::IoErr,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use super::ResolutionError;
    use crate::config::ArtifactKind;
    use crate::exit::ExitKind;

    /// Pins spec/14's `78` name/config faults, `77` runtime faults, and `74` probe fault.
    #[test]
    fn every_resolution_error_has_its_specified_exit_kind() {
        let path = PathBuf::from("/example");
        let config_errors = [
            ResolutionError::MissingHome {
                variable: "XDG_CONFIG_HOME",
            },
            ResolutionError::InvalidArtifactName {
                name: "Bad".to_owned(),
            },
            ResolutionError::ArtifactNotFound {
                kind: ArtifactKind::Image,
                name: "missing".to_owned(),
                flat: path.clone(),
                directory: path.clone(),
            },
            ResolutionError::ArtifactAmbiguous {
                kind: ArtifactKind::Piece,
                name: "both".to_owned(),
                flat: path.clone(),
                directory: path.clone(),
            },
        ];
        for error in config_errors {
            assert_eq!(error.exit_code(), ExitKind::Config);
        }

        let runtime_probe = ResolutionError::InspectRuntimeDirectory {
            path: path.clone(),
            source: io::Error::other("probe failed"),
        };
        assert!(runtime_probe.source().is_some());
        let runtime_errors = [
            ResolutionError::MissingRuntimeDirectory,
            ResolutionError::RelativeRuntimeDirectory { path: path.clone() },
            runtime_probe,
            ResolutionError::RuntimePathNotDirectory { path: path.clone() },
            ResolutionError::RuntimeDirectoryWrongOwner {
                path: path.clone(),
                expected_uid: 1000,
                actual_uid: 1001,
            },
            ResolutionError::RuntimeDirectoryAccessibleByOthers {
                path: path.clone(),
                mode: 0o755,
            },
        ];
        for error in runtime_errors {
            assert_eq!(error.exit_code(), ExitKind::NoPerm);
        }

        let probe_error = ResolutionError::InspectArtifact {
            path,
            source: io::Error::other("probe failed"),
        };
        assert!(probe_error.source().is_some());
        assert_eq!(probe_error.exit_code(), ExitKind::IoErr);
    }
}
