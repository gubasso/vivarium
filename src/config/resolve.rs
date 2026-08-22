//! Which manifest is in force, and which of the three sources said so.
//!
//! ADR-0011's precedence remains a pure decision: two overrides that are never persisted, then a
//! workspace owner already derived by the filesystem-owning caller, then failing closed. This
//! module performs no discovery or I/O.

use std::path::{Path, PathBuf};

use super::roots::Environment;

/// The environment variable that overrides workspace-derived ownership for one run.
pub const MANIFEST_VARIABLE: &str = "VIVARIUM_MANIFEST";

/// Which of the three sources named the manifest in force.
///
/// Reported by `viv config --json` as `source`, so the spellings are a published contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingSource {
    /// `--manifest`, a single-invocation override.
    Flag,
    /// `VIVARIUM_MANIFEST`, a runtime override.
    Environment,
    /// The unique owner derived from explicit workspaces in the manifest library.
    Derived,
}

impl BindingSource {
    /// The exact string `viv config --json` emits.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Environment => "env",
            Self::Derived => "derived",
        }
    }
}

/// A manifest name and where it came from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedBinding {
    /// The bare kebab-case manifest name.
    pub manifest: String,
    /// Which source won.
    pub source: BindingSource,
}

/// Resolves the effective manifest highest-wins, or reports that none does.
///
/// Takes the already-derived third-rung name rather than discovering it, which preserves this
/// module's filesystem-free precedence seam.
#[must_use]
pub fn resolve_binding(
    flag: Option<&str>,
    environment: &impl Environment,
    derived: Option<&str>,
) -> Option<ResolvedBinding> {
    if let Some(manifest) = flag {
        return Some(ResolvedBinding {
            manifest: manifest.to_owned(),
            source: BindingSource::Flag,
        });
    }

    // An empty variable is unset, matching how the XDG roots read their own overrides.
    if let Some(value) = environment.variable(MANIFEST_VARIABLE)
        && !value.is_empty()
        && let Some(manifest) = value.to_str()
    {
        return Some(ResolvedBinding {
            manifest: manifest.to_owned(),
            source: BindingSource::Environment,
        });
    }

    derived.map(|manifest| ResolvedBinding {
        manifest: manifest.to_owned(),
        source: BindingSource::Derived,
    })
}

/// The guidance a fail-closed command prints beside an absent derived owner.
#[must_use]
pub fn unbound_hint(project: &Path) -> String {
    format!(
        "add `[[workspaces]]\nsource = '{}'` to exactly one manifest, or select one with \
        `--manifest <name>`",
        project.display()
    )
}

/// Canonicalizes an invoking directory before workspace membership is compared.
#[must_use]
pub fn canonical_project(project: &Path) -> PathBuf {
    project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{BindingSource, canonical_project, resolve_binding, unbound_hint};
    use crate::config::test_support::{ScratchDirectory, TestEnvironment};

    #[test]
    fn the_precedence_order_is_flag_then_environment_then_derived() -> Result<(), String> {
        let set = TestEnvironment::from([("VIVARIUM_MANIFEST", "from-env")]);
        let unset = TestEnvironment::default();

        let winner = resolve_binding(Some("from-flag"), &set, Some("from-derived"))
            .ok_or("the flag did not resolve")?;
        assert_eq!(winner.manifest, "from-flag");
        assert_eq!(winner.source, BindingSource::Flag);

        let winner = resolve_binding(None, &set, Some("from-derived"))
            .ok_or("the environment did not resolve")?;
        assert_eq!(winner.manifest, "from-env");
        assert_eq!(winner.source, BindingSource::Environment);

        let winner = resolve_binding(None, &unset, Some("from-derived"))
            .ok_or("the derived owner did not resolve")?;
        assert_eq!(winner.manifest, "from-derived");
        assert_eq!(winner.source, BindingSource::Derived);
        assert_eq!(resolve_binding(None, &unset, None), None);
        Ok(())
    }

    #[test]
    fn an_empty_environment_override_is_ignored() -> Result<(), String> {
        let empty = TestEnvironment::from([("VIVARIUM_MANIFEST", "")]);
        let winner = resolve_binding(None, &empty, Some("from-derived"))
            .ok_or("the derived owner did not resolve past an empty override")?;
        assert_eq!(winner.source, BindingSource::Derived);
        Ok(())
    }

    #[test]
    fn the_source_spellings_are_the_published_ones() {
        assert_eq!(BindingSource::Flag.as_str(), "flag");
        assert_eq!(BindingSource::Environment.as_str(), "env");
        assert_eq!(BindingSource::Derived.as_str(), "derived");
    }

    #[test]
    fn the_unbound_hint_names_the_derived_declaration() {
        let hint = unbound_hint(Path::new("/home/alice/backend"));
        assert!(hint.contains("[[workspaces]]"));
        assert!(hint.contains("/home/alice/backend"));
        assert!(hint.contains("--manifest <name>"));
    }

    #[test]
    fn a_symlinked_invocation_canonicalizes_to_its_real_path()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let real = scratch.path().join("real");
        std::fs::create_dir(&real)?;
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink(&real, &link)?;
        assert_eq!(canonical_project(&link), real.canonicalize()?);
        Ok(())
    }
}
