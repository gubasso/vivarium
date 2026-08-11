use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::Environment;

static NEXT_SCRATCH_ID: AtomicU64 = AtomicU64::new(0);

pub struct ScratchDirectory {
    path: PathBuf,
}

impl ScratchDirectory {
    pub fn new() -> io::Result<Self> {
        let id = NEXT_SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("vivarium-config-tests-{}-{id}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
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

#[derive(Default)]
pub struct TestEnvironment {
    variables: BTreeMap<&'static str, OsString>,
}

impl TestEnvironment {
    pub fn from<const N: usize>(variables: [(&'static str, &'static str); N]) -> Self {
        Self {
            variables: variables
                .into_iter()
                .map(|(name, value)| (name, OsString::from(value)))
                .collect(),
        }
    }

    pub fn from_paths<const N: usize, P>(variables: [(&'static str, P); N]) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            variables: variables
                .into_iter()
                .map(|(name, value)| (name, value.as_ref().as_os_str().to_os_string()))
                .collect(),
        }
    }
}

impl Environment for TestEnvironment {
    fn variable(&self, name: &'static str) -> Option<OsString> {
        self.variables.get(name).cloned()
    }
}
