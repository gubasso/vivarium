use std::path::{Path, PathBuf};

use thiserror::Error;

use super::ArtifactKind;
use crate::diagnostic::{Diagnostic, DiagnosticId, Locus, Namespace};
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

/// A defect in an artifact-owned `inputs.toml` declaration.
#[derive(Debug, Error)]
#[error("{}", self.0.message)]
pub struct InputError(Box<InputFault>);

#[derive(Debug)]
struct InputFault {
    message: String,
    condition: &'static str,
    locus: Locus,
    why: String,
    accepted: Vec<String>,
}

impl InputError {
    pub(super) fn new(
        message: impl Into<String>,
        condition: &'static str,
        locus: Locus,
        why: impl Into<String>,
        accepted: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self(Box::new(InputFault {
            message: message.into(),
            condition,
            locus,
            why: why.into(),
            accepted: accepted.into_iter().map(Into::into).collect(),
        }))
    }

    /// Classifies every authored declaration defect as configuration input.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        ExitKind::Config
    }

    /// Renders the defect through the shared diagnostic skeleton.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(Namespace::Manifest, self.0.condition),
            self.0.message.clone(),
            self.0.locus.clone(),
            self.0.why.clone(),
        )
        .with_accepted(self.0.accepted.iter().cloned())
    }

    pub(super) const fn condition(&self) -> &'static str {
        self.0.condition
    }

    pub(super) fn locus(&self) -> Locus {
        self.0.locus.clone()
    }

    pub(super) fn why(&self) -> &str {
        &self.0.why
    }

    pub(super) fn accepted(&self) -> &[String] {
        &self.0.accepted
    }
}

/// A failure while resolving, planning, publishing, or pinning a generated flake.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct GeneratedFlakeError(Box<GeneratedFlakeFault>);

#[derive(Debug, Error)]
#[error("{message}")]
struct GeneratedFlakeFault {
    message: String,
    condition: &'static str,
    namespace: Namespace,
    locus: Locus,
    why: String,
    kind: GeneratedFlakeErrorKind,
    accepted: Vec<String>,
    #[source]
    source: Option<std::io::Error>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GeneratedFlakeErrorKind {
    Config,
    Permission,
    Io,
    Race,
    Internal,
    /// A required external program is absent or unusable (`69`), the same boundary
    /// [`EvaluationError`] draws for the evaluation half.
    Unavailable,
}

impl GeneratedFlakeError {
    pub(super) fn plain(
        kind: GeneratedFlakeErrorKind,
        namespace: Namespace,
        condition: &'static str,
        locus: Locus,
        message: impl Into<String>,
        why: impl Into<String>,
    ) -> Self {
        Self(Box::new(GeneratedFlakeFault {
            message: message.into(),
            condition,
            namespace,
            locus,
            why: why.into(),
            kind,
            accepted: Vec::new(),
            source: None,
        }))
    }

    /// Fills the conditional `accepted here:` slot spec/14 fixes for an unknown key or value.
    pub(super) fn with_accepted(
        mut self,
        accepted: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.0.accepted = accepted.into_iter().map(Into::into).collect();
        self
    }

    pub(super) fn io(
        namespace: Namespace,
        condition: &'static str,
        path: PathBuf,
        message: impl Into<String>,
        source: std::io::Error,
    ) -> Self {
        let kind = if source.kind() == std::io::ErrorKind::PermissionDenied {
            GeneratedFlakeErrorKind::Permission
        } else {
            GeneratedFlakeErrorKind::Io
        };
        Self(Box::new(GeneratedFlakeFault {
            message: message.into(),
            condition,
            namespace,
            locus: Locus::File(path),
            why: source.to_string(),
            kind,
            accepted: Vec::new(),
            source: Some(source),
        }))
    }

    /// Classifies authored defects, permission failures, owned I/O, and first-pin races.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        match self.0.kind {
            GeneratedFlakeErrorKind::Config => ExitKind::Config,
            GeneratedFlakeErrorKind::Permission => ExitKind::NoPerm,
            GeneratedFlakeErrorKind::Io => ExitKind::IoErr,
            GeneratedFlakeErrorKind::Race => ExitKind::TempFail,
            GeneratedFlakeErrorKind::Internal => ExitKind::Software,
            GeneratedFlakeErrorKind::Unavailable => ExitKind::Unavailable,
        }
    }

    /// Renders the failure through the shared diagnostic skeleton.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(self.0.namespace, self.0.condition),
            self.0.message.clone(),
            self.0.locus.clone(),
            self.0.why.clone(),
        )
        .with_accepted(self.0.accepted.iter().cloned())
    }
}

/// A failure while reading or publishing tool-owned state and cache records.
///
/// Separate from [`ManifestError`] because these files are derived or tool-owned rather than
/// authored configuration. Their channel failures retain the I/O and permission classifications
/// callers need, while malformed rebuildable cache content is handled by rebuilding it.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct RegistryError(Box<RegistryFault>);

#[derive(Debug, Error)]
#[error("{message}")]
struct RegistryFault {
    message: String,
    condition: &'static str,
    locus: Locus,
    why: String,
    kind: RegistryErrorKind,
    accepted: Vec<String>,
    #[source]
    source: Option<std::io::Error>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RegistryErrorKind {
    /// The file parsed but says something outside the grammar — a user's own defect.
    Config,
    /// The channel itself failed.
    Io,
    /// A host permission denied a write.
    Permission,
}

impl RegistryError {
    pub(super) fn plain(
        kind: RegistryErrorKind,
        condition: &'static str,
        locus: Locus,
        message: impl Into<String>,
        why: impl Into<String>,
    ) -> Self {
        Self(Box::new(RegistryFault {
            message: message.into(),
            condition,
            locus,
            why: why.into(),
            kind,
            accepted: Vec::new(),
            source: None,
        }))
    }

    /// Records an I/O failure, classifying a denied permission apart from a failing channel.
    ///
    /// `deny_is_permission` is the caller's, because the two directions differ: spec/02's read
    /// table
    /// answers `74` for an unreadable file however it became unreadable, while spec/14's command
    /// matrix gives owned-channel writes a separate `77`. Deciding it here would need this
    /// function to
    /// know which it was serving.
    pub(super) fn io(
        condition: &'static str,
        path: PathBuf,
        message: impl Into<String>,
        source: std::io::Error,
        deny_is_permission: bool,
    ) -> Self {
        let kind = if deny_is_permission && source.kind() == std::io::ErrorKind::PermissionDenied {
            RegistryErrorKind::Permission
        } else {
            RegistryErrorKind::Io
        };
        Self(Box::new(RegistryFault {
            message: message.into(),
            condition,
            locus: Locus::File(path),
            why: source.to_string(),
            kind,
            accepted: Vec::new(),
            source: Some(source),
        }))
    }

    /// Classifies authored defects, owned I/O, denied writes, and lock contention.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        match self.0.kind {
            RegistryErrorKind::Config => ExitKind::Config,
            RegistryErrorKind::Io => ExitKind::IoErr,
            RegistryErrorKind::Permission => ExitKind::NoPerm,
        }
    }

    /// Renders the failure through the shared diagnostic skeleton.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(Namespace::State, self.0.condition),
            self.0.message.clone(),
            self.0.locus.clone(),
            self.0.why.clone(),
        )
        .with_accepted(self.0.accepted.iter().cloned())
    }
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

/// A defect in a manifest that its own text is enough to decide.
///
/// One type for the whole parse stage because ADR-0057 gives the whole stage one code: everything
/// here is `78`, and everything that needs the merged layers is `65` somewhere else. Keeping the
/// two apart in the type system is what stops one defect acquiring two codes, which spec/14's
/// permanent-API rule forbids.
/// Boxed because a parse threads this through every accessor in the grammar walk, and an error
/// payload wider than the values it guards would enlarge each of those `Result`s in the success
/// case too — the cost clippy's `result_large_err` names.
#[derive(Debug, Error)]
#[error("{}", self.0.kind.what(&self.0.manifest))]
pub struct ManifestError(Box<ManifestFault>);

/// The parts of a manifest defect, held behind one allocation.
#[derive(Debug)]
struct ManifestFault {
    kind: ManifestErrorKind,
    manifest: String,
    locus: Locus,
}

impl ManifestError {
    /// Records one defect, resolving the byte offset into the position slot while the source text
    /// is still in hand.
    pub(super) fn new(
        kind: ManifestErrorKind,
        manifest: &str,
        path: &Path,
        source: &str,
        offset: Option<usize>,
    ) -> Self {
        let locus = offset.map_or_else(
            || Locus::File(path.to_path_buf()),
            |offset| Locus::in_source(path, source, offset),
        );
        Self(Box::new(ManifestFault {
            kind,
            manifest: manifest.to_owned(),
            locus,
        }))
    }

    /// Classifies this failure. Every parse defect is `78`, which is the point of the boundary.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        match self.0.kind {
            ManifestErrorKind::Syntax { .. }
            | ManifestErrorKind::UnknownKey { .. }
            | ManifestErrorKind::MissingKey { .. }
            | ManifestErrorKind::WrongType { .. }
            | ManifestErrorKind::InvalidValue { .. }
            | ManifestErrorKind::ExtendsRequiresDirectoryForm => ExitKind::Config,
        }
    }

    /// The condition half of this failure's diagnostic id.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        self.0.kind.condition()
    }

    /// Renders this failure in the skeleton spec/14 fixes.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let fault = &self.0;
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(Namespace::Manifest, fault.kind.condition()),
            fault.kind.what(&fault.manifest),
            fault.locus.clone(),
            fault.kind.why(),
        )
        .with_accepted(fault.kind.accepted().iter().copied());
        match fault.kind.hint() {
            Some(hint) => diagnostic.with_hint(hint),
            None => diagnostic,
        }
    }
}

/// What went wrong, separately from which manifest it went wrong in.
#[derive(Debug)]
pub(super) enum ManifestErrorKind {
    /// The bytes are not TOML at all.
    Syntax {
        /// The parser's own account of the syntax fault.
        detail: String,
    },
    /// A key outside the closed grammar. This is the compatibility surface.
    UnknownKey {
        /// The offending key, verbatim from the source.
        key: String,
        /// What is accepted at that position.
        accepted: &'static [&'static str],
    },
    /// A key the grammar requires is absent.
    MissingKey {
        /// The required key.
        key: &'static str,
    },
    /// A known key holding the wrong TOML type.
    WrongType {
        /// The dotted key path.
        key: String,
        /// What the grammar wanted.
        expected: &'static str,
        /// What the document held.
        found: &'static str,
    },
    /// A known key holding a value outside its domain.
    InvalidValue {
        /// The dotted key path.
        key: String,
        /// What the grammar wanted.
        expected: &'static str,
        /// The closed value set, when the domain is one.
        accepted: &'static [&'static str],
    },
    /// `extends` named by a manifest in the flat form, which has nowhere to put the module.
    ExtendsRequiresDirectoryForm,
}

impl ManifestErrorKind {
    /// The condition half of the id. Stable and never reassigned, per spec/14.
    const fn condition(&self) -> &'static str {
        match self {
            Self::Syntax { .. } => "syntax",
            Self::UnknownKey { .. } => "unknown-key",
            Self::MissingKey { .. } => "missing-key",
            Self::WrongType { .. } => "wrong-type",
            Self::InvalidValue { .. } => "invalid-value",
            Self::ExtendsRequiresDirectoryForm => "extends-form",
        }
    }

    fn what(&self, manifest: &str) -> String {
        match self {
            Self::Syntax { .. } => format!("manifest `{manifest}` is not valid TOML"),
            Self::UnknownKey { key, .. } => {
                format!("unknown key `{key}` in manifest `{manifest}`")
            }
            Self::MissingKey { key } => {
                format!("missing required key `{key}` in manifest `{manifest}`")
            }
            Self::WrongType { key, .. } => {
                format!("wrong type for key `{key}` in manifest `{manifest}`")
            }
            Self::InvalidValue { key, .. } => {
                format!("invalid value for key `{key}` in manifest `{manifest}`")
            }
            Self::ExtendsRequiresDirectoryForm => {
                format!("`extends` is not available in the flat manifest `{manifest}`")
            }
        }
    }

    fn why(&self) -> String {
        match self {
            Self::Syntax { detail } => detail.clone(),
            // The version is the whole point: with no schema version in the file, this is what
            // turns "unknown key" into "your file is newer than your tool".
            Self::UnknownKey { .. } => format!(
                "not part of the manifest grammar viv {} understands",
                env!("CARGO_PKG_VERSION")
            ),
            Self::MissingKey { .. } => "the manifest grammar requires it".to_owned(),
            Self::WrongType {
                expected, found, ..
            } => format!("expected {expected}, found {found}"),
            Self::InvalidValue { expected, .. } => format!("expected {expected}"),
            Self::ExtendsRequiresDirectoryForm => {
                "a relative module has nowhere to sit beside a single-file manifest".to_owned()
            }
        }
    }

    /// The conditional `accepted here:` slot, carried only by an unknown key or an unknown value.
    const fn accepted(&self) -> &'static [&'static str] {
        match self {
            Self::UnknownKey { accepted, .. } | Self::InvalidValue { accepted, .. } => accepted,
            Self::Syntax { .. }
            | Self::MissingKey { .. }
            | Self::WrongType { .. }
            | Self::ExtendsRequiresDirectoryForm => &[],
        }
    }

    /// Omitted where no honest local repair exists, rather than filled with advice that does not
    /// act — spec/14's own rule for this slot.
    fn hint(&self) -> Option<String> {
        match self {
            Self::UnknownKey { .. } => Some(
                concat!(
                    "remove the key, or upgrade vivarium — a manifest written for a newer\n",
                    "vivarium reports its new keys exactly this way",
                )
                .to_owned(),
            ),
            Self::ExtendsRequiresDirectoryForm => Some(
                concat!(
                    "move the manifest into its own directory as `default.toml` and put the\n",
                    "module beside it",
                )
                .to_owned(),
            ),
            Self::Syntax { .. }
            | Self::MissingKey { .. }
            | Self::WrongType { .. }
            | Self::InvalidValue { .. } => None,
        }
    }
}

/// A failure while evaluating a published generated flake.
///
/// Held apart from [`GeneratedFlakeError`] because the two answer for different halves of the same
/// path: that one owns everything up to a tree on disk, this one owns what Nix then says about it.
/// The split keeps the exit mapping honest — a tree vivarium could not write is `74` on a channel
/// it owns, and a tree Nix could not evaluate is `70` on a fault it does not.
#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("nix is not available on this host")]
    NixMissing {
        #[source]
        source: std::io::Error,
    },
    /// `nix` resolved on `PATH` but the host refused to execute it. Held apart from
    /// [`Self::NixUnusable`] because the exit mapping turns on the difference: a prerequisite that
    /// is absent or too old is `69`, while one the host forbids acting on is a filesystem
    /// permission failure, which is the `77` the `config` rows of spec/14 admit for preflight.
    #[error("not permitted to run nix")]
    NixNotPermitted {
        #[source]
        source: std::io::Error,
    },
    #[error("could not run nix")]
    NixUnusable {
        #[source]
        source: std::io::Error,
    },
    #[error("nix {verb} failed")]
    NixFailed { verb: &'static str, stderr: String },
    /// A declared flake input the lock in force has no node for (spec/04). Not a Nix fault: the
    /// build refuses on purpose rather than re-resolving inputs, which is the unannounced input
    /// jump N3 exists to prevent.
    #[error("the lock in force has no node for an input this composition declares")]
    LockMissingNode { stderr: String },
    #[error("could not read what nix produced")]
    Undecodable { detail: String },
}

impl EvaluationError {
    /// Classifies a host prerequisite, a Nix fault, and a refused input jump apart.
    #[must_use]
    pub const fn exit_code(&self) -> ExitKind {
        match self {
            Self::NixMissing { .. } | Self::NixUnusable { .. } => ExitKind::Unavailable,
            Self::NixNotPermitted { .. } => ExitKind::NoPerm,
            Self::NixFailed { .. } | Self::Undecodable { .. } => ExitKind::Software,
            Self::LockMissingNode { .. } => ExitKind::Config,
        }
    }

    /// Renders the failure through the shared diagnostic skeleton.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::NixMissing { source } => Diagnostic::new(
                DiagnosticId::new(Namespace::Host, "nix-missing"),
                self.to_string(),
                Locus::Named("host"),
                source.to_string(),
            )
            .with_hint("install Nix with flakes enabled; every vivarium build is a Nix build"),
            Self::NixNotPermitted { source } => Diagnostic::new(
                DiagnosticId::new(Namespace::Host, "nix-not-permitted"),
                self.to_string(),
                Locus::Named("host"),
                source.to_string(),
            )
            .with_hint("check the execute bit on the `nix` your PATH resolves to"),
            Self::NixUnusable { source } => Diagnostic::new(
                DiagnosticId::new(Namespace::Host, "nix-unusable"),
                self.to_string(),
                Locus::Named("host"),
                source.to_string(),
            ),
            Self::NixFailed { stderr, .. } => Diagnostic::new(
                DiagnosticId::new(Namespace::Store, "evaluation-failed"),
                self.to_string(),
                Locus::Named("generated flake"),
                truncate_stderr(stderr),
            ),
            Self::LockMissingNode { stderr } => Diagnostic::new(
                DiagnosticId::new(Namespace::Lock, "missing-node"),
                self.to_string(),
                Locus::Named("effective lock"),
                truncate_stderr(stderr),
            )
            .with_hint("run `viv update` to move the pin; no ordinary build re-resolves inputs"),
            Self::Undecodable { detail } => Diagnostic::new(
                DiagnosticId::new(Namespace::Internal, "report-undecodable"),
                self.to_string(),
                Locus::Named("generated flake"),
                detail.clone(),
            ),
        }
    }
}

/// Keeps a Nix trace inside the `why:` slot without turning a diagnostic into a transcript.
///
/// The whole trace is what a user needs and stderr is where it belongs, but the skeleton's `why:`
/// is one reason rather than a log. The tail is kept rather than the head: Nix puts the message
/// that names the actual defect last, under the `while evaluating` frames that lead to it.
fn truncate_stderr(stderr: &str) -> String {
    const KEPT_LINES: usize = 12;
    let trimmed = stderr.trim_end();
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() <= KEPT_LINES {
        return trimmed.to_owned();
    }
    lines[lines.len() - KEPT_LINES..].join("\n")
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use super::{
        EvaluationError, GeneratedFlakeError, GeneratedFlakeErrorKind, InputError, ResolutionError,
    };
    use crate::config::ArtifactKind;
    use crate::diagnostic::{Locus, Namespace};
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

    /// Pins the complete item-3 authored, permission, I/O, race, and contract boundary.
    #[test]
    fn every_generated_flake_error_class_has_its_specified_exit_kind() {
        let rows = [
            (GeneratedFlakeErrorKind::Config, ExitKind::Config),
            (GeneratedFlakeErrorKind::Permission, ExitKind::NoPerm),
            (GeneratedFlakeErrorKind::Io, ExitKind::IoErr),
            (GeneratedFlakeErrorKind::Race, ExitKind::TempFail),
            (GeneratedFlakeErrorKind::Internal, ExitKind::Software),
            (GeneratedFlakeErrorKind::Unavailable, ExitKind::Unavailable),
        ];
        for (kind, expected) in rows {
            let error = GeneratedFlakeError::plain(
                kind,
                Namespace::Store,
                "test-condition",
                Locus::Named("test"),
                "test failure",
                "test reason",
            );
            assert_eq!(error.exit_code(), expected);
        }

        let permission = GeneratedFlakeError::io(
            Namespace::Lock,
            "test-permission",
            PathBuf::from("/test"),
            "permission failure",
            io::Error::from(io::ErrorKind::PermissionDenied),
        );
        assert_eq!(permission.exit_code(), ExitKind::NoPerm);
        assert!(permission.source().is_some());

        let channel = GeneratedFlakeError::io(
            Namespace::Store,
            "test-io",
            PathBuf::from("/test"),
            "channel failure",
            io::Error::other("failed"),
        );
        assert_eq!(channel.exit_code(), ExitKind::IoErr);
        assert!(channel.source().is_some());

        let input = InputError::new(
            "authored defect",
            "invalid-value",
            Locus::Named("inputs"),
            "outside the grammar",
            std::iter::empty::<String>(),
        );
        assert_eq!(input.exit_code(), ExitKind::Config);
    }

    /// Pins the three answers a failed `nix` spawn can give. The middle one is the point: spec/14's
    /// `config eval` and `config sources` rows admit `77` for the Nix preflight, and a `nix` the
    /// host refuses to execute is the filesystem permission failure that code names.
    #[test]
    fn a_refused_nix_spawn_is_a_permission_failure_not_an_unavailable_host() {
        let missing = EvaluationError::NixMissing {
            source: io::Error::from(io::ErrorKind::NotFound),
        };
        assert_eq!(missing.exit_code(), ExitKind::Unavailable);

        let refused = EvaluationError::NixNotPermitted {
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert_eq!(refused.exit_code(), ExitKind::NoPerm);
        assert!(refused.source().is_some());

        let unusable = EvaluationError::NixUnusable {
            source: io::Error::other("failed"),
        };
        assert_eq!(unusable.exit_code(), ExitKind::Unavailable);
    }
}
