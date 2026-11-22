//! Atomic filesystem publication for generated flakes, and the pre-flight that writes a first pin.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use rustix::fs::RenameFlags;

use super::atomic::{self, Fault, StageFault};
use super::base::ImageBase;
use super::error::GeneratedFlakeErrorKind;
use super::flake::GeneratedEntry;
use super::{
    EffectiveLock, GeneratedFlakeError, GeneratedFlakePlan, Manifest, PreparedFlake,
    ResolvedArtifact, ResolvedComposition, XdgRoots,
};
use crate::diagnostic::{Locus, Namespace};
use crate::ui::{Ui, watch};

/// The mode a staged generated file takes. Unlike the state root's `0600`, a generated tree is
/// ordinary cache a build reads, so it keeps the umask's default rather than being made private.
const STAGED_FILE_MODE: u32 = 0o666;

/// Resolves, plans, and atomically publishes one generated flake, with a lock in force.
///
/// When the target has no lock, the pre-flight writes the first one after publication and
/// before anything evaluates (ADR-0116).
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when read-only resolution fails, the complete replacement
/// cannot be prepared, flushed, published, or cleaned up safely, or the pre-flight fails.
// Eight distinct nouns, none of which groups with another: the roots, the two path components
// the target is keyed by, the selected artifact, its text, its parse, its resolved workspaces, and
// the face the pre-flight speaks through. A struct here would be a bag named for this call rather
// than for anything.
#[allow(clippy::too_many_arguments)]
pub fn prepare_generated_flake(
    roots: &XdgRoots,
    sandbox_id: &str,
    target: &str,
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    manifest: &Manifest,
    // Expanded and canonical, resolved by the caller so the owner index and the build read one
    // derivation of the set (ADR-0110).
    workspaces: &[PathBuf],
    ui: &Ui,
) -> Result<PreparedFlake, GeneratedFlakeError> {
    let composition = ResolvedComposition::resolve(
        roots,
        sandbox_id,
        target,
        selected_manifest,
        manifest_source,
        manifest,
    )?;
    // The probe lives here, not in resolution: `resolve` stays pure path-and-text work,
    // and this seam is the first one every caller of it shares that already runs Nix soon
    // after (ADR-0112). Binding-only readers never reach it.
    let base = super::base::probe_selected_image(&composition.image)?;
    if let Some(base) = &base {
        super::base::validate_base_collisions(base, &composition.inputs)?;
    }
    let plan = GeneratedFlakePlan::build(
        roots,
        selected_manifest,
        manifest_source,
        manifest,
        workspaces,
        &composition,
        base.as_ref(),
    )?;
    publish(&plan)?;
    let effective_lock = match &plan.effective_lock {
        Some(lock) => lock.clone(),
        None => write_shipped_pin(&plan.directory, &composition.paths.owned_lock, ui)?,
    };
    Ok(PreparedFlake {
        directory: plan.directory,
        effective_lock,
        shed_vivarium: plan.migrated_lock.is_some(),
        base: plan.base,
    })
}

/// The pre-flight: writes the pin the binary ships as the target's first lock (ADR-0116).
///
/// Nix's lock step runs on the generated tree with the shipped lock as its reference, so every
/// node the reference holds is kept as it is and only an input it lacks is resolved. The result
/// meets the composed check, is installed with no-replace semantics, and is copied into the tree,
/// so the command continues exactly as for an existing owned lock. The temporary reference and
/// the staged result are removed on every path, so nothing but the lock sits beside it.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when Nix cannot run or fails, the result breaks the composed
/// rule, the owned lock cannot be installed, or a different concurrent first pin won.
fn write_shipped_pin(
    directory: &Path,
    owned_path: &Path,
    ui: &Ui,
) -> Result<EffectiveLock, GeneratedFlakeError> {
    let parent = lock_parent(owned_path)?;
    fs::create_dir_all(parent).map_err(|source| {
        GeneratedFlakeError::io(
            Namespace::Lock,
            "persist-parent",
            parent.to_path_buf(),
            format!("could not create lock directory `{}`", parent.display()),
            source,
        )
    })?;
    let (reference, mut file) = create_temp_file(parent, "flake.lock.reference")?;
    let staged = atomic::unique_path(parent, "flake.lock", "preflight");
    let result = (|| {
        file.write_all(super::shipped_pin::LOCK).map_err(|source| {
            lock_io(
                "persist-write",
                &reference,
                "could not write the shipped pin",
                source,
            )
        })?;
        drop(file);
        lock_against_shipped_pin(directory, &reference, &staged, ui)?;
        // Nix writes no output when the result equals the reference, so the reference is then
        // the result.
        if !staged.exists() {
            fs::rename(&reference, &staged).map_err(|source| {
                lock_io(
                    "persist-write",
                    &staged,
                    "could not stage the shipped pin",
                    source,
                )
            })?;
        }
        let bytes = fs::read(&staged).map_err(|source| {
            lock_io(
                "generated-read",
                &staged,
                "could not read the lock the pre-flight staged",
                source,
            )
        })?;
        // The composed check, before anything durable exists: a defect refuses the install and
        // leaves no owned lock behind, so the remedy (fix the base, re-run) starts clean.
        composed_lock_failure(&bytes)?;
        if install_first_pin(owned_path, &staged, &bytes)? {
            // Announced the moment the lock is durable, so no later failure can suppress it:
            // spec/04 requires the created pin reported, and a read-only command that quietly
            // wrote one would be the surprise ADR-0011 exists to prevent.
            ui.warn(&format!(
                "created the pin for this target: {}",
                owned_path.display()
            ));
        }
        stage_lock_into_tree(directory, &bytes)?;
        Ok(EffectiveLock::OwnedExisting {
            path: owned_path.to_path_buf(),
        })
    })();
    remove_file_if_exists(&reference);
    remove_file_if_exists(&staged);
    result
}

/// Runs `nix flake lock` on the generated tree with the shipped pin as the reference lock.
fn lock_against_shipped_pin(
    directory: &Path,
    reference: &Path,
    staged: &Path,
    ui: &Ui,
) -> Result<(), GeneratedFlakeError> {
    let mut command = Command::new("nix");
    command.args(["flake", "lock"]);
    command.args(super::evaluate::FEATURE_FLAGS);
    command.arg(directory);
    command.arg("--reference-lock-file");
    command.arg(reference);
    command.arg("--output-lock-file");
    command.arg(staged);
    let step = ui.step("pinning this target's inputs");
    let output = watch::output(&mut command, &step).map_err(|source| {
        // The three host answers a failed evaluation gives, so the pre-flight that now runs
        // Nix first tells a user the same thing about the same host.
        let (kind, condition, message) = match source.kind() {
            io::ErrorKind::NotFound => (
                GeneratedFlakeErrorKind::Unavailable,
                "nix-missing",
                "nix is not available on this host",
            ),
            io::ErrorKind::PermissionDenied => (
                GeneratedFlakeErrorKind::Permission,
                "nix-not-permitted",
                "not permitted to run nix",
            ),
            _ => (
                GeneratedFlakeErrorKind::Unavailable,
                "nix-unusable",
                "could not run nix",
            ),
        };
        GeneratedFlakeError::plain(
            kind,
            Namespace::Host,
            condition,
            Locus::Named("host"),
            message,
            source.to_string(),
        )
    })?;
    if !output.status.success() {
        drop(step);
        return Err(GeneratedFlakeError::nix_failed(
            Namespace::Lock,
            "preflight-failed",
            Locus::Named("pre-flight"),
            "the pre-flight could not write this target's first lock",
            &output.stderr,
        ));
    }
    step.done("pinned this target's inputs");
    Ok(())
}

/// Installs the staged first pin with no-replace semantics, consuming the staged file.
///
/// Returns whether this process installed it. A concurrent winner with identical bytes
/// coalesces into the same lock; one with different bytes is the `75` race.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when the staged pin cannot be flushed or installed, the
/// winner cannot be read, or a different concurrent first pin won.
fn install_first_pin(
    owned_path: &Path,
    staged: &Path,
    bytes: &[u8],
) -> Result<bool, GeneratedFlakeError> {
    let parent = lock_parent(owned_path)?;
    // Nix wrote the bytes; make them durable before the rename publishes them.
    File::open(staged)
        .and_then(|file| file.sync_all())
        .map_err(|source| {
            lock_io(
                "persist-sync",
                staged,
                "could not flush staged lock",
                source,
            )
        })?;
    match rename_with(staged, owned_path, RenameFlags::NOREPLACE) {
        Ok(()) => {
            sync_directory(parent, Namespace::Lock, "persist-directory-sync")?;
            Ok(true)
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
            if winner == bytes {
                Ok(false)
            } else {
                Err(GeneratedFlakeError::plain(
                    GeneratedFlakeErrorKind::Race,
                    Namespace::Lock,
                    "first-pin-race",
                    Locus::File(owned_path.to_path_buf()),
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
}

/// Copies the installed first pin into the generated tree, replacing nothing partially.
fn stage_lock_into_tree(directory: &Path, bytes: &[u8]) -> Result<(), GeneratedFlakeError> {
    let destination = directory.join("flake.lock");
    let (temporary, mut file) = atomic::create_temp_file(directory, "flake.lock", STAGED_FILE_MODE)
        .map_err(|fault| {
            staged(
                fault,
                Namespace::Store,
                "write-file",
                "temporary-collision",
                "could not stage the lock into the generated tree",
                "could not allocate a unique sibling in the generated tree",
            )
        })?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temporary, &destination))
        .map_err(|source| {
            store_io(
                "write-file",
                &destination,
                "could not stage the lock into the generated tree",
                source,
            )
        });
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

/// The directory the owned lock lives in.
fn lock_parent(owned_path: &Path) -> Result<&Path, GeneratedFlakeError> {
    owned_path.parent().ok_or_else(|| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "lock-parent",
            Locus::File(owned_path.to_path_buf()),
            "owned lock has no parent directory",
            "the lock path escaped its fixed data-root layout",
        )
    })
}

/// The read-only half of an update's resolution: the declared input names and the base,
/// with nothing staged and nothing locked.
///
/// Separate from [`prepare_update_tree`] because the unknown-name refusal is a usage answer
/// (`64`) the command owes before it takes the per-target lock or writes anything at all —
/// a contended lock must not turn a typo into `75`, and a tree must not be rendered for an
/// invocation that was never well-formed.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] for the same resolution, probe, and collision failures
/// as [`prepare_update_tree`].
pub fn resolve_update_inputs(
    roots: &XdgRoots,
    sandbox_id: &str,
    target: &str,
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    manifest: &Manifest,
) -> Result<(Vec<String>, Option<ImageBase>), GeneratedFlakeError> {
    let composition = ResolvedComposition::resolve(
        roots,
        sandbox_id,
        target,
        selected_manifest,
        manifest_source,
        manifest,
    )?;
    let base = super::base::probe_selected_image(&composition.image)?;
    if let Some(base) = &base {
        super::base::validate_base_collisions(base, &composition.inputs)?;
    }
    Ok((composition.inputs.keys().cloned().collect(), base))
}

/// The private tree `viv update` runs Nix against, removed when dropped.
///
/// Private rather than the shared published cache, because unlocked readers — `viv config
/// eval` and `config sources` — republish that tree with `RENAME_EXCHANGE` at any moment,
/// and Nix must not read a tree that changes under it. The per-target lock serializes an
/// update against `start` and against another update; it cannot serialize those readers,
/// so the tree they exchange is simply not the one used here.
pub struct UpdatePreparation {
    /// The rendered private tree.
    pub directory: PathBuf,
    /// The lock staged inside the tree — the reference an update re-resolves from. These are
    /// the shed bytes when the ADR-0102 migration applied, so an update never writes the dead
    /// node back, and the pre-flight's when the target had no lock.
    pub staged_reference: PathBuf,
    /// The lock in force for this target.
    pub effective_lock: EffectiveLock,
    /// The selected image's base flake, when it carries one (ADR-0112).
    pub base: Option<ImageBase>,
    /// The artifact-declared input names, for the caller's known-set validation.
    pub declared: Vec<String>,
}

impl Drop for UpdatePreparation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

/// Resolves and renders the private tree an update runs against.
///
/// The one durable write is the pre-flight's, when the target has no lock yet (ADR-0116): the
/// update then moves what it names from the pin the binary ships.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] for the same resolution, rendering, and pre-flight failures
/// as [`prepare_generated_flake`], for a team override lock in force (the caller refuses that
/// case first and this is the backstop), or for a tree that cannot be written.
#[allow(clippy::too_many_arguments)]
pub fn prepare_update_tree(
    roots: &XdgRoots,
    sandbox_id: &str,
    target: &str,
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    manifest: &Manifest,
    workspaces: &[PathBuf],
    ui: &Ui,
) -> Result<UpdatePreparation, GeneratedFlakeError> {
    let composition = ResolvedComposition::resolve(
        roots,
        sandbox_id,
        target,
        selected_manifest,
        manifest_source,
        manifest,
    )?;
    if let Some(EffectiveLock::Override { path }) = &composition.effective_lock {
        // The caller refuses this case before preparing anything — but the config root is
        // deliberately unlocked, so a team override can appear between that check and this
        // resolution. A user condition either way, so it takes the same public refusal
        // (`78`, both files named) rather than reading as an internal fault (ADR-0062).
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Lock,
            "override-in-force",
            Locus::File(path.clone()),
            "a team override lock is in force",
            format!(
                "`{}` is read-only to the tool and shadows `{}`, which this update would \
                have written; moving a shared pin is the team's own act, outside vivarium",
                path.display(),
                composition.paths.owned_lock.display()
            ),
        ));
    }
    let base = super::base::probe_selected_image(&composition.image)?;
    if let Some(base) = &base {
        super::base::validate_base_collisions(base, &composition.inputs)?;
    }
    let declared = composition.inputs.keys().cloned().collect();
    let plan = GeneratedFlakePlan::build(
        roots,
        selected_manifest,
        manifest_source,
        manifest,
        workspaces,
        &composition,
        base.as_ref(),
    )?;
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
    for entry in &plan.entries {
        if let Err(error) = apply_entry(&temporary, entry) {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }
    }
    let effective_lock = match plan.effective_lock {
        Some(lock) => lock,
        None => {
            write_shipped_pin(&temporary, &composition.paths.owned_lock, ui).inspect_err(|_| {
                let _ = fs::remove_dir_all(&temporary);
            })?
        }
    };
    Ok(UpdatePreparation {
        staged_reference: temporary.join("flake.lock"),
        directory: temporary,
        effective_lock,
        base: plan.base,
        declared,
    })
}

/// Lifts a composed-lock defect into the refusal every lock path shares.
///
/// The check itself is permissive by design — unparsable bytes pass, because lock validity
/// is Nix's to name — so a refusal here is exactly the split or gap nothing downstream
/// would report better than as a guest that fails to boot.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] at `78` when the composed lock resolves no baseline or
/// resolves the two `nixpkgs` readers to different trees.
pub fn composed_lock_failure(bytes: &[u8]) -> Result<(), GeneratedFlakeError> {
    match super::lock::validate_composed(bytes) {
        Ok(()) => Ok(()),
        Err(super::lock::ComposedLockDefect::BaselineMissing { name }) => {
            Err(GeneratedFlakeError::plain(
                GeneratedFlakeErrorKind::Config,
                Namespace::Lock,
                "baseline-missing",
                Locus::Named("composed lock"),
                format!("the composed lock resolves no `{name}` for the generated flake"),
                "every generated flake builds from a `nixpkgs` and a `microvm`; the base \
                flake redirecting one must actually declare it",
            ))
        }
        Err(super::lock::ComposedLockDefect::BaselineSplit { root, microvm }) => {
            Err(GeneratedFlakeError::plain(
                GeneratedFlakeErrorKind::Config,
                Namespace::Lock,
                "baseline-split",
                Locus::Named("composed lock"),
                format!(
                    "the guest and `microvm` would build from two different `nixpkgs` trees \
                    (`{root}` and `{microvm}`)",
                ),
                "a split surfaces as a boot failure rather than an evaluation error; a base \
                flake declaring `microvm` also carries \
                `microvm.inputs.nixpkgs.follows = \"nixpkgs\"` so both resolve to one tree",
            ))
        }
    }
}

/// Replaces the owned lock with an update's validated candidate (ADR-0059: only `viv update`
/// moves a pin, and this is that move).
///
/// The candidate is the staged file Nix wrote via `--output-lock-file`, already beside the
/// owned lock. Separate from the pre-flight's install because that path's `RENAME_NOREPLACE`
/// is the first-pin contract and must not be relaxed into a flag; an update replaces by
/// definition. The caller validates the candidate before this move and removes the staged
/// file on every earlier failure; this function consumes it on success.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when the staged candidate cannot be flushed, renamed over
/// the owned lock, or the directory entry made durable.
pub fn persist_updated_lock(owned_path: &Path, staged: &Path) -> Result<(), GeneratedFlakeError> {
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
    // Nix wrote the bytes; make them durable before the rename publishes them.
    let file = File::open(staged)
        .map_err(|source| lock_io("update-open", staged, "could not open staged lock", source))?;
    file.sync_all()
        .map_err(|source| lock_io("update-sync", staged, "could not flush staged lock", source))?;
    drop(file);
    rename_with(staged, owned_path, RenameFlags::empty()).map_err(|source| {
        lock_io(
            "update-publish",
            owned_path,
            "could not atomically install updated pin",
            source,
        )
    })?;
    sync_directory(parent, Namespace::Lock, "update-directory-sync")
}

fn publish(plan: &GeneratedFlakePlan) -> Result<(), GeneratedFlakeError> {
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
        if let (Some(bytes), Some(lock)) = (&plan.migrated_lock, &plan.effective_lock) {
            rewrite_owned_lock(lock.path(), bytes)?;
        }
        Ok(())
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
/// lets the derived index reuse the same steps without inheriting `store.` and `lock.` ids.
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

    use super::{install_first_pin, prepare_generated_flake};
    use crate::config::test_support::ScratchDirectory;
    use crate::config::{ArtifactForm, ArtifactKind, Manifest, ResolvedArtifact, XdgRoots};
    use crate::exit::ExitKind;
    use crate::ui::Ui;

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
        seed_lock(&roots, "project")?;
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
            &[],
            &Ui::silent(),
        )?;
        fs::write(first.directory.join("old-only"), "old")?;
        let second = prepare_generated_flake(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
            &[],
            &Ui::silent(),
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
        seed_lock(&roots, "project")?;
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
            &[],
            &Ui::silent(),
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
            &[],
            &Ui::silent(),
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
            &[],
            &Ui::silent(),
        )?;
        assert_eq!(
            fs::read(prepared.directory.join("flake.lock"))?,
            b"owned bytes"
        );
        assert_eq!(fs::read(&owned)?, b"owned bytes");
        Ok(())
    }

    /// The pre-flight's install: a fresh pin installs and consumes its staged file, an identical
    /// concurrent winner coalesces, and a different one is the `75` race that leaves the winner.
    #[test]
    fn first_pin_install_coalesces_identical_and_rejects_different_winner()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        let parent = scratch.path().join("data/projects/one/default");
        fs::create_dir_all(&parent)?;
        let owned = parent.join("flake.lock");

        let staged = parent.join(".first.preflight");
        fs::write(&staged, b"new pin")?;
        assert!(install_first_pin(&owned, &staged, b"new pin")?);
        assert!(!staged.exists());
        assert_eq!(fs::read(&owned)?, b"new pin");

        let identical = parent.join(".identical.preflight");
        fs::write(&identical, b"new pin")?;
        assert!(!install_first_pin(&owned, &identical, b"new pin")?);
        assert_eq!(fs::read(&owned)?, b"new pin");

        let losing = parent.join(".losing.preflight");
        fs::write(&losing, b"losing pin")?;
        let error = install_first_pin(&owned, &losing, b"losing pin")
            .err()
            .ok_or("different winner unexpectedly coalesced")?;
        assert_eq!(error.exit_code(), ExitKind::TempFail);
        assert!(
            error
                .diagnostic()
                .to_string()
                .contains("lock.first-pin-race")
        );
        assert_eq!(fs::read(&owned)?, b"new pin");
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
        seed_lock(&roots, "syntax-project")?;
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
            &[],
            &Ui::silent(),
        )?;
        assert!(prepared.directory.join("flake.nix").is_file());
        Ok(())
    }

    /// An owned lock for `sandbox`, so a preparation publishes without running the pre-flight.
    fn seed_lock(roots: &XdgRoots, sandbox: &str) -> Result<(), Box<dyn std::error::Error>> {
        let owned = roots
            .data
            .join("projects")
            .join(sandbox)
            .join("default/flake.lock");
        fs::create_dir_all(owned.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(owned, crate::config::shipped_pin::LOCK)?;
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
