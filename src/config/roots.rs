use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::ResolutionError;

/// An injected source of environment variables used during path resolution.
pub trait Environment {
    fn variable(&self, name: &'static str) -> Option<OsString>;
}

impl<F> Environment for F
where
    F: Fn(&'static str) -> Option<OsString>,
{
    fn variable(&self, name: &'static str) -> Option<OsString> {
        self(name)
    }
}

/// The four durable XDG roots, each including vivarium's own subtree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XdgRoots {
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

/// Resolves the four durable roots without inspecting or creating any path.
///
/// # Errors
///
/// Returns [`ResolutionError::MissingHome`] when an ignored XDG override needs a fallback and
/// `HOME` is unset, empty, or relative.
pub fn resolve_xdg_roots(environment: &impl Environment) -> Result<XdgRoots, ResolutionError> {
    Ok(XdgRoots {
        config: resolve_durable_root(environment, "XDG_CONFIG_HOME", ".config")?,
        data: resolve_durable_root(environment, "XDG_DATA_HOME", ".local/share")?,
        state: resolve_durable_root(environment, "XDG_STATE_HOME", ".local/state")?,
        cache: resolve_durable_root(environment, "XDG_CACHE_HOME", ".cache")?,
    })
}

/// Validates the session runtime base and returns vivarium's uncreated subtree beneath it.
///
/// # Errors
///
/// Returns [`ResolutionError`] when `XDG_RUNTIME_DIR` is absent or relative, cannot be inspected,
/// is not a directory, has the wrong owner, or is accessible by group or other users.
pub fn resolve_runtime_root(
    environment: &impl Environment,
    effective_uid: u32,
) -> Result<PathBuf, ResolutionError> {
    let Some(value) = environment.variable("XDG_RUNTIME_DIR") else {
        return Err(ResolutionError::MissingRuntimeDirectory);
    };
    if value.is_empty() {
        return Err(ResolutionError::MissingRuntimeDirectory);
    }

    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(ResolutionError::RelativeRuntimeDirectory { path });
    }

    let metadata =
        fs::metadata(&path).map_err(|source| ResolutionError::InspectRuntimeDirectory {
            path: path.clone(),
            source,
        })?;
    if !metadata.is_dir() {
        return Err(ResolutionError::RuntimePathNotDirectory { path });
    }

    let actual_uid = metadata.uid();
    if actual_uid != effective_uid {
        return Err(ResolutionError::RuntimeDirectoryWrongOwner {
            path,
            expected_uid: effective_uid,
            actual_uid,
        });
    }

    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(ResolutionError::RuntimeDirectoryAccessibleByOthers { path, mode });
    }

    Ok(path.join("vivarium"))
}

fn resolve_durable_root(
    environment: &impl Environment,
    variable: &'static str,
    fallback: &str,
) -> Result<PathBuf, ResolutionError> {
    if let Some(base) = absolute_nonempty(environment.variable(variable)) {
        return Ok(base.join("vivarium"));
    }

    let home = absolute_nonempty(environment.variable("HOME"))
        .ok_or(ResolutionError::MissingHome { variable })?;
    Ok(home.join(fallback).join("vivarium"))
}

fn absolute_nonempty(value: Option<OsString>) -> Option<PathBuf> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    let path = Path::new(&value);
    path.is_absolute().then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    use super::{resolve_runtime_root, resolve_xdg_roots};
    use crate::config::ResolutionError;
    use crate::config::test_support::{ScratchDirectory, TestEnvironment};
    use crate::exit::ExitKind;

    /// Pins spec/02's absolute overrides and required `vivarium` suffix.
    #[test]
    fn xdg_roots_use_absolute_overrides_and_append_vivarium()
    -> Result<(), Box<dyn std::error::Error>> {
        let environment = TestEnvironment::from([
            ("XDG_CONFIG_HOME", "/roots/config"),
            ("XDG_DATA_HOME", "/roots/data"),
            ("XDG_STATE_HOME", "/roots/state"),
            ("XDG_CACHE_HOME", "/roots/cache"),
        ]);

        let roots = resolve_xdg_roots(&environment)?;
        assert_eq!(roots.config, Path::new("/roots/config/vivarium"));
        assert_eq!(roots.data, Path::new("/roots/data/vivarium"));
        assert_eq!(roots.state, Path::new("/roots/state/vivarium"));
        assert_eq!(roots.cache, Path::new("/roots/cache/vivarium"));
        Ok(())
    }

    /// Pins unset, empty, and relative overrides as ignored rather than cwd-relative.
    #[test]
    fn xdg_roots_fall_back_for_unset_empty_and_relative_overrides()
    -> Result<(), Box<dyn std::error::Error>> {
        let environment = TestEnvironment::from([
            ("HOME", "/home/alice"),
            ("XDG_DATA_HOME", ""),
            ("XDG_STATE_HOME", "relative/state"),
            ("XDG_CACHE_HOME", "../cache"),
        ]);

        let roots = resolve_xdg_roots(&environment)?;
        assert_eq!(roots.config, Path::new("/home/alice/.config/vivarium"));
        assert_eq!(roots.data, Path::new("/home/alice/.local/share/vivarium"));
        assert_eq!(roots.state, Path::new("/home/alice/.local/state/vivarium"));
        assert_eq!(roots.cache, Path::new("/home/alice/.cache/vivarium"));
        Ok(())
    }

    /// Pins missing, empty, and relative HOME fallback failures to spec/02's code `78`.
    #[test]
    fn xdg_roots_fail_config_when_home_cannot_supply_a_default() {
        for environment in [
            TestEnvironment::default(),
            TestEnvironment::from([("HOME", "")]),
            TestEnvironment::from([("HOME", "relative/home")]),
        ] {
            let error = resolve_xdg_roots(&environment).err();
            assert!(matches!(
                error.as_ref(),
                Some(ResolutionError::MissingHome {
                    variable: "XDG_CONFIG_HOME"
                })
            ));
            assert_eq!(
                error.as_ref().map(ResolutionError::exit_code),
                Some(ExitKind::Config)
            );
        }
    }

    /// Pins N13: resolving config and durable roots is a read-only operation.
    #[test]
    fn xdg_resolution_creates_no_directories() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let base = scratch.path().join("absent");
        let environment = TestEnvironment::from_paths([
            ("XDG_CONFIG_HOME", base.join("config")),
            ("XDG_DATA_HOME", base.join("data")),
            ("XDG_STATE_HOME", base.join("state")),
            ("XDG_CACHE_HOME", base.join("cache")),
        ]);

        let roots = resolve_xdg_roots(&environment)?;
        for root in [roots.config, roots.data, roots.state, roots.cache] {
            assert!(!root.exists(), "resolver created {}", root.display());
        }
        assert!(!base.exists());
        Ok(())
    }

    /// Pins required absolute runtime input and code `77`, retaining a relative path.
    #[test]
    fn runtime_root_requires_an_absolute_environment_value() {
        for environment in [
            TestEnvironment::default(),
            TestEnvironment::from([("XDG_RUNTIME_DIR", "")]),
        ] {
            let error = resolve_runtime_root(&environment, 1000).err();
            assert!(matches!(
                error.as_ref(),
                Some(ResolutionError::MissingRuntimeDirectory)
            ));
            assert_eq!(
                error.as_ref().map(ResolutionError::exit_code),
                Some(ExitKind::NoPerm)
            );
        }

        let environment = TestEnvironment::from([("XDG_RUNTIME_DIR", "relative/runtime")]);
        let error = resolve_runtime_root(&environment, 1000).err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::RelativeRuntimeDirectory { path })
                if path == Path::new("relative/runtime")
        ));
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::NoPerm)
        );
    }

    /// Pins a non-directory runtime path as a specific code `77` fault.
    #[test]
    fn runtime_root_rejects_a_non_directory() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let file = scratch.path().join("runtime-file");
        fs::write(&file, b"not a directory")?;
        let environment = TestEnvironment::from_paths([("XDG_RUNTIME_DIR", file.clone())]);

        let error = resolve_runtime_root(&environment, fs::metadata(&file)?.uid()).err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::RuntimePathNotDirectory { path }) if path == &file
        ));
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::NoPerm)
        );
        Ok(())
    }

    /// Pins both actual and expected uid in the wrong-owner code `77` fault.
    #[test]
    fn runtime_root_rejects_the_wrong_owner() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = private_runtime_directory()?;
        let actual_uid = fs::metadata(scratch.path())?.uid();
        let expected_uid = actual_uid.wrapping_add(1);
        let environment = TestEnvironment::from_paths([("XDG_RUNTIME_DIR", scratch.path())]);

        let error = resolve_runtime_root(&environment, expected_uid).err();
        assert!(matches!(
            error.as_ref(),
            Some(ResolutionError::RuntimeDirectoryWrongOwner {
                expected_uid: expected,
                actual_uid: actual,
                ..
            }) if *expected == expected_uid && *actual == actual_uid
        ));
        assert_eq!(
            error.as_ref().map(ResolutionError::exit_code),
            Some(ExitKind::NoPerm)
        );
        Ok(())
    }

    /// Pins both group and world access bits as specific code `77` faults.
    #[test]
    fn runtime_root_rejects_group_or_world_access() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let actual_uid = fs::metadata(scratch.path())?.uid();
        let environment = TestEnvironment::from_paths([("XDG_RUNTIME_DIR", scratch.path())]);

        for offending_mode in [0o750, 0o701] {
            fs::set_permissions(scratch.path(), fs::Permissions::from_mode(offending_mode))?;
            let error = resolve_runtime_root(&environment, actual_uid).err();
            assert!(matches!(
                error.as_ref(),
                Some(ResolutionError::RuntimeDirectoryAccessibleByOthers { mode, .. })
                    if *mode & offending_mode == offending_mode
            ));
            assert_eq!(
                error.as_ref().map(ResolutionError::exit_code),
                Some(ExitKind::NoPerm)
            );
        }
        Ok(())
    }

    /// Pins acceptance of an owned `0700` base and N13-like no-create behavior beneath it.
    #[test]
    fn runtime_root_accepts_a_private_owned_directory_without_creating_its_subtree()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = private_runtime_directory()?;
        let actual_uid = fs::metadata(scratch.path())?.uid();
        let environment = TestEnvironment::from_paths([("XDG_RUNTIME_DIR", scratch.path())]);

        let root = resolve_runtime_root(&environment, actual_uid)?;
        assert_eq!(root, scratch.path().join("vivarium"));
        assert!(!root.exists());
        Ok(())
    }

    fn private_runtime_directory() -> Result<ScratchDirectory, Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o700))?;
        Ok(scratch)
    }
}
