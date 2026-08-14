//! Crate-wide test scaffolding. `#[cfg(test)]`-only, so nothing here exists in
//! the shipped binaries or the library integration tests link.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SCRATCH_ID: AtomicU64 = AtomicU64::new(0);

/// An RAII scratch directory under the system temp root.
///
/// The name carries the process id and a per-process counter, because nextest
/// runs a binary's tests as threads of one process: a pid alone collides the
/// moment two tests want a directory, and a timestamp collides on a fast
/// clock. `Drop` removes the tree, so a passing run leaves nothing behind; a
/// killed process leaks one uniquely-named directory rather than corrupting a
/// shared one.
pub struct ScratchDirectory {
    path: PathBuf,
}

impl ScratchDirectory {
    pub fn new() -> io::Result<Self> {
        loop {
            let id = NEXT_SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("vivarium-tests-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                // A recycled pid can meet the directory a killed predecessor
                // leaked; exclusive creation refuses its stale contents, and
                // the next counter value steps past it.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
