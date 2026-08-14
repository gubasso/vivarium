//! Project identity: how `<project-id>` is derived, persisted, and resolved (spec/15).
//!
//! Two artifacts hold it and either can rebuild the other. The marker `<project>/.vivarium/id`
//! anchors identity inside the project tree so a move or rename keeps it; the identity index
//! `identity.toml` under the state root maps each assigned id to the live path it occupies, which
//! is what makes the smallest-free-suffix rule a global invariant rather than a local guess.
//!
//! Resolution always runs; persistence does not. Every command resolves an id — a read-only
//! `viv status` needs one to locate state just as `viv start` does — but only a minting command
//! writes either artifact, which is what keeps the read-only guarantee in spec/14 literally true.
//! [`resolve`] answers without writing and [`mint`] is the writing form.
//!
//! Unlike the registry beside it, this file is not a supported interface (ADR-0052): nothing
//! prints it and nothing invites a user to write one. So it is rendered and parsed plainly here
//! rather than walked as a spanned tree, and a key nobody published needs no position to report.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::atomic::{
    Fault, PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, create_temp_file, remove_file_if_exists,
    rename_with, sync_directory,
};
use super::error::{RegistryError, RegistryErrorKind};
use crate::diagnostic::Locus;

/// The identity index's file name, beside `registry.toml` under the state root (spec/02).
pub const IDENTITY_FILE: &str = "identity.toml";

/// The marker directory vivarium owns inside a project tree.
pub const MARKER_DIR: &str = ".vivarium";

/// The `-->` slot every identity failure reports.
const LOCUS: Locus = Locus::Named("identity index");

/// One assignment: an id and the live canonical path currently holding it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    /// The assigned `<project-id>`.
    pub id: String,
    /// The canonical, symlink-resolved project directory.
    pub path: PathBuf,
}

/// Every identity assigned on this machine.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IdentityIndex {
    identities: Vec<Identity>,
}

/// The index's shape, closed on both levels.
///
/// `deny_unknown_fields` because [`read_index`] promises `78` for a file that is not the shape
/// this module writes, and because a mint re-renders only the fields below: accepting an unknown
/// key would read state written by a newer vivarium under older semantics and then silently erase
/// it. Failing closed is spec/02's rule for every state file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIndex {
    #[serde(default)]
    identities: Vec<RawIdentity>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIdentity {
    id: String,
    path: PathBuf,
}

impl IdentityIndex {
    /// The path currently recorded for `id`.
    #[must_use]
    pub fn path_of(&self, id: &str) -> Option<&Path> {
        self.identities
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.path.as_path())
    }

    /// The id currently recorded for `path`.
    #[must_use]
    pub fn id_at(&self, path: &Path) -> Option<&str> {
        self.identities
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.id.as_str())
    }

    /// Records `id -> path`, replacing whatever either side held.
    ///
    /// Both directions are replaced, because the index is a bijection in use: an id occupies one
    /// path, and a path holds one id. Leaving a stale row on either side would make the liveness
    /// test below read a path that no longer means what the row says.
    fn assign(&mut self, id: String, path: PathBuf) {
        self.identities
            .retain(|entry| entry.id != id && entry.path != path);
        self.identities.push(Identity { id, path });
        self.identities.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// Drops the row recording `path`, returning the id it held.
    ///
    /// By path and never by id, which is the whole safety of it. An index row holding this id at a
    /// *different* path belongs to a copy that disambiguated against this project (spec/15), and
    /// removing that would take another project's identity away for a teardown it had no part in.
    fn forget(&mut self, path: &Path) -> Option<String> {
        let id = self.id_at(path)?.to_owned();
        self.identities.retain(|entry| entry.path != path);
        Some(id)
    }

    /// Whether `id` is held by a still-existing project other than `path`.
    ///
    /// Liveness is read live rather than cached, because whether the other directory still exists
    /// is exactly what distinguishes a copy from a move (spec/15), and a cached flag would answer
    /// for the moment it was written.
    fn taken_by_other(&self, id: &str, path: &Path) -> bool {
        self.path_of(id)
            .is_some_and(|held| held != path && held.exists())
    }

    /// The smallest free suffix of `base`, per spec/15: `api`, then `api-2`, `api-3`.
    ///
    /// There is deliberately no `api-1` — the first holder keeps the bare name.
    fn smallest_free(&self, base: &str, path: &Path) -> String {
        if !self.taken_by_other(base, path) {
            return base.to_owned();
        }
        // Bounded by the index's own size plus one: every iteration tests a distinct id, and a
        // set of `n` held ids cannot occupy `n + 2` candidates. So the gap is always found inside
        // the range, and the range is finite rather than merely believed to terminate.
        let ceiling = u32::try_from(self.identities.len()).unwrap_or(u32::MAX - 2) + 2;
        (2..=ceiling)
            .map(|suffix| format!("{base}-{suffix}"))
            .find(|candidate| !self.taken_by_other(candidate, path))
            .unwrap_or_else(|| base.to_owned())
    }

    fn render(&self) -> String {
        let mut rendered = String::from(concat!(
            "# vivarium's identity index (spec/15). Tool-managed; not a supported interface.\n",
            "# It records which project directory currently holds each assigned id, and nothing\n",
            "# more — no timestamps, and no cached liveness, which must be read live.\n",
        ));
        for entry in &self.identities {
            rendered.push_str("\n[[identities]]\n");
            let _ = writeln!(rendered, "id = {}", toml_string(&entry.id));
            let _ = writeln!(
                rendered,
                "path = {}",
                toml_string(&entry.path.to_string_lossy())
            );
        }
        rendered
    }
}

/// TOML's basic-string escaping, for the two values this file carries.
fn toml_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// Where the identity index and its sidecar lock live beneath a state root.
#[must_use]
pub fn identity_path(state_root: &Path) -> PathBuf {
    state_root.join(IDENTITY_FILE)
}

fn identity_lock_path(state_root: &Path) -> PathBuf {
    state_root.join(format!("{IDENTITY_FILE}.lock"))
}

/// Reads the identity index without taking the lock.
///
/// # Errors
///
/// Returns [`RegistryError`] when the file exists but cannot be read (`74`) or is not the shape
/// this module writes (`78`). An absent or empty file is the empty index; vivarium never silently
/// rebuilds a corrupt one.
pub fn read_index(state_root: &Path) -> Result<IdentityIndex, RegistryError> {
    let path = identity_path(state_root);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(IdentityIndex::default());
        }
        Err(source) => {
            return Err(RegistryError::io(
                "identity-read",
                path.clone(),
                format!("could not read the identity index `{}`", path.display()),
                source,
                true,
            ));
        }
    };
    let raw: RawIndex = toml::from_str(&source).map_err(|error| {
        RegistryError::plain(
            RegistryErrorKind::Config,
            "identity-malformed",
            Locus::File(path.clone()),
            "the identity index is not in the shape vivarium writes",
            error.message().to_owned(),
        )
    })?;
    let mut identities = Vec::with_capacity(raw.identities.len());
    for entry in raw.identities {
        // Validated on the way in rather than at each use. An id from this file reaches the state
        // and runtime paths and the transient unit name, so a row that is not a `<project-id>` is
        // a state defect to report (spec/02's `78`), not a value to carry into a path join.
        if !is_project_id(&entry.id) {
            return Err(invalid_id(
                "identity-index-invalid-id",
                Locus::File(path),
                "the identity index records an id outside the project-id grammar",
                &entry.id,
            ));
        }
        identities.push(Identity {
            id: entry.id,
            path: entry.path,
        });
    }
    Ok(IdentityIndex { identities })
}

/// Whether `value` is a `<project-id>` as spec/15 fixes it.
///
/// Stated as the grammar the sanitizer produces rather than checked by re-sanitizing, because a
/// collision suffix may push a name that was already at the 48-character cap past it — so the
/// sanitizer is not its own fixed point on every id it helps produce. The pairing is asserted by
/// test rather than left to the reader.
///
/// It exists because the two artifacts holding an id are both outside vivarium's control between
/// writes: the marker sits in the user's own tree and the index is a plain file. The value reaches
/// `roots.state`/`runtime_root` path joins and the `vivarium-<id>-<target>.service` unit name, so
/// anything carrying `/`, `..`, or a newline has to be refused here (N21).
fn is_project_id(value: &str) -> bool {
    /// spec/15 truncates a sanitized name to 48 characters.
    const CAP: usize = 48;

    // The component grammar the sanitizer produces: lowercase ASCII and digits, single interior
    // hyphens, no hyphen at either end, and non-empty.
    fn is_component(value: &str) -> bool {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !value.starts_with('-')
            && !value.ends_with('-')
            && !value.contains("--")
    }

    if !is_component(value) {
        return false;
    }
    if value.len() <= CAP {
        return true;
    }
    // Longer than the cap is admissible only the one way the minting path can produce it: a base
    // already at the cap with a collision suffix appended afterwards. So the value must split at
    // its last hyphen into a capped component and a plain decimal, which is exactly what
    // `smallest_free` writes and nothing else.
    value.rsplit_once('-').is_some_and(|(base, suffix)| {
        base.len() <= CAP
            && is_component(base)
            && !suffix.is_empty()
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// The one shape both identity artifacts report an ungrammatical id through.
fn invalid_id(
    condition: &'static str,
    locus: Locus,
    message: &'static str,
    found: &str,
) -> RegistryError {
    RegistryError::plain(
        RegistryErrorKind::Config,
        condition,
        locus,
        message,
        format!(
            "`{}` is not a project id: it must be lowercase letters, digits, and single interior \
            hyphens",
            found.escape_debug()
        ),
    )
}

/// The identity a command resolves without persisting anything.
///
/// This is the read-only half of spec/15's resolution table, and it is the whole of it for every
/// command but the minting ones. A resolved-but-unpersisted id is deterministic, not stable: two
/// copies of a project both resolve to the same suffix until one of them starts.
///
/// # Errors
///
/// Returns [`RegistryError`] when the index cannot be read.
pub fn resolve(state_root: &Path, project: &Path) -> Result<String, RegistryError> {
    let index = read_index(state_root)?;
    resolve_against(&index, project)
}

/// Spec/15's resolution table as a pure function of the index, the marker, and the path.
///
/// Split out from [`resolve`] and [`mint`] so both reach one implementation of the table, and so
/// every row of it is testable without a state root.
///
/// # Errors
///
/// Returns [`RegistryError`] when the marker cannot be read, or carries something that is not a
/// project id.
fn resolve_against(index: &IdentityIndex, project: &Path) -> Result<String, RegistryError> {
    let sanitized = sanitize_project_name(project_basename(project));
    Ok(read_marker(project)?.map_or_else(
        // No marker, but the index may know this path: that recovers an id whose marker the user
        // deleted. Otherwise this is a first assignment.
        || {
            index.id_at(project).map_or_else(
                || index.smallest_free(&sanitized, project),
                ToOwned::to_owned,
            )
        },
        // A marker keeps its id unless a live holder elsewhere proves this directory is a copy.
        // That one rule covers three of spec/15's rows, which is why they are not spelled as three
        // arms: no index entry at all (adopt the marker), an entry pointing here (the ordinary
        // in-place run), and an entry pointing at a path that no longer exists (a move or rename,
        // where identity follows the marker). Only a still-existing other holder is a copy, and a
        // copy is disambiguated rather than allowed to share the original's state.
        |marked| {
            let copied = index
                .path_of(&marked)
                .is_some_and(|held| held != project && held.exists());
            if copied {
                index.smallest_free(&sanitized, project)
            } else {
                marked
            }
        },
    ))
}

/// The identity a starting command assigns, persisting both the marker and the index entry.
///
/// The lock is taken around the read-modify-write and released before the caller builds or boots
/// anything: ADR-0053 makes "short" normative, and this is the second link of the one total lock
/// order — registry, then identity, then the per-target `flock`.
///
/// # Errors
///
/// Returns [`RegistryError`] when the lock cannot be taken promptly (`75`), or when the index or
/// the marker cannot be read or published (`74`).
pub fn mint(state_root: &Path, project: &Path) -> Result<String, RegistryError> {
    let guard = Guard::acquire(state_root)?;
    // Re-read under the lock. Reading outside it and writing inside would be exactly the race the
    // lock exists to close: two first-time starts could each see the same free suffix.
    let mut index = read_index(state_root)?;
    let id = resolve_against(&index, project)?;
    index.assign(id.clone(), project.to_path_buf());
    write_index(state_root, &index)?;
    write_marker(project, &id)?;
    drop(guard);
    Ok(id)
}

/// What a teardown found and undid, so a second run can honestly report nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Forgotten {
    /// The id whose index row was removed, when there was one.
    pub id: Option<String>,
    /// Whether the marker directory itself went away.
    pub marker_removed: bool,
}

/// Clears this project's identity: the `.vivarium/` marker and its index row (ADR-0043).
///
/// The inverse of [`mint`], and the only writer here that removes rather than assigns. spec/10
/// carves the marker out of an otherwise absolute rule — `destroy` never touches the workspace —
/// because the marker is vivarium-owned and never user-authored (N9, N21).
///
/// Marker first, index second, which is the reverse of `mint`'s order and is chosen rather than
/// incidental: both crash windows recover and neither loses an identity. Marker gone with the row
/// still present is recovered by `resolve_against` through `id_at`; row gone with the marker still
/// present readopts the marker. The reverse pairing would leave a marker naming an id the index
/// has re-issued to someone else.
///
/// # Errors
///
/// Returns [`RegistryError`] when the lock cannot be taken promptly (`75`), or when the marker or
/// the index cannot be read, removed, or published (`74`, or `77` for a permission denial).
pub fn forget(state_root: &Path, project: &Path) -> Result<Forgotten, RegistryError> {
    let guard = Guard::acquire(state_root)?;
    // Read under the lock and write under the same one, for the reason `mint` does: a read outside
    // it would lose a concurrent assignment to the classic lost update.
    let mut index = read_index(state_root)?;
    let marker_removed = remove_marker(project)?;
    let id = index.forget(project);
    // Only when something actually changed. This is what makes a second `viv destroy` touch no
    // state at all rather than rewrite the file it already agrees with.
    if id.is_some() {
        write_index(state_root, &index)?;
    }
    drop(guard);
    Ok(Forgotten { id, marker_removed })
}

/// Removes vivarium's own two files and then the directory, and nothing else.
///
/// Deliberately not `remove_dir_all`: N9 and spec/10 permit removing the marker vivarium wrote and
/// nothing beside it, so a `.vivarium/` a user has put something of their own into survives as a
/// non-empty directory. `ENOTEMPTY` is therefore an outcome rather than a failure.
fn remove_marker(project: &Path) -> Result<bool, RegistryError> {
    let directory = project.join(MARKER_DIR);
    for name in ["id", ".gitignore"] {
        let path = directory.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(RegistryError::io(
                    "marker-remove",
                    path.clone(),
                    format!("could not remove the identity marker `{}`", path.display()),
                    source,
                    true,
                ));
            }
        }
    }
    match fs::remove_dir(&directory) {
        Ok(()) => Ok(true),
        // Absent is the idempotent path; non-empty is the user's own file, kept on purpose.
        Err(source)
            if matches!(
                source.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(false)
        }
        Err(source) => Err(RegistryError::io(
            "marker-remove",
            directory.clone(),
            format!(
                "could not remove the identity marker `{}`",
                directory.display()
            ),
            source,
            true,
        )),
    }
}

/// Reads `<project>/.vivarium/id`.
///
/// Absent or empty is "no marker" — the first-run row of spec/15's table. Anything else is either
/// one clean line carrying a project id, or a defect this refuses: the file lives in the user's
/// own tree, and its value reaches state paths, runtime paths, and a systemd unit name, so a hand
/// edit must not be able to steer any of the three (N21).
fn read_marker(project: &Path) -> Result<Option<String>, RegistryError> {
    let path = project.join(MARKER_DIR).join("id");
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(RegistryError::io(
                "identity-marker-read",
                path.clone(),
                format!("could not read the identity marker `{}`", path.display()),
                source,
                true,
            ));
        }
    };
    // The exact shape spec/15 fixes is `<id>\n`, so a trailing newline is the file and anything
    // else around the id is not. Only that one terminator is forgiven; interior or leading
    // whitespace makes the file something vivarium did not write.
    let trimmed = raw.strip_suffix('\n').unwrap_or(&raw);
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !is_project_id(trimmed) {
        return Err(invalid_id(
            "identity-marker-invalid",
            Locus::File(path),
            "the identity marker does not carry a project id",
            trimmed,
        ));
    }
    Ok(Some(trimmed.to_owned()))
}

/// Writes the two-file marker spec/15 fixes, byte for byte.
///
/// The `.gitignore` of `*` is what makes the directory invisible to git with no user action, and
/// the trailing newline on `id` is part of the file's shape rather than a formatting habit.
fn write_marker(project: &Path, id: &str) -> Result<(), RegistryError> {
    let directory = project.join(MARKER_DIR);
    fs::create_dir_all(&directory).map_err(|source| {
        RegistryError::io(
            "marker-create",
            directory.clone(),
            format!(
                "could not create the identity marker `{}`",
                directory.display()
            ),
            source,
            false,
        )
    })?;
    write_marker_file(&directory, ".gitignore", "*\n")?;
    write_marker_file(&directory, "id", &format!("{id}\n"))
}

fn write_marker_file(directory: &Path, name: &str, contents: &str) -> Result<(), RegistryError> {
    let path = directory.join(name);
    fs::write(&path, contents).map_err(|source| {
        RegistryError::io(
            "marker-write",
            path.clone(),
            format!("could not write the identity marker `{}`", path.display()),
            source,
            false,
        )
    })
}

/// Publishes the index so a concurrent reader sees the old file or the new one, never both.
fn write_index(state_root: &Path, index: &IdentityIndex) -> Result<(), RegistryError> {
    ensure_state_root(state_root)?;
    let destination = identity_path(state_root);
    let bytes = index.render();

    let (temporary, mut file) = create_temp_file(state_root, IDENTITY_FILE, PRIVATE_FILE_MODE)
        .map_err(|_| {
            RegistryError::plain(
                RegistryErrorKind::Io,
                "identity-write-temporary",
                LOCUS,
                "could not stage the identity index",
                "a same-directory temporary could not be created",
            )
        })?;
    let result = (|| {
        use std::io::Write as _;
        file.write_all(bytes.as_bytes()).map_err(|source| {
            RegistryError::io(
                "identity-write",
                temporary.clone(),
                "could not write the staged identity index",
                source,
                false,
            )
        })?;
        file.sync_all().map_err(|source| {
            RegistryError::io(
                "identity-write",
                temporary.clone(),
                "could not flush the staged identity index",
                source,
                false,
            )
        })?;
        drop(file);
        rename_with(&temporary, &destination, rustix::fs::RenameFlags::empty()).map_err(
            |source| {
                RegistryError::io(
                    "identity-publish",
                    destination.clone(),
                    "could not publish the identity index",
                    source,
                    false,
                )
            },
        )?;
        // The last step of ADR-0053's publication, and the one that makes a mint durable: without
        // it a crash after the rename can lose the directory entry while the marker survives, and
        // a copied project would then re-adopt an id whose collision record no longer exists.
        sync_directory(state_root).map_err(|Fault { path, source }| {
            RegistryError::io(
                "identity-directory-sync",
                path,
                "could not flush the state root after publishing the identity index",
                source,
                false,
            )
        })
    })();
    // Every failure path, not only the two before the rename: a staged sibling left behind by a
    // failed publication is residue in a directory nothing sweeps.
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

fn ensure_state_root(state_root: &Path) -> Result<(), RegistryError> {
    fs::create_dir_all(state_root).map_err(|source| {
        RegistryError::io(
            "state-root",
            state_root.to_path_buf(),
            format!("could not create the state root `{}`", state_root.display()),
            source,
            false,
        )
    })?;
    let _ = fs::set_permissions(
        state_root,
        std::os::unix::fs::PermissionsExt::from_mode(PRIVATE_DIR_MODE),
    );
    Ok(())
}

/// The exclusive `flock` on the index's sidecar, held for exactly as long as the mint.
///
/// The second link of ADR-0053's total order. The registry lock, when a caller holds one, is taken
/// before this; the per-target `flock` is taken after.
struct Guard(File);

impl Guard {
    fn acquire(state_root: &Path) -> Result<Self, RegistryError> {
        ensure_state_root(state_root)?;
        let path = identity_lock_path(state_root);
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(PRIVATE_FILE_MODE)
            .open(&path)
            .map_err(|source| {
                RegistryError::io(
                    "identity-lock-open",
                    path.clone(),
                    format!("could not open the identity lock `{}`", path.display()),
                    source,
                    true,
                )
            })?;
        // `try_lock`, like the registry's: a contended lock is a `75` to retry rather than a hang
        // behind whatever the other process is doing.
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(fs::TryLockError::WouldBlock) => Err(RegistryError::plain(
                RegistryErrorKind::Contended,
                "identity-lock-unavailable",
                Locus::File(path),
                "another vivarium process holds the identity lock",
                "the exclusive lock could not be taken promptly; retry",
            )),
            Err(fs::TryLockError::Error(source)) => Err(RegistryError::io(
                "identity-lock-open",
                path.clone(),
                format!("could not lock the identity lock `{}`", path.display()),
                source,
                true,
            )),
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// The basename identity sanitizes, which is the directory's own name and never its parent's.
#[must_use]
pub fn project_basename(project: &Path) -> &str {
    project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
}

/// Produces the unsuffixed project id fixed by spec/15.
#[must_use]
pub fn sanitize_project_name(name: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_hyphen = false;

    for character in name.chars().flat_map(char::to_lowercase) {
        let character = if character.is_ascii_lowercase() || character.is_ascii_digit() {
            character
        } else {
            '-'
        };

        if character == '-' {
            if !sanitized.is_empty() && !previous_was_hyphen {
                sanitized.push(character);
            }
            previous_was_hyphen = true;
        } else {
            sanitized.push(character);
            previous_was_hyphen = false;
        }
    }

    while sanitized.ends_with('-') {
        sanitized.pop();
    }
    sanitized.truncate(48);
    while sanitized.ends_with('-') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        "project".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IDENTITY_FILE, IdentityIndex, MARKER_DIR, forget, identity_path, is_project_id, mint,
        read_index, read_marker, resolve, resolve_against, sanitize_project_name,
    };
    use crate::config::test_support::ScratchDirectory;
    use std::fs;
    use std::path::{Path, PathBuf};

    type Outcome = Result<(), Box<dyn std::error::Error>>;

    fn project(scratch: &ScratchDirectory, name: &str) -> Result<PathBuf, std::io::Error> {
        let path = scratch.path().join(name);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn index_of(rows: &[(&str, &Path)]) -> IdentityIndex {
        let mut index = IdentityIndex::default();
        for (id, path) in rows {
            index.assign((*id).to_owned(), (*path).to_path_buf());
        }
        index
    }

    /// Every row of spec/15's scenario table that is a function of the index and the marker.
    #[test]
    fn resolution_follows_the_scenario_table() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let api = project(&scratch, "api")?;
        let other = project(&scratch, "other")?;
        let second = project(&scratch, "collision-api")?;
        let third = project(&scratch, "third")?;
        let gone = scratch.path().join("removed");

        // First start in `~/work/api`: no marker, no index entry, so the sanitized basename.
        assert_eq!(resolve_against(&IdentityIndex::default(), &api)?, "api");
        // Re-run in place: the index already records this exact path.
        assert_eq!(resolve_against(&index_of(&[("api", &api)]), &api)?, "api");

        // A second, different project also named `api`. The first holder keeps the bare name, and
        // there is deliberately no `api-1`.
        assert_eq!(
            index_of(&[("api", &other)]).smallest_free("api", &second),
            "api-2"
        );
        assert_eq!(
            index_of(&[("api", &other), ("api-2", &second)]).smallest_free("api", &third),
            "api-3"
        );

        // A move or rename: the index points at a path that no longer exists, so identity follows
        // the marker rather than being disambiguated as a copy.
        fs::create_dir_all(api.join(MARKER_DIR))?;
        fs::write(api.join(MARKER_DIR).join("id"), "api\n")?;
        assert_eq!(resolve_against(&index_of(&[("api", &gone)]), &api)?, "api");

        // A copy: the original still exists elsewhere, so this directory takes a free suffix.
        assert_eq!(
            resolve_against(&index_of(&[("api", &other)]), &api)?,
            "api-2"
        );

        // Marker deleted: recovered from the index entry for this path.
        fs::remove_dir_all(api.join(MARKER_DIR))?;
        assert_eq!(
            resolve_against(&index_of(&[("recovered", &api)]), &api)?,
            "recovered"
        );
        Ok(())
    }

    /// Minting writes both artifacts, and resolving afterwards agrees with what it wrote.
    #[test]
    fn minting_persists_the_marker_and_the_index() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let project = project(&scratch, "API")?;
        let state = scratch.path().join("state");

        assert_eq!(mint(&state, &project)?, "api");
        assert_eq!(read_marker(&project)?.as_deref(), Some("api"));
        // The exact bytes spec/15 fixes: `*` is what makes the directory invisible to git with no
        // user action, and the trailing newline on `id` is part of the shape.
        assert_eq!(
            fs::read_to_string(project.join(MARKER_DIR).join(".gitignore"))?,
            "*\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(MARKER_DIR).join("id"))?,
            "api\n"
        );
        assert_eq!(read_index(&state)?.path_of("api"), Some(project.as_path()));

        // Idempotent: a second mint in place neither moves the id nor invents a suffix.
        assert_eq!(mint(&state, &project)?, "api");
        assert_eq!(resolve(&state, &project)?, "api");
        Ok(())
    }

    /// The collision `workflow_02_identity_collision_suffix` asserts, driven through the writer.
    #[test]
    fn a_second_project_of_the_same_name_mints_the_smallest_free_suffix() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        let first = project(&scratch, "API")?;
        let second = project(&scratch, "collision/API")?;

        assert_eq!(mint(&state, &first)?, "api");
        assert_eq!(mint(&state, &second)?, "api-2");
        // Minting is what settles a suffix, so both keep their own id on a re-run.
        assert_eq!(mint(&state, &first)?, "api");
        assert_eq!(mint(&state, &second)?, "api-2");
        Ok(())
    }

    /// A read-only resolution leaves nothing behind, which is what makes spec/14's guarantee true.
    #[test]
    fn resolving_persists_nothing() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        let project = project(&scratch, "api")?;

        assert_eq!(resolve(&state, &project)?, "api");
        assert!(!project.join(MARKER_DIR).exists());
        assert!(!identity_path(&state).exists());
        Ok(())
    }

    /// Pins the written file's text as the on-disk contract — the header, the row
    /// shape, and document order — then proves the reader accepts that literal.
    /// Only the two paths are interpolated, because the scratch root moves.
    #[test]
    fn the_index_file_carries_the_pinned_text_and_reads_back() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        let first = project(&scratch, "one")?;
        let second = project(&scratch, "two")?;

        super::write_index(&state, &index_of(&[("one", &first), ("two", &second)]))?;
        let written = std::fs::read_to_string(identity_path(&state))?;
        let pinned = format!(
            concat!(
                "# vivarium's identity index (spec/15). Tool-managed; not a supported interface.\n",
                "# It records which project directory currently holds each assigned id, and ",
                "nothing\n",
                "# more — no timestamps, and no cached liveness, which must be read live.\n",
                "\n[[identities]]\n",
                "id = \"one\"\npath = \"{}\"\n",
                "\n[[identities]]\n",
                "id = \"two\"\npath = \"{}\"\n",
            ),
            first.display(),
            second.display(),
        );
        assert_eq!(written, pinned);

        let reread = read_index(&state)?;
        assert_eq!(reread.path_of("one"), Some(first.as_path()));
        assert_eq!(reread.id_at(&second), Some("two"));
        Ok(())
    }

    /// Pins lowercase, replacement, collapse, and both-end trimming in spec/15's
    /// order, and the `project` fallback when sanitization leaves nothing usable.
    #[test]
    fn project_name_sanitization_follows_the_fixed_pipeline() {
        let rows = [
            ("API", "api"),
            ("Mixed name_value!", "mixed-name-value"),
            ("alpha___...beta", "alpha-beta"),
            ("---trim me---", "trim-me"),
            ("", "project"),
            ("___!!!", "project"),
        ];
        for (input, expected) in rows {
            assert_eq!(sanitize_project_name(input), expected, "input {input:?}");
        }
    }

    /// Pins the 48-character cap, trailing-hyphen retrim, and unsuffixed collision behavior.
    #[test]
    fn project_name_sanitization_caps_and_retrims_at_48_characters() {
        let forty_eight = "a".repeat(48);
        assert_eq!(
            sanitize_project_name(&format!("{forty_eight}extra")),
            forty_eight
        );

        let cut_on_hyphen = format!("{}-tail", "b".repeat(47));
        assert_eq!(sanitize_project_name(&cut_on_hyphen), "b".repeat(47));

        let shared_prefix = "c".repeat(48);
        assert_eq!(
            sanitize_project_name(&format!("{shared_prefix}one")),
            sanitize_project_name(&format!("{shared_prefix}two"))
        );
    }

    /// The pairing `is_project_id` claims: it accepts everything the minting path can produce.
    ///
    /// Both halves matter. Sanitizing is how a first assignment is derived, and the collision
    /// suffix is appended afterwards — so a name already at the 48-character cap mints an id
    /// longer than the cap, and a validator that re-sanitized would reject the id it just wrote.
    #[test]
    fn every_id_the_minting_path_can_produce_is_accepted() {
        let long = "a".repeat(60);
        let alternating = "b-".repeat(40);
        let names = ["api", "API", "my project", "___!!!", &long, &alternating];
        for name in names {
            let base = sanitize_project_name(name);
            assert!(is_project_id(&base), "sanitized `{base}` was rejected");
            for suffix in [2, 10, 4_294_967_295_u32] {
                let suffixed = format!("{base}-{suffix}");
                assert!(is_project_id(&suffixed), "`{suffixed}` was rejected");
            }
        }
    }

    /// What the grammar exists to keep out of a path join and a unit name (N21).
    #[test]
    fn a_value_outside_the_grammar_is_not_a_project_id() {
        let over_cap = "a".repeat(49);
        let long_unsuffixed = "a".repeat(60);
        // A long value whose tail is not the decimal suffix the collision writer appends.
        let long_noncanonical = format!("{}-x2", "a".repeat(48));
        // A long value whose base is itself past the cap, so no mint could have produced it.
        let long_overlong_base = format!("{}-2", "a".repeat(49));
        for rejected in [
            "",
            "../../escape",
            "api/../other",
            "a/b",
            "api\nrogue",
            "API",
            "-api",
            "api-",
            "api--2",
            "api 2",
            " api",
            "api ",
            &over_cap,
            &long_unsuffixed,
            &long_noncanonical,
            &long_overlong_base,
        ] {
            assert!(!is_project_id(rejected), "`{rejected}` was accepted");
        }
    }

    /// The marker is exactly `<id>\n`, so padding is a file vivarium did not write.
    #[test]
    fn a_marker_carrying_more_than_one_clean_line_is_refused() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let project = project(&scratch, "api")?;
        fs::create_dir_all(project.join(MARKER_DIR))?;
        for contents in ["  api\n", "api\nextra\n", "api \n", "\napi\n"] {
            fs::write(project.join(MARKER_DIR).join("id"), contents)?;
            assert!(
                read_marker(&project).is_err(),
                "`{}` was accepted as a marker",
                contents.escape_debug()
            );
        }
        // The one shape it does write round-trips.
        fs::write(project.join(MARKER_DIR).join("id"), "api\n")?;
        assert_eq!(read_marker(&project)?.as_deref(), Some("api"));
        Ok(())
    }

    /// An index carrying a key this module never writes fails closed rather than losing it.
    #[test]
    fn an_index_with_an_unknown_key_is_refused() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        fs::create_dir_all(&state)?;
        for contents in [
            "schema = 2\n",
            "[[identities]]\nid = \"api\"\npath = \"/home/alice/api\"\nminted = \"today\"\n",
        ] {
            fs::write(identity_path(&state), contents)?;
            let code = read_index(&state)
                .err()
                .map(|error| error.exit_code().code());
            assert_eq!(code, Some(78), "`{contents}` was accepted");
        }
        Ok(())
    }

    /// A hand-edited marker is refused rather than carried into the paths it would steer.
    #[test]
    fn a_marker_outside_the_grammar_is_refused() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let project = project(&scratch, "api")?;
        fs::create_dir_all(project.join(MARKER_DIR))?;
        fs::write(project.join(MARKER_DIR).join("id"), "../../escape\n")?;

        let code = read_marker(&project)
            .err()
            .map(|error| error.exit_code().code());
        assert_eq!(code, Some(78), "a traversal marker must not resolve");

        // And the whole resolution refuses with it, rather than falling back to the basename.
        assert!(resolve(&scratch.path().join("state"), &project).is_err());
        Ok(())
    }

    /// A staged index that cannot be published leaves no sibling behind, and a published one is
    /// durable — the two halves of ADR-0053's publication that `write_index` is responsible for.
    #[test]
    fn publishing_the_index_leaves_no_staged_sibling() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let first = project(&scratch, "one")?;
        let state = scratch.path().join("state");

        super::write_index(&state, &index_of(&[("one", &first)]))?;
        let strays: Vec<_> = fs::read_dir(&state)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != IDENTITY_FILE)
            .collect();
        assert!(strays.is_empty(), "staged siblings survived: {strays:?}");
        Ok(())
    }

    /// An index row outside the grammar is a state defect, not a value to carry into a path.
    #[test]
    fn an_index_row_outside_the_grammar_is_refused() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        fs::create_dir_all(&state)?;
        fs::write(
            identity_path(&state),
            "[[identities]]\nid = \"../escape\"\npath = \"/home/alice/api\"\n",
        )?;

        let code = read_index(&state)
            .err()
            .map(|error| error.exit_code().code());
        assert_eq!(code, Some(78), "a traversal id must not load");
        Ok(())
    }

    /// The teardown half of the marker lifecycle (ADR-0043).
    #[test]
    fn forgetting_removes_this_project_and_leaves_every_other_alone() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let api = project(&scratch, "api")?;
        let copy = project(&scratch, "copy")?;
        let state = scratch.path().join("state");

        let api_id = mint(&state, &api)?;
        let copy_id = mint(&state, &copy)?;
        assert_ne!(api_id, copy_id, "a copy must disambiguate");

        let forgotten = forget(&state, &api)?;
        assert_eq!(forgotten.id.as_deref(), Some(api_id.as_str()));
        assert!(forgotten.marker_removed);
        assert!(!api.join(MARKER_DIR).exists());
        // Removal is by path, never by id. The copy holds a different id at a different path and
        // must survive untouched — taking a row by id would erase whichever project matched.
        assert_eq!(read_marker(&copy)?.as_deref(), Some(copy_id.as_str()));
        assert_eq!(read_index(&state)?.id_at(&copy), Some(copy_id.as_str()));
        assert_eq!(read_index(&state)?.id_at(&api), None);
        Ok(())
    }

    /// The idempotent path: a second `destroy` finds nothing and writes nothing.
    #[test]
    fn forgetting_what_is_already_gone_touches_no_state() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let api = project(&scratch, "api")?;
        let state = scratch.path().join("state");
        mint(&state, &api)?;
        forget(&state, &api)?;

        let before = fs::read_to_string(identity_path(&state))?;
        let forgotten = forget(&state, &api)?;
        assert_eq!(forgotten.id, None);
        assert!(!forgotten.marker_removed);
        // Byte-identical, which is the observable form of "wrote nothing": a rewrite would
        // republish the same content and be indistinguishable from this without reading the file.
        assert_eq!(fs::read_to_string(identity_path(&state))?, before);
        Ok(())
    }

    /// N9 and spec/10: vivarium removes the two files it wrote, and nothing a user put beside them.
    #[test]
    fn a_marker_directory_holding_a_users_own_file_survives_as_a_directory() -> Outcome {
        let scratch = ScratchDirectory::new()?;
        let api = project(&scratch, "api")?;
        let state = scratch.path().join("state");
        mint(&state, &api)?;
        let mine = api.join(MARKER_DIR).join("notes.txt");
        fs::write(&mine, "mine\n")?;

        let forgotten = forget(&state, &api)?;
        assert!(!forgotten.marker_removed, "a non-empty directory is kept");
        assert!(mine.exists(), "the user's own file must survive");
        assert!(!api.join(MARKER_DIR).join("id").exists());
        assert!(!api.join(MARKER_DIR).join(".gitignore").exists());
        // The identity is still cleared, which is what `destroy` promises: the leftover directory
        // is the user's, and the next `start` re-mints into it.
        assert_eq!(read_index(&state)?.id_at(&api), None);
        Ok(())
    }
}
