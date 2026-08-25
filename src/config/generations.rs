//! The per-project profile that keeps a retained build reachable from a garbage-collector root.
//!
//! spec/11 fixes the layout under the state root at `projects/<sandbox-id>/<target>/`: numbered
//! symlinks under `generations/`, a `current` pointer, and per-generation metadata beside them.
//! ADR-0014 chose this shape over bare `result` links and over an index with no roots, and
//! ADR-0059 amended it so each generation retains the whole lockfile it was built against rather
//! than a revision from it.
//!
//! What makes `generations/<n>` a root is registration Nix follows, not the symlink itself — the
//! one outcome slice 032 forbids is a link that looks like a root and is not. Registration is a
//! `nix-store` invocation, so this module takes it as a closure: the state layer stays testable
//! without a store, and the caller owns the subprocess the way it owns the build itself.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use rustix::fs::RenameFlags;
use serde::{Deserialize, Serialize};

use super::atomic::{PRIVATE_FILE_MODE, StageFault, create_temp_file, rename_with, sync_directory};
use super::error::{RegistryError, RegistryErrorKind};
use crate::diagnostic::Locus;

/// What one generation's `metadata/<n>.json` records, exactly the fields spec/11 enumerates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GenerationRecord {
    /// The build output the generation's symlink pins.
    pub store_path: String,
    /// Content digest of the retained lock snapshot — what makes two generations comparable.
    pub lock_digest: String,
    /// The sandbox key the profile is filed under (ADR-0107).
    pub manifest: String,
    /// The backend the build targets.
    pub backend: String,
    /// When the build completed, RFC 3339 UTC.
    pub built_at: String,
}

/// One retained generation, read leniently: a crash between append steps must still list.
///
/// `store_path` comes from the symlink and `record` from the metadata file; either can be absent
/// without taking the other rows down, because `generations list` answers `78` and nothing else
/// (spec/14) — a row it cannot fill is rendered incomplete, never minted into a failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Generation {
    pub number: u64,
    pub store_path: Option<String>,
    pub record: Option<GenerationRecord>,
    pub current: bool,
}

/// Where one project target's profile lives, and nothing about what is in it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationPaths {
    root: PathBuf,
}

impl GenerationPaths {
    #[must_use]
    pub fn new(state_root: &Path, sandbox_id: &str, target: &str) -> Self {
        Self {
            root: state_root.join("projects").join(sandbox_id).join(target),
        }
    }

    /// The `current` pointer, a relative symlink to `generations/<n>`.
    #[must_use]
    pub fn current(&self) -> PathBuf {
        self.root.join(CURRENT)
    }

    /// The directory of numbered root symlinks.
    #[must_use]
    pub fn generations(&self) -> PathBuf {
        self.root.join(GENERATIONS_DIR)
    }

    /// The directory of per-generation records and lock snapshots.
    #[must_use]
    pub fn metadata_dir(&self) -> PathBuf {
        self.root.join(METADATA_DIR)
    }

    /// One generation's root symlink.
    #[must_use]
    pub fn link(&self, number: u64) -> PathBuf {
        self.generations().join(number.to_string())
    }

    /// One generation's metadata record.
    #[must_use]
    pub fn metadata(&self, number: u64) -> PathBuf {
        self.metadata_dir().join(format!("{number}.json"))
    }

    /// One generation's retained lockfile.
    #[must_use]
    pub fn lock_snapshot(&self, number: u64) -> PathBuf {
        self.metadata_dir().join(format!("{number}.lock"))
    }
}

const CURRENT: &str = "current";
const GENERATIONS_DIR: &str = "generations";
const METADATA_DIR: &str = "metadata";

/// The next generation number: one past the highest ever used, scanning links and metadata both.
///
/// Both directories rather than one, because a crash can leave either half alone and spec/11
/// forbids reusing a number — a gap is legal, a collision is a rewrite of history.
#[must_use]
pub fn next_number(paths: &GenerationPaths) -> u64 {
    numbers(paths).last().map_or(1, |highest| highest + 1)
}

/// Every generation, oldest first, read as leniently as [`Generation`] documents.
#[must_use]
pub fn list(paths: &GenerationPaths) -> Vec<Generation> {
    let current = current_number(paths);
    numbers(paths)
        .into_iter()
        .map(|number| Generation {
            number,
            store_path: fs::read_link(paths.link(number))
                .ok()
                .map(|target| target.to_string_lossy().into_owned()),
            record: fs::read(paths.metadata(number))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
            current: current == Some(number),
        })
        .collect()
}

/// The generation `current` points at, if the pointer exists and parses.
#[must_use]
pub fn current_number(paths: &GenerationPaths) -> Option<u64> {
    let target = fs::read_link(paths.current()).ok()?;
    let name = target.file_name()?.to_str()?;
    name.parse().ok()
}

/// The store path of the current generation, and only if that output still exists.
///
/// The existence check carries the same contract the old `last-build` reader had: a recorded
/// build whose output has been collected is not a build `--no-rebuild` could boot, and reporting
/// `built` for one would be a state that cannot be acted on.
#[must_use]
pub fn current_store_path(paths: &GenerationPaths) -> Option<String> {
    store_path(paths, current_number(paths)?)
}

/// One generation's store path, under the same collected-output guard.
#[must_use]
pub fn store_path(paths: &GenerationPaths, number: u64) -> Option<String> {
    let target = fs::read_link(paths.link(number)).ok()?;
    target
        .exists()
        .then(|| target.to_string_lossy().into_owned())
}

/// Appends one generation: lock snapshot, its digest, metadata, registered root, then `current`.
///
/// The root symlink is created by `register_root` at its final path — no stage-and-rename,
/// because the registration Nix records names that exact pathname and a rename would orphan it.
/// The metadata lands before the root so a failure between the two leaves a listable gap rather
/// than an unlabeled root; a registration failure removes the metadata again and reports.
///
/// `lock` arrives as bytes rather than a path, because the caller captured the lock in force at
/// evaluation time and a path re-read here could name something newer. `digest_lock` runs
/// against the published snapshot: the digest's contract is "the content digest of that
/// snapshot" (spec/11), and a digest taken in a separate read of anything else could disagree
/// with the bytes actually retained. The seed record's own `lock_digest` is ignored and
/// replaced.
///
/// # Errors
///
/// Returns [`RegistryError`] when a directory or record cannot be written, when the lock snapshot
/// cannot be published or digested, or when `register_root` reports the root was not registered.
pub fn append(
    paths: &GenerationPaths,
    record: &GenerationRecord,
    lock: &[u8],
    digest_lock: impl FnOnce(&Path) -> Result<String, String>,
    register_root: impl FnOnce(&Path, &str) -> Result<(), String>,
) -> Result<u64, RegistryError> {
    for directory in [paths.generations(), paths.metadata_dir()] {
        fs::create_dir_all(&directory).map_err(|source| {
            RegistryError::io(
                "generation-directory",
                directory.clone(),
                format!(
                    "could not create the generation directory `{}`",
                    directory.display()
                ),
                source,
                true,
            )
        })?;
    }
    let number = next_number(paths);

    publish(paths, &paths.lock_snapshot(number), lock)?;
    let lock_digest = digest_lock(&paths.lock_snapshot(number)).map_err(|why| {
        let _ = fs::remove_file(paths.lock_snapshot(number));
        RegistryError::plain(
            RegistryErrorKind::Io,
            "generation-digest",
            Locus::File(paths.lock_snapshot(number)),
            "could not digest the retained lock snapshot",
            why,
        )
    })?;
    let record = GenerationRecord {
        lock_digest,
        ..record.clone()
    };
    let rendered = serde_json::to_vec_pretty(&record).map_err(|source| {
        RegistryError::plain(
            RegistryErrorKind::Io,
            "generation-record",
            Locus::File(paths.metadata(number)),
            "could not render the generation record",
            source.to_string(),
        )
    })?;
    publish(paths, &paths.metadata(number), &rendered)?;

    let link = paths.link(number);
    if let Err(why) = register_root(&link, &record.store_path) {
        // A record without a root would be exactly the defect this profile exists to remove, so
        // the half that landed is taken back; the number stays burned either way.
        let _ = fs::remove_file(paths.metadata(number));
        let _ = fs::remove_file(paths.lock_snapshot(number));
        return Err(RegistryError::plain(
            RegistryErrorKind::Io,
            "generation-root",
            Locus::File(link),
            "could not register the build as a garbage-collector root",
            why,
        ));
    }

    set_current(paths, number)?;
    Ok(number)
}

/// Repoints `current` at a retained generation, staged so no reader sees a missing pointer.
///
/// Whether `number` names a retained generation is the caller's question — spec/14 makes an
/// unknown generation a usage error on `activate`, which this layer has no vocabulary for.
///
/// # Errors
///
/// Returns [`RegistryError`] when the staged symlink cannot be created or published.
pub fn set_current(paths: &GenerationPaths, number: u64) -> Result<(), RegistryError> {
    let current = paths.current();
    let target = PathBuf::from(GENERATIONS_DIR).join(number.to_string());
    let staged = paths
        .root
        .join(format!(".{CURRENT}.vivarium-{}.new", std::process::id()));
    let _ = fs::remove_file(&staged);
    std::os::unix::fs::symlink(&target, &staged).map_err(|source| {
        RegistryError::io(
            "generation-current",
            staged.clone(),
            "could not stage the current-generation pointer",
            source,
            false,
        )
    })?;
    rename_with(&staged, &current, RenameFlags::empty()).map_err(|source| {
        let _ = fs::remove_file(&staged);
        RegistryError::io(
            "generation-current",
            current.clone(),
            "could not publish the current-generation pointer",
            source,
            false,
        )
    })?;
    // The flush is what makes the completed rename survive a crash — the same publication step
    // ADR-0053 fixes for every other staged rename in this tree. Without it a reported append,
    // activate, or rollback can quietly restore the old pointer on power loss.
    sync_directory(&paths.root).map_err(|fault| {
        RegistryError::io(
            "generation-current",
            fault.path,
            "could not flush the profile directory",
            fault.source,
            false,
        )
    })
}

/// Unlinks one generation: the root symlink first, then its metadata and lock snapshot.
///
/// The root first because that is the half with consequences — spec/11 has pruning unlink the
/// root and a later collection reclaim the path. Absence is idempotent throughout. Denied
/// permission stays `74` rather than `77`: spec/14's `generations prune` row assigns no `77`,
/// unlike `destroy`'s.
///
/// # Errors
///
/// Returns [`RegistryError`] when a present entry cannot be removed.
pub fn unlink(paths: &GenerationPaths, number: u64) -> Result<(), RegistryError> {
    for path in [
        paths.link(number),
        paths.metadata(number),
        paths.lock_snapshot(number),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(RegistryError::io(
                    "generation-unlink",
                    path.clone(),
                    format!("could not unlink `{}`", path.display()),
                    source,
                    false,
                ));
            }
        }
    }
    Ok(())
}

/// The union of numbers either half of the profile knows, ascending.
fn numbers(paths: &GenerationPaths) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    collect(&paths.generations(), |name| name.parse().ok(), &mut found);
    collect(
        &paths.metadata_dir(),
        |name| name.strip_suffix(".json")?.parse().ok(),
        &mut found,
    );
    found
}

/// Reads one directory's entries through `parse`, treating an unreadable directory as empty.
///
/// Empty rather than an error, for the reason [`Generation`] gives: the readers of this scan are
/// fixed at `78`-only or answer about what they removed, so a scan fault must degrade to "nothing
/// visible" rather than mint a code no row admits.
fn collect(directory: &Path, parse: impl Fn(&str) -> Option<u64>, into: &mut BTreeSet<u64>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if let Some(number) = entry.file_name().to_str().and_then(&parse) {
            into.insert(number);
        }
    }
}

/// Publishes one small file through the staged-rename steps ADR-0053 fixes.
fn publish(paths: &GenerationPaths, destination: &Path, bytes: &[u8]) -> Result<(), RegistryError> {
    let directory = paths.metadata_dir();
    let stem = destination.file_name().map_or_else(
        || "generation".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let (temporary, mut file) =
        create_temp_file(&directory, &stem, PRIVATE_FILE_MODE).map_err(|stage| match stage {
            StageFault::Io(fault) => RegistryError::io(
                "generation-stage",
                fault.path,
                "could not stage a generation record",
                fault.source,
                false,
            ),
            StageFault::Exhausted(path) => RegistryError::plain(
                RegistryErrorKind::Io,
                "generation-stage",
                Locus::File(path),
                "could not stage a generation record",
                "ran out of unique staging names",
            ),
        })?;
    let result = (|| {
        file.write_all(bytes).map_err(|source| {
            RegistryError::io(
                "generation-write",
                temporary.clone(),
                "could not write a staged generation record",
                source,
                false,
            )
        })?;
        file.sync_all().map_err(|source| {
            RegistryError::io(
                "generation-write",
                temporary.clone(),
                "could not flush a staged generation record",
                source,
                false,
            )
        })?;
        drop(file);
        rename_with(&temporary, destination, RenameFlags::empty()).map_err(|source| {
            RegistryError::io(
                "generation-publish",
                destination.to_path_buf(),
                "could not publish a generation record",
                source,
                false,
            )
        })?;
        sync_directory(&directory).map_err(|fault| {
            RegistryError::io(
                "generation-publish",
                fault.path,
                "could not flush the generation metadata directory",
                fault.source,
                false,
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Formats an epoch instant as the RFC 3339 UTC string spec/01 fixes for `built_at`.
#[must_use]
pub fn rfc3339_utc(epoch_seconds: u64) -> String {
    let days = i64::try_from(epoch_seconds / 86_400).unwrap_or(0);
    let seconds = epoch_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Parses exactly what [`rfc3339_utc`] writes, and nothing wider.
///
/// A `built_at` this cannot read means the generation's age is unknown, and `--older-than` must
/// treat unknown as retained: deleting on an unparsable date fails in the unrecoverable direction.
#[must_use]
pub fn parse_rfc3339_utc(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let piece = text.get(range)?;
        if !piece.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        piece.parse().ok()
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    // Round-tripped rather than range-checked: `2026-02-31` is within every per-field range and
    // still not a date, and `days_from_civil` would silently normalize it. A value this writer
    // could not have produced must read as an unknown age, which `--older-than` retains.
    if civil_from_days(days) != (year, month, day) {
        return None;
    }
    let total = days * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(total).ok()
}

/// Days-to-date and date-to-days by the standard civil-calendar arithmetic, era-based so the
/// two are exact inverses across leap years without a table.
const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::test_support::ScratchDirectory;

    fn profile(scratch: &ScratchDirectory) -> GenerationPaths {
        GenerationPaths::new(scratch.path(), "demo", "default")
    }

    fn record(store_path: &str) -> GenerationRecord {
        GenerationRecord {
            store_path: store_path.to_owned(),
            lock_digest: "sha256:abcd".to_owned(),
            manifest: "demo".to_owned(),
            backend: "cloud-hypervisor".to_owned(),
            built_at: "2026-08-25T12:00:00Z".to_owned(),
        }
    }

    /// A registrar that only makes the symlink, which is what a store-less test can do.
    fn plain_symlink(link: &Path, store_path: &str) -> Result<(), String> {
        std::os::unix::fs::symlink(store_path, link).map_err(|error| error.to_string())
    }

    /// A digest that names the snapshot's bytes without a store, proving it ran on the snapshot.
    fn fake_digest(snapshot: &Path) -> Result<String, String> {
        let bytes = fs::read(snapshot).map_err(|error| error.to_string())?;
        Ok(format!("sha256:len{}", bytes.len()))
    }

    fn fabricated_output(scratch: &ScratchDirectory, name: &str) -> PathBuf {
        let output = scratch.path().join(name);
        fs::create_dir_all(&output).unwrap();
        output
    }

    const LOCK_BYTES: &[u8] = b"{\"nodes\":{}}";

    #[test]
    fn append_roots_records_and_repoints() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        let output = fabricated_output(&scratch, "out-1");

        let number = append(
            &paths,
            &record(&output.to_string_lossy()),
            LOCK_BYTES,
            fake_digest,
            plain_symlink,
        )
        .unwrap();
        assert_eq!(number, 1);
        assert_eq!(fs::read_link(paths.link(1)).unwrap(), output);
        assert_eq!(fs::read(paths.lock_snapshot(1)).unwrap(), b"{\"nodes\":{}}");
        assert_eq!(current_number(&paths), Some(1));
        assert_eq!(
            current_store_path(&paths).unwrap(),
            output.to_string_lossy()
        );

        let second = fabricated_output(&scratch, "out-2");
        let number = append(
            &paths,
            &record(&second.to_string_lossy()),
            LOCK_BYTES,
            fake_digest,
            plain_symlink,
        )
        .unwrap();
        assert_eq!(number, 2);
        assert_eq!(current_number(&paths), Some(2));

        let rows = list(&paths);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, 1);
        assert!(!rows[0].current);
        assert!(rows[1].current);
        assert_eq!(rows[1].record.as_ref().unwrap().manifest, "demo");
        // The recorded digest is the one computed from the published snapshot, never the seed's.
        assert_eq!(
            rows[1].record.as_ref().unwrap().lock_digest,
            format!(
                "sha256:len{}",
                fs::read(paths.lock_snapshot(2)).unwrap().len()
            )
        );
    }

    #[test]
    fn registration_failure_takes_the_record_back() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);

        let result = append(
            &paths,
            &record("/nix/store/gone"),
            LOCK_BYTES,
            fake_digest,
            |_, _| Err("refused".to_owned()),
        );
        assert!(result.is_err());
        assert!(!paths.metadata(1).exists());
        assert!(!paths.lock_snapshot(1).exists());
        assert!(list(&paths).is_empty());
    }

    #[test]
    fn numbering_never_reuses_across_gaps_or_halves() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        fs::create_dir_all(paths.generations()).unwrap();
        fs::create_dir_all(paths.metadata_dir()).unwrap();

        std::os::unix::fs::symlink("/nix/store/old", paths.link(5)).unwrap();
        assert_eq!(next_number(&paths), 6);
        // A crash that left only the metadata half must still burn the number.
        fs::write(paths.metadata(9), b"{}").unwrap();
        assert_eq!(next_number(&paths), 10);
    }

    #[test]
    fn crash_shaped_trees_still_list() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        fs::create_dir_all(paths.generations()).unwrap();
        fs::create_dir_all(paths.metadata_dir()).unwrap();

        // A link with no record, and a record with no link — both render, incomplete.
        std::os::unix::fs::symlink("/nix/store/linked", paths.link(1)).unwrap();
        fs::write(
            paths.metadata(2),
            serde_json::to_vec(&record("/nix/store/recorded")).unwrap(),
        )
        .unwrap();
        // Unparsable metadata degrades to an empty record, never a failure.
        fs::write(paths.metadata(3), b"not json").unwrap();
        std::os::unix::fs::symlink("/nix/store/third", paths.link(3)).unwrap();

        let rows = list(&paths);
        assert_eq!(
            rows.iter().map(|row| row.number).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(rows[0].store_path.as_deref(), Some("/nix/store/linked"));
        assert!(rows[0].record.is_none());
        assert!(rows[1].store_path.is_none());
        assert_eq!(
            rows[1].record.as_ref().unwrap().store_path,
            "/nix/store/recorded"
        );
        assert!(rows[2].record.is_none());
    }

    #[test]
    fn collected_output_reads_as_no_build() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        let output = fabricated_output(&scratch, "out-1");
        append(
            &paths,
            &record(&output.to_string_lossy()),
            LOCK_BYTES,
            fake_digest,
            plain_symlink,
        )
        .unwrap();

        fs::remove_dir_all(&output).unwrap();
        assert_eq!(current_store_path(&paths), None);
        assert_eq!(current_number(&paths), Some(1));
    }

    #[test]
    fn unlink_is_complete_and_idempotent() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        let output = fabricated_output(&scratch, "out-1");
        append(
            &paths,
            &record(&output.to_string_lossy()),
            LOCK_BYTES,
            fake_digest,
            plain_symlink,
        )
        .unwrap();

        unlink(&paths, 1).unwrap();
        assert!(fs::symlink_metadata(paths.link(1)).is_err());
        assert!(!paths.metadata(1).exists());
        assert!(!paths.lock_snapshot(1).exists());
        unlink(&paths, 1).unwrap();
    }

    #[test]
    fn set_current_repoints_atomically_named() {
        let scratch = ScratchDirectory::new().unwrap();
        let paths = profile(&scratch);
        fs::create_dir_all(paths.generations()).unwrap();
        std::os::unix::fs::symlink("/nix/store/a", paths.link(1)).unwrap();
        std::os::unix::fs::symlink("/nix/store/b", paths.link(2)).unwrap();

        set_current(&paths, 1).unwrap();
        assert_eq!(current_number(&paths), Some(1));
        set_current(&paths, 2).unwrap();
        assert_eq!(current_number(&paths), Some(2));
        // No staged sibling survives a completed repoint.
        let leftovers: Vec<_> = fs::read_dir(scratch.path().join("projects/demo/default"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn rfc3339_round_trips() {
        for seconds in [
            0,
            951_782_400,   // 2000-02-29, a century leap day
            1_772_150_400, // 2026-02-27
            4_107_542_399, // far future, end-of-day
        ] {
            let text = rfc3339_utc(seconds);
            assert_eq!(parse_rfc3339_utc(&text), Some(seconds), "{text}");
        }
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn parse_rejects_normalizable_dates() {
        // Within every per-field range and still not a date: `days_from_civil` would normalize
        // it, and a normalized reading is a guessed age `--older-than` must not delete on.
        assert_eq!(parse_rfc3339_utc("2026-02-31T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_utc("2025-02-29T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_utc("2026-04-31T00:00:00Z"), None);
        assert_eq!(
            parse_rfc3339_utc("2024-02-29T00:00:00Z"),
            Some(1_709_164_800)
        );
    }

    #[test]
    fn parse_rejects_what_it_never_writes() {
        for text in [
            "",
            "2026-08-25",
            "2026-08-25 12:00:00Z",
            "2026-08-25T12:00:00+00:00",
            "2026-13-01T00:00:00Z",
            "2026-08-25T24:00:00Z",
            "not a date at all!!",
        ] {
            assert_eq!(parse_rfc3339_utc(text), None, "{text}");
        }
    }
}
