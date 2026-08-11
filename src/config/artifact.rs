use std::fmt;
use std::path::{Path, PathBuf};

use super::ResolutionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    Image,
    Piece,
    Manifest,
}

impl ArtifactKind {
    const fn library(self) -> &'static str {
        match self {
            Self::Image => "images",
            Self::Piece => "pieces",
            Self::Manifest => "manifests",
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Image | Self::Piece => "nix",
            Self::Manifest => "toml",
        }
    }
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Image => "image",
            Self::Piece => "piece",
            Self::Manifest => "manifest",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactForm {
    Flat,
    Directory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedArtifact {
    pub kind: ArtifactKind,
    pub name: String,
    pub form: ArtifactForm,
    pub path: PathBuf,
}

/// Resolves exactly the flat and directory candidates fixed by ADR-0045.
///
/// # Errors
///
/// Returns [`ResolutionError`] when the name is invalid, a candidate cannot be inspected, neither
/// candidate exists, or both candidates exist.
pub fn resolve_artifact(
    config_root: &Path,
    kind: ArtifactKind,
    name: &str,
) -> Result<ResolvedArtifact, ResolutionError> {
    if !valid_artifact_name(name) {
        return Err(ResolutionError::InvalidArtifactName {
            name: name.to_owned(),
        });
    }

    let library = config_root.join(kind.library());
    let flat = library.join(format!("{name}.{}", kind.extension()));
    let directory = library
        .join(name)
        .join(format!("default.{}", kind.extension()));
    let flat_exists = probe(&flat)?;
    let directory_exists = probe(&directory)?;

    match (flat_exists, directory_exists) {
        (true, false) => Ok(resolved(kind, name, ArtifactForm::Flat, flat)),
        (false, true) => Ok(resolved(kind, name, ArtifactForm::Directory, directory)),
        (false, false) => Err(ResolutionError::ArtifactNotFound {
            kind,
            name: name.to_owned(),
            flat,
            directory,
        }),
        (true, true) => Err(ResolutionError::ArtifactAmbiguous {
            kind,
            name: name.to_owned(),
            flat,
            directory,
        }),
    }
}

fn valid_artifact_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let valid_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    !bytes.is_empty()
        && name.is_ascii()
        && bytes.first().is_some_and(|byte| valid_edge(*byte))
        && bytes.last().is_some_and(|byte| valid_edge(*byte))
        && bytes.iter().all(|byte| valid_edge(*byte) || *byte == b'-')
}

fn probe(path: &Path) -> Result<bool, ResolutionError> {
    path.try_exists()
        .map_err(|source| ResolutionError::InspectArtifact {
            path: path.to_path_buf(),
            source,
        })
}

fn resolved(kind: ArtifactKind, name: &str, form: ArtifactForm, path: PathBuf) -> ResolvedArtifact {
    ResolvedArtifact {
        kind,
        name: name.to_owned(),
        form,
        path,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{ArtifactForm, ArtifactKind, resolve_artifact};
    use crate::config::ResolutionError;
    use crate::config::test_support::ScratchDirectory;
    use crate::exit::ExitKind;

    /// Pins the exact `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$` grammar.
    #[test]
    fn artifact_names_match_the_fixed_kebab_case_grammar() {
        let root = Path::new("/definitely-absent");
        for name in ["a", "7", "rust-web", "a--b"] {
            assert!(!matches!(
                resolve_artifact(root, ArtifactKind::Image, name),
                Err(ResolutionError::InvalidArtifactName { .. })
            ));
        }
        for name in [
            "",
            "Upper",
            "under_score",
            "with/slash",
            "bad..dots",
            "-lead",
            "trail-",
        ] {
            assert!(matches!(
                resolve_artifact(root, ArtifactKind::Image, name),
                Err(ResolutionError::InvalidArtifactName { .. })
            ));
        }
    }

    /// Pins all six flat/directory paths in ADR-0045.
    #[test]
    fn each_library_resolves_flat_and_directory_forms() -> Result<(), Box<dyn std::error::Error>> {
        let rows = [
            (ArtifactKind::Image, "images", "nix"),
            (ArtifactKind::Piece, "pieces", "nix"),
            (ArtifactKind::Manifest, "manifests", "toml"),
        ];
        for (kind, library, extension) in rows {
            for form in [ArtifactForm::Flat, ArtifactForm::Directory] {
                let scratch = ScratchDirectory::new()?;
                let path = candidate(scratch.path(), library, "sample", extension, form);
                create_file(&path)?;

                let artifact = resolve_artifact(scratch.path(), kind, "sample")?;
                assert_eq!(artifact.kind, kind);
                assert_eq!(artifact.name, "sample");
                assert_eq!(artifact.form, form);
                assert_eq!(artifact.path, path);
            }
        }
        Ok(())
    }

    /// Pins ambiguity as code `78`, retaining and displaying both paths.
    #[test]
    fn both_artifact_forms_fail_closed_and_name_both_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let flat = scratch.path().join("images/demo.nix");
        let directory = scratch.path().join("images/demo/default.nix");
        create_file(&flat)?;
        create_file(&directory)?;

        let error = resolve_artifact(scratch.path(), ArtifactKind::Image, "demo").err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::ArtifactAmbiguous {
                flat,
                directory,
                ..
            }) if flat == &scratch.path().join("images/demo.nix")
                && directory == &scratch.path().join("images/demo/default.nix")
        ));
        let display = error.as_ref().map(ToString::to_string).unwrap_or_default();
        assert!(display.contains(&flat.display().to_string()));
        assert!(display.contains(&directory.display().to_string()));
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::Config)
        );
        Ok(())
    }

    /// Pins missing artifacts to code `78` with both attempted paths retained.
    #[test]
    fn a_missing_artifact_is_a_config_error() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let flat = scratch.path().join("pieces/missing.nix");
        let directory = scratch.path().join("pieces/missing/default.nix");
        let error = resolve_artifact(scratch.path(), ArtifactKind::Piece, "missing").err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::ArtifactNotFound {
                flat,
                directory,
                ..
            }) if flat == &scratch.path().join("pieces/missing.nix")
                && directory == &scratch.path().join("pieces/missing/default.nix")
        ));
        assert_eq!(flat, scratch.path().join("pieces/missing.nix"));
        assert_eq!(directory, scratch.path().join("pieces/missing/default.nix"));
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::Config)
        );
        Ok(())
    }

    /// Pins a genuine probe-channel failure to code `74` with source chaining.
    #[test]
    fn an_artifact_probe_failure_preserves_its_source() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        fs::write(scratch.path().join("images"), b"not a directory")?;

        let error = resolve_artifact(scratch.path(), ArtifactKind::Image, "demo").err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::InspectArtifact { .. })
        ));
        assert!(error.as_ref().and_then(std::error::Error::source).is_some());
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::IoErr)
        );
        Ok(())
    }

    /// Pins non-members as invisible: only the two fixed candidates can satisfy a name.
    #[test]
    fn unrelated_library_files_are_invisible_to_resolution()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        for relative in [
            "images/README.md",
            "images/helper.nix",
            "images/not_valid/default.nix",
            "images/flake.lock",
            "images/inputs.toml",
        ] {
            create_file(&scratch.path().join(relative))?;
        }

        assert!(matches!(
            resolve_artifact(scratch.path(), ArtifactKind::Image, "wanted"),
            Err(ResolutionError::ArtifactNotFound { .. })
        ));
        let fixed = scratch.path().join("images/wanted.nix");
        create_file(&fixed)?;
        assert_eq!(
            resolve_artifact(scratch.path(), ArtifactKind::Image, "wanted")?.path,
            fixed
        );
        Ok(())
    }

    /// Pins ADR-0061's config-root-only search with no bundled or sibling fallback.
    #[test]
    fn the_config_root_is_the_complete_search_path() -> Result<(), Box<dyn std::error::Error>> {
        let supplied = ScratchDirectory::new()?;
        let outside = ScratchDirectory::new()?;
        create_file(&outside.path().join("manifests/demo.toml"))?;

        assert!(matches!(
            resolve_artifact(supplied.path(), ArtifactKind::Manifest, "demo"),
            Err(ResolutionError::ArtifactNotFound { .. })
        ));
        Ok(())
    }

    fn candidate(
        root: &Path,
        library: &str,
        name: &str,
        extension: &str,
        form: ArtifactForm,
    ) -> PathBuf {
        match form {
            ArtifactForm::Flat => root.join(library).join(format!("{name}.{extension}")),
            ArtifactForm::Directory => root
                .join(library)
                .join(name)
                .join(format!("default.{extension}")),
        }
    }

    fn create_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, b"fixture")?;
        Ok(())
    }
}
