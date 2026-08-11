//! Which manifest is in force, and which of the three sources said so.
//!
//! ADR-0011 collapsed an earlier seven-step order down to four steps, and the shape of what it cut
//! is the reason this module is small: there are no repository pointer files to look for and no
//! configured default to fall back through, because vivarium writes nothing into a project's tree
//! and a manifest binding has no meaningful user-wide default. What is left is two overrides that
//! are never persisted, one persisted binding, and failing closed.
//!
//! Failing closed rather than prompting is the load-bearing half. A tool that guessed here would
//! boot a sandbox the user did not ask for, so the absence of a binding is an error carrying the
//! snippet that fixes it — which is also why the snippet is rendered by the registry that would
//! accept it, rather than composed again here.

use std::path::{Path, PathBuf};

use super::registry::Registry;
use super::roots::Environment;

/// The environment variable that overrides the binding for one run.
pub const MANIFEST_VARIABLE: &str = "VIVARIUM_MANIFEST";

/// Which of the three sources named the manifest in force.
///
/// Reported by `viv config --json` as `source`, so the three spellings are a published contract
/// rather than an internal label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingSource {
    /// `--manifest`, a single-invocation override.
    Flag,
    /// `VIVARIUM_MANIFEST`, a runtime override.
    Environment,
    /// The project registry in the state root, the only persisted source.
    Registry,
}

impl BindingSource {
    /// The exact string `viv config --json` emits.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Environment => "env",
            Self::Registry => "registry",
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
/// Takes the flag, the environment, the already-read registry, and the already-canonicalized
/// project path rather than discovering any of them, so the whole precedence rule is decidable
/// without a filesystem — the seam `resolve_xdg_roots` and `parse_manifest` both established.
///
/// `project` must already be canonical. Canonicalizing here would make this fallible for a reason
/// that has nothing to do with precedence, and would let a symlinked path miss a binding recorded
/// under its real one.
#[must_use]
pub fn resolve_binding(
    flag: Option<&str>,
    environment: &impl Environment,
    registry: &Registry,
    project: &Path,
) -> Option<ResolvedBinding> {
    if let Some(manifest) = flag {
        return Some(ResolvedBinding {
            manifest: manifest.to_owned(),
            source: BindingSource::Flag,
        });
    }

    // An empty variable is unset, matching how the XDG roots read their own overrides: an exported
    // but empty value is a shell artifact, not a request to bind to the empty name.
    if let Some(value) = environment.variable(MANIFEST_VARIABLE)
        && !value.is_empty()
        && let Some(manifest) = value.to_str()
    {
        return Some(ResolvedBinding {
            manifest: manifest.to_owned(),
            source: BindingSource::Environment,
        });
    }

    registry.lookup(project).map(|binding| ResolvedBinding {
        manifest: binding.manifest.clone(),
        source: BindingSource::Registry,
    })
}

/// The guidance a fail-closed command prints beside the snippet.
///
/// Both paths are named because spec/01 makes them equally supported: pasting the block and letting
/// vivarium write it are two ways to do one thing, and presenting only the second would read as the
/// real one.
#[must_use]
pub fn unbound_hint(project: &Path, registry_file: &Path) -> String {
    format!(
        "run `viv init --manifest <name> --write`, or add this to `{}`:\n\n{}",
        registry_file.display(),
        Registry::snippet(project, "<name>")
    )
}

/// Canonicalizes a project directory the way the registry key is defined.
///
/// spec/02 requires the same symlink-resolved absolute path identity resolution uses, so that one
/// directory can never acquire two bindings. A path that cannot be canonicalized falls back to
/// itself: a binding lookup that misses is a fail-closed `78`, which is a better answer than an
/// I/O error from a directory the user is standing in.
#[must_use]
pub fn canonical_project(project: &Path) -> PathBuf {
    project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{BindingSource, canonical_project, resolve_binding, unbound_hint};
    use crate::config::registry::Registry;
    use crate::config::test_support::{ScratchDirectory, TestEnvironment};

    fn bound() -> Registry {
        let mut registry = Registry::default();
        registry.bind(
            PathBuf::from("/home/alice/backend"),
            "from-registry".to_owned(),
        );
        registry
    }

    /// Pins ADR-0011's four-step order, highest-wins, at every rung.
    #[test]
    fn the_precedence_order_is_flag_then_environment_then_registry() -> Result<(), String> {
        let project = Path::new("/home/alice/backend");
        let set = TestEnvironment::from([("VIVARIUM_MANIFEST", "from-env")]);
        let unset = TestEnvironment::default();

        // The flag outranks everything below it, including a set variable and a binding.
        let winner = resolve_binding(Some("from-flag"), &set, &bound(), project)
            .ok_or("the flag did not resolve")?;
        assert_eq!(winner.manifest, "from-flag");
        assert_eq!(winner.source, BindingSource::Flag);

        // The variable outranks the registry.
        let winner =
            resolve_binding(None, &set, &bound(), project).ok_or("the variable did not resolve")?;
        assert_eq!(winner.manifest, "from-env");
        assert_eq!(winner.source, BindingSource::Environment);

        // The registry is what is left.
        let winner = resolve_binding(None, &unset, &bound(), project)
            .ok_or("the registry did not resolve")?;
        assert_eq!(winner.manifest, "from-registry");
        assert_eq!(winner.source, BindingSource::Registry);

        // And nothing resolves for a project with no entry: fail closed, never a default.
        assert_eq!(
            resolve_binding(None, &unset, &bound(), Path::new("/home/alice/other")),
            None
        );
        assert_eq!(
            resolve_binding(None, &unset, &Registry::default(), project),
            None
        );
        Ok(())
    }

    /// Pins an exported-but-empty variable as unset, matching how the XDG roots read theirs.
    #[test]
    fn an_empty_environment_override_is_ignored() -> Result<(), String> {
        let empty = TestEnvironment::from([("VIVARIUM_MANIFEST", "")]);
        let winner = resolve_binding(None, &empty, &bound(), Path::new("/home/alice/backend"))
            .ok_or("the registry did not resolve past an empty override")?;
        assert_eq!(winner.source, BindingSource::Registry);
        Ok(())
    }

    /// Pins the three published `source` spellings, which `viv config --json` emits verbatim.
    #[test]
    fn the_source_spellings_are_the_published_ones() {
        assert_eq!(BindingSource::Flag.as_str(), "flag");
        assert_eq!(BindingSource::Environment.as_str(), "env");
        assert_eq!(BindingSource::Registry.as_str(), "registry");
    }

    /// Pins that the fail-closed guidance names both supported paths and carries a real snippet.
    #[test]
    fn the_unbound_hint_offers_both_supported_paths() {
        let hint = unbound_hint(
            Path::new("/home/alice/backend"),
            Path::new("/state/vivarium/registry.toml"),
        );
        assert!(hint.contains("viv init --manifest <name> --write"));
        assert!(hint.contains("/state/vivarium/registry.toml"));
        assert!(hint.contains("[[projects]]"));
        assert!(hint.contains("path = \"/home/alice/backend\""));
    }

    /// Pins the registry key as the symlink-resolved path, so one directory holds one binding.
    #[test]
    fn a_symlinked_project_resolves_to_its_real_path() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let real = scratch.path().join("real");
        std::fs::create_dir(&real)?;
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink(&real, &link)?;

        let mut registry = Registry::default();
        registry.bind(real.canonicalize()?, "bound-once".to_owned());

        // Reached through the link, the project still finds the binding recorded under its real
        // path — the miss this canonicalization exists to prevent.
        let winner = resolve_binding(
            None,
            &TestEnvironment::default(),
            &registry,
            &canonical_project(&link),
        )
        .ok_or("the symlinked project did not resolve")?;
        assert_eq!(winner.manifest, "bound-once");
        Ok(())
    }
}
