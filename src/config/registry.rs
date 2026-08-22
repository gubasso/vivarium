//! The rebuildable workspace-owner index.
//!
//! The index is derived from the manifest library and lives under the cache root. It is never an
//! authority: a missing, malformed, or stale file is rebuilt from manifest names, mtimes, and
//! explicit `[[workspaces]]` declarations. Publication is lockless because concurrent writers
//! derive equivalent answers; each stages a complete same-directory sibling, flushes it, renames
//! it over the old cache, and flushes the parent.

use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rustix::fs::RenameFlags;
use serde::{Deserialize, Serialize};

use super::atomic::{
    Fault, PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, StageFault, create_temp_file,
    remove_file_if_exists, rename_with, sync_directory,
};
use super::error::{RegistryError, RegistryErrorKind};
use crate::diagnostic::Locus;

/// The cache filename. It deliberately cannot collide with the removed authored registry.
pub const INDEX_FILE: &str = "workspace-index.json";

/// One manifest-library file and the mtime that decides whether cached derivation is current.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestStamp {
    /// The artifact name resolved through ADR-0045's two library forms.
    pub name: String,
    /// The resolved manifest file.
    pub path: PathBuf,
    modified_seconds: u64,
    modified_nanos: u32,
}

impl ManifestStamp {
    /// Reads the stamp for one resolved manifest, or `None` when it is no longer there.
    ///
    /// `None` rather than an error for a vanished manifest, and the reason is the scan this feeds:
    /// it walks every member of the manifest library to decide which one owns the invoking
    /// directory. A member removed between the directory listing and this read is by definition
    /// not the owner, so failing the whole invocation on it would refuse a command over an
    /// unrelated file — and refuse it with a diagnostic naming a path that no longer exists, which
    /// a user cannot act on. The caller skips a member it cannot resolve for the same reason.
    ///
    /// # Errors
    ///
    /// Returns a state-channel error if metadata or the modification time cannot be read for any
    /// reason other than the manifest having gone.
    pub fn read(name: String, path: PathBuf) -> Result<Option<Self>, RegistryError> {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(RegistryError::io(
                    "index-source-unreadable",
                    path.clone(),
                    format!("could not inspect manifest `{}`", path.display()),
                    source,
                    false,
                ));
            }
        };
        let modified = metadata.modified().map_err(|source| {
            RegistryError::io(
                "index-source-unreadable",
                path.clone(),
                format!("could not read the mtime of `{}`", path.display()),
                source,
                false,
            )
        })?;
        let elapsed = modified.duration_since(UNIX_EPOCH).map_err(|_| {
            RegistryError::plain(
                RegistryErrorKind::Io,
                "index-source-unreadable",
                Locus::File(path.clone()),
                format!("could not use the mtime of `{}`", path.display()),
                "the manifest modification time predates the Unix epoch",
            )
        })?;
        Ok(Some(Self {
            name,
            path,
            modified_seconds: elapsed.as_secs(),
            modified_nanos: elapsed.subsec_nanos(),
        }))
    }

    #[cfg(test)]
    fn fixture(name: &str, tick: u64) -> Self {
        Self {
            name: name.to_owned(),
            path: PathBuf::from(format!("/config/manifests/{name}.toml")),
            modified_seconds: tick,
            modified_nanos: 0,
        }
    }
}

/// One successfully parsed manifest's recomputable owner declarations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexedManifest {
    /// The bare artifact name.
    pub name: String,
    /// Unexpanded workspace source tokens, in manifest order.
    pub workspace_sources: Vec<String>,
}

/// The current derived answer.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceIndex {
    stamps: Vec<ManifestStamp>,
    manifests: Vec<IndexedManifest>,
}

impl WorkspaceIndex {
    /// Successfully parsed manifests in stable library order.
    #[must_use]
    pub fn manifests(&self) -> &[IndexedManifest] {
        &self.manifests
    }

    /// Derives an index in memory without reading or publishing the cache.
    #[must_use]
    pub fn derive(
        mut stamps: Vec<ManifestStamp>,
        mut derive: impl FnMut(&ManifestStamp) -> Option<Vec<String>>,
    ) -> Self {
        stamps.sort();
        let manifests = stamps
            .iter()
            .filter_map(|stamp| {
                derive(stamp).map(|workspace_sources| IndexedManifest {
                    name: stamp.name.clone(),
                    workspace_sources,
                })
            })
            .collect();
        Self { stamps, manifests }
    }
}

/// Where the derived index lives beneath the cache root.
#[must_use]
pub fn index_path(cache_root: &Path) -> PathBuf {
    cache_root.join(INDEX_FILE)
}

/// Reads a current index or rebuilds and publishes it from the supplied manifest snapshot.
///
/// `derive` returns `None` for an unrelated manifest that is missing or unparsable. Its stamp is
/// still cached, so a broken file does not force a rebuild on every command and changing it does.
///
/// # Errors
///
/// Returns a state-channel error when the existing cache cannot be read for reasons other than
/// absence, or when a rebuilt cache cannot be staged, flushed, renamed, or directory-synced.
pub fn load_or_rebuild(
    cache_root: &Path,
    mut stamps: Vec<ManifestStamp>,
    mut derive: impl FnMut(&ManifestStamp) -> Option<Vec<String>>,
) -> Result<WorkspaceIndex, RegistryError> {
    stamps.sort();
    if let Some(index) = read_current(cache_root)?
        && index.stamps == stamps
    {
        return Ok(index);
    }

    let index = WorkspaceIndex::derive(stamps, &mut derive);
    publish(cache_root, &index)?;
    Ok(index)
}

fn read_current(cache_root: &Path) -> Result<Option<WorkspaceIndex>, RegistryError> {
    let path = index_path(cache_root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(RegistryError::io(
                "index-unreadable",
                path.clone(),
                format!("could not read the derived index `{}`", path.display()),
                source,
                false,
            ));
        }
    };
    // Derived data has no authored-defect state. A torn, old, or otherwise malformed cache is a
    // miss, and the complete manifest snapshot below is the repair.
    Ok(serde_json::from_slice(&bytes).ok())
}

fn publish(cache_root: &Path, index: &WorkspaceIndex) -> Result<(), RegistryError> {
    ensure_cache_root(cache_root)?;
    let destination = index_path(cache_root);
    let bytes = serde_json::to_vec(index).map_err(|source| {
        RegistryError::plain(
            RegistryErrorKind::Io,
            "index-encode",
            Locus::File(destination.clone()),
            "could not encode the derived workspace index",
            source.to_string(),
        )
    })?;
    let (temporary, mut file) = create_temp_file(cache_root, INDEX_FILE, PRIVATE_FILE_MODE)
        .map_err(|fault| stage_fault("index-stage", fault))?;
    let result = (|| {
        file.write_all(&bytes).map_err(|source| {
            write_error(
                "index-write",
                &temporary,
                "could not write the staged workspace index",
                source,
            )
        })?;
        file.sync_all().map_err(|source| {
            write_error(
                "index-sync",
                &temporary,
                "could not flush the staged workspace index",
                source,
            )
        })?;
        drop(file);
        rename_with(&temporary, &destination, RenameFlags::empty()).map_err(|source| {
            write_error(
                "index-publish",
                &destination,
                "could not publish the workspace index",
                source,
            )
        })?;
        sync_directory(cache_root).map_err(|fault| {
            write_error(
                "index-directory-sync",
                &fault.path,
                "could not flush the cache root",
                fault.source,
            )
        })
    })();
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

fn ensure_cache_root(cache_root: &Path) -> Result<(), RegistryError> {
    fs::create_dir_all(cache_root).map_err(|source| {
        write_error(
            "index-parent",
            cache_root,
            "could not create the cache root",
            source,
        )
    })?;
    fs::set_permissions(cache_root, fs::Permissions::from_mode(PRIVATE_DIR_MODE)).map_err(
        |source| {
            write_error(
                "index-permissions",
                cache_root,
                "could not make the cache root private",
                source,
            )
        },
    )
}

fn stage_fault(condition: &'static str, fault: StageFault) -> RegistryError {
    match fault {
        StageFault::Io(Fault { path, source }) => write_error(
            condition,
            &path,
            "could not stage the workspace index",
            source,
        ),
        StageFault::Exhausted(parent) => RegistryError::plain(
            RegistryErrorKind::Io,
            "index-temporary-collision",
            Locus::File(parent),
            "could not allocate a staged workspace-index sibling",
            "the bounded temporary-name retry set was exhausted",
        ),
    }
}

fn write_error(
    condition: &'static str,
    path: &Path,
    message: &str,
    source: io::Error,
) -> RegistryError {
    RegistryError::io(
        condition,
        path.to_path_buf(),
        format!("{message}: `{}`", path.display()),
        source,
        true,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    use super::{INDEX_FILE, ManifestStamp, index_path, load_or_rebuild};
    use crate::config::test_support::ScratchDirectory;

    #[test]
    fn absent_malformed_and_stale_indexes_rebuild() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let cache = scratch.path().join("cache");
        let calls = Cell::new(0);
        let derive = |stamp: &ManifestStamp| {
            calls.set(calls.get() + 1);
            Some(vec![format!("/work/{}", stamp.name)])
        };

        let one = vec![ManifestStamp::fixture("one", 1)];
        let built = load_or_rebuild(&cache, one.clone(), derive)?;
        assert_eq!(built.manifests()[0].workspace_sources, ["/work/one"]);
        assert_eq!(calls.get(), 1);

        let cached = load_or_rebuild(&cache, one.clone(), derive)?;
        assert_eq!(cached, built);
        assert_eq!(calls.get(), 1, "a current cache was rebuilt");

        fs::write(index_path(&cache), b"not json")?;
        load_or_rebuild(&cache, one, derive)?;
        assert_eq!(calls.get(), 2, "a malformed cache was not rebuilt");

        load_or_rebuild(&cache, vec![ManifestStamp::fixture("one", 2)], derive)?;
        assert_eq!(
            calls.get(),
            3,
            "an mtime change did not invalidate the cache"
        );
        Ok(())
    }

    #[test]
    fn publication_is_private_atomic_and_leaves_no_staged_sibling()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let cache = scratch.path().join("cache");
        let first = vec![ManifestStamp::fixture("one", 1)];
        load_or_rebuild(&cache, first, |_| Some(vec!["/one".to_owned()]))?;
        let path = index_path(&cache);
        let first_inode = fs::metadata(&path)?.ino();

        let second = vec![ManifestStamp::fixture("two", 1)];
        load_or_rebuild(&cache, second, |_| Some(vec!["/two".to_owned()]))?;
        assert_ne!(first_inode, fs::metadata(&path)?.ino());
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&cache)?.permissions().mode() & 0o777, 0o700);
        let strays = fs::read_dir(&cache)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".new"))
            .count();
        assert_eq!(strays, 0);
        Ok(())
    }

    #[test]
    fn old_authored_registry_is_neither_read_written_nor_deleted()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        let cache = scratch.path().join("cache");
        fs::create_dir(&state)?;
        let old = state.join("registry.toml");
        let original = b"[[projects]]\npath = \"/old\"\nmanifest = \"old\"\n";
        fs::write(&old, original)?;

        load_or_rebuild(&cache, Vec::new(), |_| None)?;
        assert_eq!(fs::read(&old)?, original);
        assert!(cache.join(INDEX_FILE).is_file());
        assert!(!state.join(INDEX_FILE).exists());
        Ok(())
    }
}
