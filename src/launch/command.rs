//! Immutable command descriptions; no shell rendering is used for execution.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: PathBuf,
    args: Vec<String>,
}

impl CommandSpec {
    pub(crate) const fn new(program: PathBuf, args: Vec<String>) -> Self {
        Self { program, args }
    }

    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    #[must_use]
    pub fn rendered(&self) -> Vec<String> {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .collect()
    }
}
