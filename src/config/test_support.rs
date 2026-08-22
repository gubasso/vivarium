use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

use super::Environment;

pub use crate::test_support::ScratchDirectory;

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

    fn dynamic_variable(&self, name: &str) -> Option<OsString> {
        self.variables.get(name).cloned()
    }
}
