//! Atomic filesystem publication for generated flakes and first pins.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

use rustix::fs::RenameFlags;

use super::atomic::{self, Fault, StageFault};
use super::error::GeneratedFlakeErrorKind;
use super::flake::GeneratedEntry;
use super::{
    BaselineInputs, EffectiveLock, GeneratedFlakeError, GeneratedFlakePlan, Manifest,
    PreparedFlake, ResolvedArtifact, ResolvedComposition, XdgRoots,
};
use crate::diagnostic::{Locus, Namespace};

/// The mode a staged generated file takes. Unlike the state root's `0600`, a generated tree is
/// ordinary cache a build reads, so it keeps the umask's default rather than being made private.
const STAGED_FILE_MODE: u32 = 0o666;

/// Resolves, plans, and atomically publishes one generated flake.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when read-only resolution fails or the complete replacement
/// cannot be prepared, flushed, published, or cleaned up safely.
pub fn prepare_generated_flake(
    roots: &XdgRoots,
    project_id: &str,
    target: &str,
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    manifest: &Manifest,
    baseline: &BaselineInputs,
) -> Result<PreparedFlake, GeneratedFlakeError> {
    let composition = ResolvedComposition::resolve(
        roots,
        project_id,
        target,
        selected_manifest,
        manifest_source,
        manifest,
    )?;
    let plan = GeneratedFlakePlan::build(
        roots,
        selected_manifest,
        manifest_source,
        manifest,
        &composition,
        baseline,
    )?;
    publish(&plan)
}

/// Persists a lock created by a successful first build with atomic no-replace semantics.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] if the preparation was not lock-creation eligible, the
/// generated lock cannot be read, the owned pin cannot be installed, or a different concurrent
/// first pin won.
#[allow(clippy::too_many_lines)]
pub fn persist_created_lock(
    prepared: &PreparedFlake,
) -> Result<EffectiveLock, GeneratedFlakeError> {
    let EffectiveLock::OwnedMissing { path: owned_path } = &prepared.effective_lock else {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "lock-persist-contract",
            Locus::Named("generated flake"),
            "lock persistence requested for an ineligible preparation",
            "only a successful build prepared without an owned or override lock may persist one",
        ));
    };
    let generated_path = prepared.directory.join("flake.lock");
    let bytes = fs::read(&generated_path).map_err(|source| {
        GeneratedFlakeError::io(
            Namespace::Lock,
            "generated-read",
            generated_path.clone(),
            format!(
                "could not read generated lock `{}`",
                generated_path.display()
            ),
            source,
        )
    })?;
    let parent = owned_path.parent().ok_or_else(|| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "lock-parent",
            Locus::File(owned_path.clone()),
            "owned lock has no parent directory",
            "the generated lock path escaped its fixed data-root layout",
        )
    })?;
    fs::create_dir_all(parent).map_err(|source| {
        GeneratedFlakeError::io(
            Namespace::Lock,
            "persist-parent",
            parent.to_path_buf(),
            format!("could not create lock directory `{}`", parent.display()),
            source,
        )
    })?;
    let (temporary, mut file) = create_temp_file(parent, "flake.lock")?;
    let result = (|| {
        file.write_all(&bytes).map_err(|source| {
            lock_io(
                "persist-write",
                &temporary,
                "could not write staged lock",
                source,
            )
        })?;
        file.sync_all().map_err(|source| {
            lock_io(
                "persist-sync",
                &temporary,
                "could not flush staged lock",
                source,
            )
        })?;
        drop(file);
        match rename_with(&temporary, owned_path, RenameFlags::NOREPLACE) {
            Ok(()) => {
                sync_directory(parent, Namespace::Lock, "persist-directory-sync")?;
                Ok(EffectiveLock::OwnedExisting {
                    path: owned_path.clone(),
                })
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                let winner = fs::read(owned_path).map_err(|source| {
                    lock_io(
                        "winner-read",
                        owned_path,
                        "could not read concurrently installed lock",
                        source,
                    )
                })?;
                remove_file_if_exists(&temporary);
                if winner == bytes {
                    Ok(EffectiveLock::OwnedExisting {
                        path: owned_path.clone(),
                    })
                } else {
                    Err(GeneratedFlakeError::plain(
                        GeneratedFlakeErrorKind::Race,
                        Namespace::Lock,
                        "first-pin-race",
                        Locus::File(owned_path.clone()),
                        "a different first pin won concurrently",
                        "discard this build and retry from the installed lock",
                    ))
                }
            }
            Err(source) => Err(lock_io(
                "persist-publish",
                owned_path,
                "could not atomically install first pin",
                source,
            )),
        }
    })();
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

fn publish(plan: &GeneratedFlakePlan) -> Result<PreparedFlake, GeneratedFlakeError> {
    let parent = plan.directory.parent().ok_or_else(|| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "generated-parent",
            Locus::File(plan.directory.clone()),
            "generated flake directory has no parent",
            "the path escaped its fixed cache-root layout",
        )
    })?;
    fs::create_dir_all(parent).map_err(|source| {
        store_io(
            "create-parent",
            parent,
            "could not create generated-flake parent",
            source,
        )
    })?;
    let temporary = create_temp_directory(parent)?;
    let prepared = (|| {
        for entry in &plan.entries {
            apply_entry(&temporary, entry)?;
        }
        sync_tree(&temporary)?;
        publish_directory(&temporary, &plan.directory)?;
        // After the tree, so a publication that fails sheds nothing durable: the migration
        // rewrite (ADR-0102) replaces the owned lock with bytes that differ only by the removed
        // subtree, and a concurrent preparation computes the identical bytes, so replacement is
        // race-benign in a way a first pin is not.
        if let Some(bytes) = &plan.migrated_lock {
            rewrite_owned_lock(plan.effective_lock.path(), bytes)?;
        }
        Ok(PreparedFlake {
            directory: plan.directory.clone(),
            effective_lock: plan.effective_lock.clone(),
            shed_vivarium: plan.migrated_lock.is_some(),
        })
    })();
    if prepared.is_err() && temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    prepared
}

/// Atomically replaces the owned lock with its migrated bytes.
fn rewrite_owned_lock(owned_path: &Path, bytes: &[u8]) -> Result<(), GeneratedFlakeError> {
    let parent = owned_path.parent().ok_or_else(|| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "lock-parent",
            Locus::File(owned_path.to_path_buf()),
            "owned lock has no parent directory",
            "the lock path escaped its fixed data-root layout",
        )
    })?;
    let (temporary, mut file) = create_temp_file(parent, "flake.lock")?;
    let result = (|| {
        file.write_all(bytes).map_err(|source| {
            lock_io(
                "migrate-write",
                &temporary,
                "could not write the migrated lock",
                source,
            )
        })?;
        file.sync_all().map_err(|source| {
            lock_io(
                "migrate-sync",
                &temporary,
                "could not flush the migrated lock",
                source,
            )
        })?;
        drop(file);
        fs::rename(&temporary, owned_path).map_err(|source| {
            lock_io(
                "migrate-install",
                owned_path,
                "could not install the migrated lock",
                source,
            )
        })
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn apply_entry(root: &Path, entry: &GeneratedEntry) -> Result<(), GeneratedFlakeError> {
    let destination = root.join(entry.destination());
    if !destination.starts_with(root) {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "generated-path",
            Locus::File(destination),
            "generated destination escaped its temporary tree",
            "all generated entries must use bounded relative paths",
        ));
    }
    match entry {
        GeneratedEntry::RenderedFile { bytes, .. } => write_file(&destination, bytes, None),
        GeneratedEntry::CopiedFile { source, .. } => copy_file(source, &destination),
        GeneratedEntry::CopiedTree { source, .. } => copy_tree(source, &destination),
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), GeneratedFlakeError> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        // A library a user has not created yet is a normal state, not a broken one — the same
        // reading `manifest list` gives an absent `manifests/`. The generated tree still gets the
        // directory, because the flake's module paths are written against it either way.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return fs::create_dir(destination).map_err(|error| {
                store_io(
                    "copy-create-directory",
                    destination,
                    "could not create copied directory",
                    error,
                )
            });
        }
        Err(error) => {
            return Err(store_io(
                "copy-inspect",
                source,
                "could not inspect copied tree",
                error,
            ));
        }
    };
    if !metadata.is_dir() {
        return Err(unsupported_type(source));
    }
    fs::create_dir(destination).map_err(|error| {
        store_io(
            "copy-create-directory",
            destination,
            "could not create copied directory",
            error,
        )
    })?;
    let entries = fs::read_dir(source).map_err(|error| {
        store_io(
            "copy-read-directory",
            source,
            "could not read copied directory",
            error,
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            store_io(
                "copy-read-directory",
                source,
                "could not read directory entry",
                error,
            )
        })?;
        let source_child = entry.path();
        let destination_child = destination.join(entry.file_name());
        let child_metadata = fs::symlink_metadata(&source_child).map_err(|error| {
            store_io(
                "copy-inspect",
                &source_child,
                "could not inspect copied entry",
                error,
            )
        })?;
        let kind = child_metadata.file_type();
        if kind.is_dir() {
            copy_tree(&source_child, &destination_child)?;
        } else if kind.is_file() {
            copy_file(&source_child, &destination_child)?;
        } else if kind.is_symlink() {
            let target = fs::read_link(&source_child).map_err(|error| {
                store_io(
                    "copy-read-link",
                    &source_child,
                    "could not read copied symlink",
                    error,
                )
            })?;
            symlink(&target, &destination_child).map_err(|error| {
                store_io(
                    "copy-create-link",
                    &destination_child,
                    "could not recreate copied symlink",
                    error,
                )
            })?;
        } else {
            return Err(unsupported_type(&source_child));
        }
    }
    fs::set_permissions(destination, metadata.permissions()).map_err(|error| {
        store_io(
            "copy-permissions",
            destination,
            "could not preserve directory permissions",
            error,
        )
    })?;
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), GeneratedFlakeError> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        store_io(
            "copy-inspect",
            source,
            "could not inspect copied file",
            error,
        )
    })?;
    if !metadata.is_file() {
        return Err(unsupported_type(source));
    }
    let bytes = fs::read(source)
        .map_err(|error| store_io("copy-read", source, "could not read copied file", error))?;
    write_file(destination, &bytes, Some(metadata.permissions().mode()))
}

fn write_file(
    destination: &Path,
    bytes: &[u8],
    mode: Option<u32>,
) -> Result<(), GeneratedFlakeError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            store_io(
                "write-parent",
                parent,
                "could not create generated directory",
                error,
            )
        })?;
    }
    let mut file = File::create(destination).map_err(|error| {
        store_io(
            "write-file",
            destination,
            "could not create generated file",
            error,
        )
    })?;
    file.write_all(bytes).map_err(|error| {
        store_io(
            "write-file",
            destination,
            "could not write generated file",
            error,
        )
    })?;
    if let Some(mode) = mode {
        file.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|error| {
                store_io(
                    "copy-permissions",
                    destination,
                    "could not preserve file permissions",
                    error,
                )
            })?;
    }
    file.sync_all().map_err(|error| {
        store_io(
            "write-sync",
            destination,
            "could not flush generated file",
            error,
        )
    })
}

fn sync_tree(directory: &Path) -> Result<(), GeneratedFlakeError> {
    for entry in fs::read_dir(directory).map_err(|error| {
        store_io(
            "sync-read-directory",
            directory,
            "could not inspect generated directory before sync",
            error,
        )
    })? {
        let entry = entry.map_err(|error| {
            store_io(
                "sync-read-directory",
                directory,
                "could not inspect generated directory entry",
                error,
            )
        })?;
        if fs::symlink_metadata(entry.path())
            .map_err(|error| {
                store_io(
                    "sync-inspect",
                    &entry.path(),
                    "could not inspect generated entry before sync",
                    error,
                )
            })?
            .is_dir()
        {
            sync_tree(&entry.path())?;
        }
    }
    sync_directory(directory, Namespace::Store, "sync-directory")
}

fn publish_directory(temporary: &Path, destination: &Path) -> Result<(), GeneratedFlakeError> {
    match destination.try_exists() {
        Ok(false) => match fs::rename(temporary, destination) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                exchange_and_cleanup(temporary, destination)?;
            }
            Err(error) => {
                return Err(store_io(
                    "publish",
                    destination,
                    "could not publish generated flake",
                    error,
                ));
            }
        },
        Ok(true) => exchange_and_cleanup(temporary, destination)?,
        Err(error) => {
            return Err(store_io(
                "publish-inspect",
                destination,
                "could not inspect generated-flake destination",
                error,
            ));
        }
    }
    let parent = destination.parent().ok_or_else(|| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "generated-parent",
            Locus::File(destination.to_path_buf()),
            "published directory has no parent",
            "the path escaped its fixed cache-root layout",
        )
    })?;
    sync_directory(parent, Namespace::Store, "publish-directory-sync")
}

fn exchange_and_cleanup(temporary: &Path, destination: &Path) -> Result<(), GeneratedFlakeError> {
    rename_with(temporary, destination, RenameFlags::EXCHANGE).map_err(|error| {
        store_io(
            "publish-exchange",
            destination,
            "could not atomically exchange generated flake",
            error,
        )
    })?;
    fs::remove_dir_all(temporary).map_err(|error| {
        store_io(
            "cleanup-stale",
            temporary,
            "published generated flake but could not remove stale sibling",
            error,
        )
    })
}

fn rename_with(source: &Path, destination: &Path, flags: RenameFlags) -> io::Result<()> {
    atomic::rename_with(source, destination, flags)
}

fn create_temp_directory(parent: &Path) -> Result<PathBuf, GeneratedFlakeError> {
    atomic::create_temp_directory(parent).map_err(|fault| {
        staged(
            fault,
            Namespace::Store,
            "create-temporary",
            "temporary-collision",
            "could not create temporary generated-flake sibling",
            "could not allocate a unique generated-flake sibling",
        )
    })
}

fn create_temp_file(parent: &Path, stem: &str) -> Result<(PathBuf, File), GeneratedFlakeError> {
    atomic::create_temp_file(parent, stem, STAGED_FILE_MODE).map_err(|fault| {
        staged(
            fault,
            Namespace::Lock,
            "persist-temporary",
            "temporary-collision",
            "could not create staged lock",
            "could not allocate a unique staged-lock sibling",
        )
    })
}

/// Names a shared staging failure in this module's own vocabulary.
///
/// The mechanics live in `atomic` and the two frozen id strings live here, which is the split that
/// lets the registry reuse the same steps without inheriting `store.` and `lock.` ids.
fn staged(
    fault: StageFault,
    namespace: Namespace,
    io_condition: &'static str,
    exhausted_condition: &'static str,
    io_message: &'static str,
    exhausted_message: &'static str,
) -> GeneratedFlakeError {
    match fault {
        StageFault::Io(Fault { path, source }) => GeneratedFlakeError::io(
            namespace,
            io_condition,
            path.clone(),
            format!("{io_message}: `{}`", path.display()),
            source,
        ),
        StageFault::Exhausted(parent) => GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Io,
            namespace,
            exhausted_condition,
            Locus::File(parent),
            exhausted_message,
            "the bounded temporary-name retry set was exhausted",
        ),
    }
}

fn sync_directory(
    path: &Path,
    namespace: Namespace,
    condition: &'static str,
) -> Result<(), GeneratedFlakeError> {
    atomic::sync_directory(path).map_err(|Fault { path, source }| {
        GeneratedFlakeError::io(
            namespace,
            condition,
            path.clone(),
            format!("could not flush directory `{}`", path.display()),
            source,
        )
    })
}

fn unsupported_type(path: &Path) -> GeneratedFlakeError {
    GeneratedFlakeError::plain(
        GeneratedFlakeErrorKind::Io,
        Namespace::Store,
        "unsupported-file-type",
        Locus::File(path.to_path_buf()),
        format!("cannot copy unsupported file type at `{}`", path.display()),
        "generated trees admit only directories, ordinary files, and symbolic links",
    )
}

fn store_io(
    condition: &'static str,
    path: &Path,
    message: &'static str,
    source: io::Error,
) -> GeneratedFlakeError {
    GeneratedFlakeError::io(
        Namespace::Store,
        condition,
        path.to_path_buf(),
        format!("{message}: `{}`", path.display()),
        source,
    )
}

fn lock_io(
    condition: &'static str,
    path: &Path,
    message: &'static str,
    source: io::Error,
) -> GeneratedFlakeError {
    GeneratedFlakeError::io(
        Namespace::Lock,
        condition,
        path.to_path_buf(),
        format!("{message}: `{}`", path.display()),
        source,
    )
}

fn remove_file_if_exists(path: &Path) {
    atomic::remove_file_if_exists(path);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use super::{persist_created_lock, prepare_generated_flake};
    use crate::config::test_support::ScratchDirectory;
    use crate::config::{
        ArtifactForm, ArtifactKind, BaselineInputs, EffectiveLock, Manifest, ResolvedArtifact,
        XdgRoots,
    };
    use crate::exit::ExitKind;

    #[test]
    fn publishes_replaces_and_preserves_symlinks_without_config_writes()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        let roots = roots(scratch.path());
        let selected = selected(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            ..Manifest::default()
        };
        fs::write(roots.config.join("images/helper.nix"), "helper")?;
        symlink("helper.nix", roots.config.join("images/link.nix"))?;
        let before = fs::read(&selected.path)?;
        let first = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        fs::write(first.directory.join("old-only"), "old")?;
        let second = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        assert!(!second.directory.join("old-only").exists());
        assert_eq!(
            fs::read_link(second.directory.join("images/link.nix"))?,
            Path::new("helper.nix")
        );
        assert_eq!(fs::read(&selected.path)?, before);
        Ok(())
    }

    #[test]
    fn copy_failure_leaves_existing_tree_intact() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        let roots = roots(scratch.path());
        let selected = selected(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            ..Manifest::default()
        };
        let prepared = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        fs::write(prepared.directory.join("sentinel"), "old")?;
        // A file where a library directory belongs. An absent one would not do: that is the
        // ordinary state of a user who has written no pieces, and it copies as an empty directory.
        fs::remove_dir_all(roots.config.join("pieces"))?;
        fs::write(roots.config.join("pieces"), "not a directory")?;
        let error = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )
        .err()
        .ok_or("missing wholesale tree unexpectedly published")?;
        assert_eq!(error.exit_code(), ExitKind::IoErr);
        assert_eq!(
            fs::read_to_string(prepared.directory.join("sentinel"))?,
            "old"
        );
        Ok(())
    }

    #[test]
    fn stages_existing_lock_and_never_mutates_it() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        let roots = roots(scratch.path());
        let selected = selected(scratch.path());
        let owned = roots.data.join("projects/project/default/flake.lock");
        fs::create_dir_all(owned.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&owned, b"owned bytes")?;
        let prepared = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &Manifest {
                image: "base".to_owned(),
                ..Manifest::default()
            },
            &BaselineInputs::default(),
        )?;
        assert_eq!(
            fs::read(prepared.directory.join("flake.lock"))?,
            b"owned bytes"
        );
        assert_eq!(fs::read(&owned)?, b"owned bytes");
        assert!(persist_created_lock(&prepared).is_err());
        Ok(())
    }

    #[test]
    fn first_pin_install_coalesces_identical_and_rejects_different_winner()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        let roots = roots(scratch.path());
        let selected = selected(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            ..Manifest::default()
        };
        let first = prepare_generated_flake(
            &roots,
            "one",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        fs::write(first.directory.join("flake.lock"), b"new pin")?;
        assert!(matches!(
            persist_created_lock(&first)?,
            EffectiveLock::OwnedExisting { .. }
        ));

        let second = prepare_generated_flake(
            &roots,
            "two",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        fs::write(second.directory.join("flake.lock"), b"new pin")?;
        let winner = second.effective_lock.path().to_path_buf();
        fs::create_dir_all(winner.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&winner, b"new pin")?;
        assert!(persist_created_lock(&second).is_ok());

        let third = prepare_generated_flake(
            &roots,
            "three",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &BaselineInputs::default(),
        )?;
        fs::write(third.directory.join("flake.lock"), b"losing pin")?;
        let winner = third.effective_lock.path().to_path_buf();
        fs::create_dir_all(winner.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&winner, b"winning pin")?;
        let error = persist_created_lock(&third)
            .err()
            .ok_or("different winner unexpectedly coalesced")?;
        assert_eq!(error.exit_code(), ExitKind::TempFail);
        assert!(
            error
                .diagnostic()
                .to_string()
                .contains("lock.first-pin-race")
        );
        Ok(())
    }

    /// Leaves a representative materialized tree only when the syntax-evidence command asks.
    #[test]
    fn emits_an_opt_in_nix_syntax_fixture() -> Result<(), Box<dyn std::error::Error>> {
        let Some(cache) = std::env::var_os("VIVARIUM_ITEM3_SYNTAX_CACHE") else {
            return Ok(());
        };
        let scratch = fixture()?;
        let mut roots = roots(scratch.path());
        roots.cache = PathBuf::from(cache);
        let selected = selected(scratch.path());
        let prepared = prepare_generated_flake(
            &roots,
            "syntax-project",
            "default",
            &selected,
            "image = 'base'",
            &Manifest {
                image: "base".to_owned(),
                ..Manifest::default()
            },
            &BaselineInputs::default(),
        )?;
        assert!(prepared.directory.join("flake.nix").is_file());
        Ok(())
    }

    fn fixture() -> Result<ScratchDirectory, Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        fs::create_dir_all(scratch.path().join("config/images"))?;
        fs::create_dir_all(scratch.path().join("config/pieces"))?;
        fs::create_dir_all(scratch.path().join("config/manifests"))?;
        fs::write(scratch.path().join("config/images/base.nix"), "{}")?;
        fs::write(
            scratch.path().join("config/manifests/demo.toml"),
            "image = 'base'",
        )?;
        Ok(scratch)
    }

    fn roots(base: &Path) -> XdgRoots {
        XdgRoots {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            cache: base.join("cache"),
        }
    }

    fn selected(base: &Path) -> ResolvedArtifact {
        ResolvedArtifact {
            kind: ArtifactKind::Manifest,
            name: "demo".to_owned(),
            form: ArtifactForm::Flat,
            path: PathBuf::from(base).join("config/manifests/demo.toml"),
        }
    }
}
