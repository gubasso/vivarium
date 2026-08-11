//! The mechanics of publishing a file without ever exposing a partial one.
//!
//! Two callers need the same steps — the generated flake and its pin in `materialize`, and the
//! state registry in `registry` — and ADR-0053 and ADR-0058 fix those steps identically for both:
//! stage a same-directory temporary, flush it, rename it over the target, then flush the parent.
//! What the two do not share is how a failure is named. Their diagnostic ids come from different
//! namespaces and their conditions are frozen strings in different spec tables, so a shared error
//! type would have to be a union of two id spaces.
//!
//! So this module owns the steps and none of the vocabulary: every fallible function returns the
//! path it was working on beside the raw `io::Error`, and each caller turns that pair into its own
//! diagnostic. That is the only split that lets one mechanism carry two contracts.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{CWD, RenameFlags, renameat_with};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// How many unique-name attempts a staged allocation makes before giving up.
///
/// Bounded rather than unbounded because the names embed a process id and a monotonic counter, so
/// exhausting this many is evidence of something wrong with the directory rather than of bad luck.
const MAX_TEMP_ATTEMPTS: u64 = 64;

/// The mode every file vivarium writes under the state root carries (ADR-0053).
pub(super) const PRIVATE_FILE_MODE: u32 = 0o600;

/// The mode the state root itself carries (ADR-0053).
pub(super) const PRIVATE_DIR_MODE: u32 = 0o700;

/// A failed step, carrying what it was touching and why it failed, but not what to call it.
#[derive(Debug)]
pub(super) struct Fault {
    pub(super) path: PathBuf,
    pub(super) source: io::Error,
}

impl Fault {
    fn at(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Why a staged allocation failed: the channel, or the bounded retry set running out.
#[derive(Debug)]
pub(super) enum StageFault {
    Io(Fault),
    Exhausted(PathBuf),
}

/// Allocates a uniquely named sibling directory beside `parent`.
pub(super) fn create_temp_directory(parent: &Path) -> Result<PathBuf, StageFault> {
    for _ in 0..MAX_TEMP_ATTEMPTS {
        let path = unique_path(parent, "flake", "tmp");
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(StageFault::Io(Fault::at(&path, error))),
        }
    }
    Err(StageFault::Exhausted(parent.to_path_buf()))
}

/// Allocates and opens a uniquely named sibling file beside `parent`, at `mode`.
///
/// The mode is set at creation rather than afterwards, so the file is never briefly readable by
/// anyone the final mode excludes — the registry holds absolute paths, which ADR-0053 calls weak
/// but real information about a user's filesystem.
pub(super) fn create_temp_file(
    parent: &Path,
    stem: &str,
    mode: u32,
) -> Result<(PathBuf, File), StageFault> {
    for _ in 0..MAX_TEMP_ATTEMPTS {
        let path = unique_path(parent, stem, "new");
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(StageFault::Io(Fault::at(&path, error))),
        }
    }
    Err(StageFault::Exhausted(parent.to_path_buf()))
}

/// The name a staged sibling takes: dot-prefixed so no library listing enumerates it, and carrying
/// both the process id and a counter so two writers in one process cannot collide either.
fn unique_path(parent: &Path, stem: &str, suffix: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{stem}.vivarium-{}-{id}.{suffix}",
        std::process::id()
    ))
}

/// Renames with explicit flags, so a caller can demand no-replace or an atomic exchange.
pub(super) fn rename_with(source: &Path, destination: &Path, flags: RenameFlags) -> io::Result<()> {
    renameat_with(CWD, source, CWD, destination, flags).map_err(io::Error::from)
}

/// Flushes a directory, which is what makes a completed rename survive a crash.
pub(super) fn sync_directory(path: &Path) -> Result<(), Fault> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| Fault::at(path, source))
}

/// Removes a staged sibling on the failure path, where nothing can be done about a second failure.
pub(super) fn remove_file_if_exists(path: &Path) {
    let _ = fs::remove_file(path);
}
