//! The project registry: the one home for project→manifest bindings.
//!
//! `registry.toml` under the state root, keyed by a project's canonical absolute path. ADR-0011
//! put it here rather than in the config root because the tool never writes config, and spec/02
//! makes exactly two of its keys — `path` and `manifest` — a supported interface: `viv init` prints
//! this block for a user to paste, so a hand-written entry has to parse the same way a written one
//! does. That is also why the grammar is walked as a spanned tree like the manifest's rather than
//! deserialized: an unknown key here is the same compatibility signal, and spec/14 requires it to
//! carry a position.
//!
//! Nothing else about the state root is specified. This module writes one file and reads one file,
//! and the identity index that lives beside it (spec/15) is deliberately not here: different key,
//! different write gate, different lifetime.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use rustix::fs::RenameFlags;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::atomic::{
    Fault, PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, StageFault, create_temp_file,
    remove_file_if_exists, rename_with, sync_directory,
};
use super::error::{RegistryError, RegistryErrorKind};
use crate::diagnostic::Locus;

/// The file name spec/02 fixes. Frozen: a rename would break every snippet already pasted.
pub const REGISTRY_FILE: &str = "registry.toml";

/// The accepted keys at the registry root.
const ROOT_KEYS: &[&str] = &["projects"];

/// The accepted keys in a `[[projects]]` record. Both are a supported interface (ADR-0052).
const PROJECT_KEYS: &[&str] = &["path", "manifest"];

/// The `-->` slot every registry failure that has no parser position reports.
const LOCUS: Locus = Locus::Named("state registry");

/// One binding: a project directory and the manifest name it resolves to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The project directory's canonical, symlink-resolved absolute path.
    pub path: PathBuf,
    /// The bare kebab-case manifest name, never a resolved file path.
    ///
    /// A path would be a stale pointer for no gain: resolution is a config-root function, and the
    /// config root is user-mutable behind the tool's back (N13).
    pub manifest: String,
}

/// Every binding recorded on this machine.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Registry {
    projects: Vec<Binding>,
}

impl Registry {
    /// The manifest bound to `path`, if any.
    #[must_use]
    pub fn lookup(&self, path: &Path) -> Option<&Binding> {
        self.projects.iter().find(|binding| binding.path == path)
    }

    /// Every binding, in the order the file records them.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.projects
    }

    /// Records a binding, replacing any the same project already had.
    ///
    /// Replacement rather than a second entry because spec/02 canonicalizes the key exactly as
    /// identity resolution does, so that one directory can never acquire two bindings — an
    /// invariant the writer has to keep, not just the reader.
    pub fn bind(&mut self, path: PathBuf, manifest: String) {
        match self
            .projects
            .iter_mut()
            .find(|binding| binding.path == path)
        {
            Some(existing) => existing.manifest = manifest,
            None => self.projects.push(Binding { path, manifest }),
        }
    }

    /// Renders the exact `[[projects]]` block spec/02 publishes and `viv init` prints.
    #[must_use]
    pub fn snippet(path: &Path, manifest: &str) -> String {
        format!(
            "[[projects]]\npath = {}\nmanifest = {}\n",
            toml_string(&path.to_string_lossy()),
            toml_string(manifest)
        )
    }

    fn render(&self) -> String {
        let mut rendered = String::new();
        for binding in &self.projects {
            if !rendered.is_empty() {
                rendered.push('\n');
            }
            rendered.push_str(&Self::snippet(&binding.path, &binding.manifest));
        }
        rendered
    }
}

/// Where the registry and its sidecar lock live beneath a state root.
#[must_use]
pub fn registry_path(state_root: &Path) -> PathBuf {
    state_root.join(REGISTRY_FILE)
}

fn lock_path(state_root: &Path) -> PathBuf {
    state_root.join(format!("{REGISTRY_FILE}.lock"))
}

/// Reads the registry without taking the lock.
///
/// # Errors
///
/// Returns [`RegistryError`] when the file exists but cannot be read (`74`), is not TOML (`78`), or
/// says something outside the grammar (`78`). An absent or zero-length file is the empty registry,
/// which spec/02 distinguishes from a corrupt one: vivarium never silently rebuilds either.
pub fn read(state_root: &Path) -> Result<Registry, RegistryError> {
    let path = registry_path(state_root);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(error) => {
            return Err(RegistryError::io(
                "unreadable",
                path.clone(),
                format!("could not read the state registry `{}`", path.display()),
                error,
                // A read answers `74` however it became unreadable — spec/02's table has no `77`
                // row, because a registry a user cannot read is the channel failing either way.
                false,
            ));
        }
    };
    parse(&source, &path)
}

/// Parses registry text, which is the whole grammar and touches no filesystem.
///
/// # Errors
///
/// Returns [`RegistryError`] for malformed TOML, an unknown key, a missing required key, or a key
/// holding the wrong type. Every one of them is `78`.
pub fn parse(source: &str, path: &Path) -> Result<Registry, RegistryError> {
    if source.trim().is_empty() {
        return Ok(Registry::default());
    }

    let document = DeTable::parse(source).map_err(|error| {
        config_fault(
            "syntax",
            locus_at(path, source, error.span().map(|span| span.start)),
            format!("the state registry `{}` is not valid TOML", path.display()),
            error.message().to_owned(),
        )
    })?;
    let root = document.get_ref();

    // The unknown-key sweep runs first and over the whole document, for the reason the manifest's
    // does: this message is the entire compatibility signal for a file carrying no schema version,
    // so it must report the earliest offending key rather than whichever the walk reaches first.
    if let Some(error) = first_unknown_key(root, source, path) {
        return Err(error);
    }

    let mut registry = Registry::default();
    let Some(projects) = entry(root, "projects") else {
        return Ok(registry);
    };
    let DeValue::Array(records) = projects.get_ref() else {
        return Err(wrong_type(path, source, projects, "projects", "an array"));
    };

    for record in records {
        let DeValue::Table(table) = record.get_ref() else {
            return Err(wrong_type(path, source, record, "projects", "a table"));
        };
        let mut fields = Vec::new();
        for key in PROJECT_KEYS {
            let Some(value) = entry(table, key) else {
                return Err(config_fault(
                    "missing-key",
                    locus_at(path, source, Some(record.span().start)),
                    format!("missing required key `{key}` in the state registry"),
                    "every `[[projects]]` record carries both `path` and `manifest`",
                ));
            };
            let DeValue::String(text) = value.get_ref() else {
                return Err(wrong_type(
                    path,
                    source,
                    value,
                    &format!("projects.{key}"),
                    "a string",
                ));
            };
            fields.push((*key, text.as_ref().to_owned(), value));
        }

        let (_, path_text, path_value) = &fields[0];
        if !Path::new(path_text).is_absolute() {
            return Err(config_fault(
                "invalid-value",
                locus_at(path, source, Some(path_value.span().start)),
                "invalid value for key `path` in the state registry",
                "expected a canonical absolute project path",
            ));
        }
        let (_, manifest, manifest_value) = &fields[1];
        if !super::artifact::valid_artifact_name(manifest) {
            return Err(config_fault(
                "invalid-value",
                locus_at(path, source, Some(manifest_value.span().start)),
                "invalid value for key `manifest` in the state registry",
                "expected a bare kebab-case manifest name, not a file path",
            ));
        }

        registry.bind(PathBuf::from(path_text), manifest.clone());
    }

    Ok(registry)
}

/// Reads the registry, hands it to `change`, and republishes it atomically under the exclusive
/// lock.
///
/// The read happens inside the lock rather than before it, which is the whole point: a caller that
/// read first and wrote second would lose a concurrent binding to the classic lost update, the
/// failure ADR-0053 calls out as silent.
///
/// # Errors
///
/// Returns [`RegistryError`] when the lock cannot be taken promptly (`75`), the current contents
/// cannot be read or parsed, or the replacement cannot be staged, flushed, or published.
pub fn update<T>(
    state_root: &Path,
    change: impl FnOnce(&mut Registry) -> T,
) -> Result<T, RegistryError> {
    let guard = Guard::acquire(state_root)?;
    let mut registry = read(state_root)?;
    let outcome = change(&mut registry);
    write(state_root, &registry)?;
    drop(guard);
    Ok(outcome)
}

/// The exclusive `flock` on the sidecar, held for exactly as long as the read-modify-write.
///
/// ADR-0053 fixes one total order across every lock vivarium takes — registry, then the identity
/// index, then the per-target `flock`, then the Nix profile — acquired in that order and released
/// in reverse. The registry is the first link, so this file takes nothing before it; a later lock
/// site is required to place itself explicitly rather than assume.
struct Guard(File);

impl Guard {
    fn acquire(state_root: &Path) -> Result<Self, RegistryError> {
        ensure_state_root(state_root)?;
        let path = lock_path(state_root);
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(PRIVATE_FILE_MODE)
            .open(&path)
            .map_err(|source| {
                RegistryError::io(
                    "lock-open",
                    path.clone(),
                    format!("could not open the registry lock `{}`", path.display()),
                    source,
                    true,
                )
            })?;
        // `try_lock` rather than `lock`: spec/02 makes a lock that cannot be taken promptly a `75`
        // rather than a hang, so a contended registry tells the caller to retry instead of
        // stalling behind whatever the other process is doing.
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(fs::TryLockError::WouldBlock) => Err(RegistryError::plain(
                RegistryErrorKind::Contended,
                "lock-unavailable",
                Locus::File(path),
                "another vivarium process holds the registry lock",
                "the exclusive lock could not be taken promptly; retry",
            )),
            Err(fs::TryLockError::Error(source)) => Err(RegistryError::io(
                "lock-open",
                path.clone(),
                format!("could not lock the registry lock `{}`", path.display()),
                source,
                true,
            )),
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // `flock` releases on close, and on process death — which is what stops a crash from
        // leaving the registry wedged. Unlocking explicitly only makes the ordinary path explicit.
        let _ = self.0.unlock();
    }
}

/// Publishes a registry so that a concurrent reader sees the old file or the new one, never both.
fn write(state_root: &Path, registry: &Registry) -> Result<(), RegistryError> {
    ensure_state_root(state_root)?;
    let destination = registry_path(state_root);
    let bytes = registry.render();

    let (temporary, mut file) = create_temp_file(state_root, REGISTRY_FILE, PRIVATE_FILE_MODE)
        .map_err(|fault| {
            stage_fault(
                "write-temporary",
                "could not stage the state registry",
                fault,
            )
        })?;
    let result = (|| {
        file.write_all(bytes.as_bytes()).map_err(|source| {
            write_io(
                "write",
                &temporary,
                "could not write the staged registry",
                source,
            )
        })?;
        file.sync_all().map_err(|source| {
            write_io(
                "write-sync",
                &temporary,
                "could not flush the staged registry",
                source,
            )
        })?;
        drop(file);
        rename_with(&temporary, &destination, RenameFlags::empty()).map_err(|source| {
            write_io(
                "publish",
                &destination,
                "could not publish the state registry",
                source,
            )
        })?;
        sync_directory(state_root)
            .map_err(|fault| write_io_at("directory-sync", fault, "could not flush the state root"))
    })();
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

/// Creates the state root at `0700`, and corrects the mode if it already exists wider.
///
/// Correcting rather than only creating, because ADR-0053 makes the mode an invariant of the
/// directory rather than of the moment it was made: a root that predates this rule, or that a
/// permissive umask widened, would otherwise stay readable for the life of the installation.
fn ensure_state_root(state_root: &Path) -> Result<(), RegistryError> {
    fs::create_dir_all(state_root).map_err(|source| {
        RegistryError::io(
            "write-parent",
            state_root.to_path_buf(),
            format!("could not create the state root `{}`", state_root.display()),
            source,
            true,
        )
    })?;
    let metadata = fs::metadata(state_root).map_err(|source| {
        RegistryError::io(
            "write-parent",
            state_root.to_path_buf(),
            format!(
                "could not inspect the state root `{}`",
                state_root.display()
            ),
            source,
            true,
        )
    })?;
    if metadata.permissions().mode() & 0o777 != PRIVATE_DIR_MODE {
        fs::set_permissions(state_root, fs::Permissions::from_mode(PRIVATE_DIR_MODE)).map_err(
            |source| {
                RegistryError::io(
                    "write-permissions",
                    state_root.to_path_buf(),
                    format!(
                        "could not make the state root `{}` private",
                        state_root.display()
                    ),
                    source,
                    true,
                )
            },
        )?;
    }
    Ok(())
}

fn stage_fault(condition: &'static str, message: &'static str, fault: StageFault) -> RegistryError {
    match fault {
        StageFault::Io(fault) => write_io_at(condition, fault, message),
        StageFault::Exhausted(parent) => RegistryError::plain(
            RegistryErrorKind::Io,
            "temporary-collision",
            Locus::File(parent),
            "could not allocate a unique staged-registry sibling",
            "the bounded temporary-name retry set was exhausted",
        ),
    }
}

fn write_io_at(condition: &'static str, fault: Fault, message: &str) -> RegistryError {
    write_io(condition, &fault.path, message, fault.source)
}

fn write_io(
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
        // Every write path answers `77` on a denied permission, which spec/14's command matrix
        // gives `viv init --write` separately from its `74`.
        true,
    )
}

fn config_fault(
    condition: &'static str,
    locus: Locus,
    message: impl Into<String>,
    why: impl Into<String>,
) -> RegistryError {
    RegistryError::plain(RegistryErrorKind::Config, condition, locus, message, why)
}

fn wrong_type(
    path: &Path,
    source: &str,
    value: &Spanned<DeValue<'_>>,
    key: &str,
    expected: &str,
) -> RegistryError {
    config_fault(
        "wrong-type",
        locus_at(path, source, Some(value.span().start)),
        format!("wrong type for key `{key}` in the state registry"),
        format!("expected {expected}"),
    )
}

/// Finds the earliest unknown key, descending only through keys the grammar knows.
fn first_unknown_key(root: &DeTable<'_>, source: &str, path: &Path) -> Option<RegistryError> {
    let mut found: Vec<(usize, String, &'static [&'static str])> = Vec::new();
    collect_unknown_keys(root, ROOT_KEYS, &mut found);
    found.sort_by_key(|(offset, ..)| *offset);
    found.into_iter().next().map(|(offset, key, accepted)| {
        config_fault(
            "unknown-key",
            locus_at(path, source, Some(offset)),
            format!("unknown key `{key}` in the state registry"),
            // The version is the whole point: with no schema version in the file, this is what
            // turns "unknown key" into "your registry is newer than your tool".
            format!(
                "not part of the registry grammar viv {} understands",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .with_accepted(accepted.iter().copied())
    })
}

fn collect_unknown_keys(
    table: &DeTable<'_>,
    accepted: &'static [&'static str],
    found: &mut Vec<(usize, String, &'static [&'static str])>,
) {
    for (key, value) in table {
        let name = key.get_ref().as_ref();
        if !accepted.contains(&name) {
            found.push((key.span().start, name.to_owned(), accepted));
            continue;
        }
        if name == "projects"
            && let DeValue::Array(records) = value.get_ref()
        {
            for record in records {
                if let DeValue::Table(inner) = record.get_ref() {
                    collect_unknown_keys(inner, PROJECT_KEYS, found);
                }
            }
        }
    }
}

fn entry<'t>(table: &'t DeTable<'_>, key: &str) -> Option<&'t Spanned<DeValue<'t>>> {
    table
        .iter()
        .find(|(name, _)| name.get_ref().as_ref() == key)
        .map(|(_, value)| value)
}

fn locus_at(path: &Path, source: &str, offset: Option<usize>) -> Locus {
    offset.map_or(LOCUS, |offset| Locus::in_source(path, source, offset))
}

/// Quotes a value as a TOML basic string, escaping what the format requires.
///
/// Hand-rolled rather than reached for through a serializer because the snippet is a published
/// contract: spec/02 shows the exact three lines, and a serializer's table ordering or spacing
/// could drift the block a user is invited to paste.
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
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    use super::{Registry, parse, read, registry_path, update};
    use crate::config::test_support::ScratchDirectory;
    use crate::exit::ExitKind;

    fn registry_source() -> &'static str {
        concat!(
            "[[projects]]\npath = \"/home/alice/backend\"\nmanifest = \"rust-web\"\n",
            "\n[[projects]]\npath = \"/home/alice/site\"\nmanifest = \"node\"\n"
        )
    }

    /// Pins spec/02's published record shape, both keys, and document order.
    #[test]
    fn the_published_record_shape_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let registry = parse(registry_source(), Path::new("registry.toml"))?;
        assert_eq!(registry.bindings().len(), 2);
        assert_eq!(
            registry
                .lookup(Path::new("/home/alice/backend"))
                .map(|b| b.manifest.as_str()),
            Some("rust-web")
        );
        assert_eq!(
            registry
                .lookup(Path::new("/home/alice/site"))
                .map(|b| b.manifest.as_str()),
            Some("node")
        );
        assert_eq!(registry.lookup(Path::new("/home/alice/absent")), None);

        // The rendering a write publishes is pinned as the source text itself, not
        // a reparse: the literal is the on-disk contract, and equality against it
        // proves the reader accepts what the writer emits without the loop being
        // self-referential.
        assert_eq!(registry.render(), registry_source());
        Ok(())
    }

    /// Pins the snippet `viv init` prints as exactly what the parser accepts.
    #[test]
    fn the_printed_snippet_is_what_the_parser_accepts() -> Result<(), Box<dyn std::error::Error>> {
        let snippet = Registry::snippet(Path::new("/home/alice/backend"), "rust-web");
        assert_eq!(
            snippet,
            "[[projects]]\npath = \"/home/alice/backend\"\nmanifest = \"rust-web\"\n"
        );
        let parsed = parse(&snippet, Path::new("registry.toml"))?;
        assert_eq!(parsed.bindings().len(), 1);
        Ok(())
    }

    /// Pins spec/02's absent and zero-length rows as the empty collection, not a defect.
    #[test]
    fn absent_and_empty_are_the_empty_registry() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        assert_eq!(read(&state)?, Registry::default());

        fs::create_dir_all(&state)?;
        fs::write(registry_path(&state), b"")?;
        assert_eq!(read(&state)?, Registry::default());

        fs::write(registry_path(&state), b"\n\n  \n")?;
        assert_eq!(read(&state)?, Registry::default());
        Ok(())
    }

    /// Pins spec/02's unreadable row to `74` — the channel failing, not its contents being wrong.
    #[test]
    fn an_unreadable_registry_is_an_io_failure() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        fs::create_dir_all(&state)?;
        let path = registry_path(&state);
        fs::write(&path, registry_source())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;

        // Root ignores the mode, so a run as root proves nothing here rather than failing wrongly.
        let readable = fs::read_to_string(&path).is_ok();
        if !readable {
            let error = read(&state).err().ok_or("expected an unreadable failure")?;
            assert_eq!(error.exit_code(), ExitKind::IoErr);
            assert_eq!(error.diagnostic().id().to_string(), "state.unreadable");
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    /// Pins every authored-defect row of spec/02's table to `78` with its own id.
    #[test]
    fn every_authored_defect_is_config_with_its_own_id() -> Result<(), String> {
        let rows = [
            ("[[projects]\npath = \"/a\"\n", "state.syntax"),
            (
                "[[projects]]\npath = \"/a\"\nmanifest = \"m\"\nschema_version = 1\n",
                "state.unknown-key",
            ),
            ("[[projects]]\npath = \"/a\"\n", "state.missing-key"),
            (
                "[[projects]]\npath = 7\nmanifest = \"m\"\n",
                "state.wrong-type",
            ),
            (
                "[[projects]]\npath = \"relative\"\nmanifest = \"m\"\n",
                "state.invalid-value",
            ),
            (
                "[[projects]]\npath = \"/a\"\nmanifest = \"/etc/m.toml\"\n",
                "state.invalid-value",
            ),
            ("projects = 7\n", "state.wrong-type"),
        ];
        for (source, expected_id) in rows {
            let error = parse(source, Path::new("registry.toml"))
                .err()
                .ok_or_else(|| format!("accepted a defective registry: {source:?}"))?;
            assert_eq!(error.exit_code(), ExitKind::Config, "for {source:?}");
            assert_eq!(
                error.diagnostic().id().to_string(),
                expected_id,
                "for {source:?}"
            );
        }
        Ok(())
    }

    /// Pins all five parts spec/14 requires of the compatibility message.
    #[test]
    fn the_unknown_key_message_carries_all_five_parts() -> Result<(), String> {
        let source = "[[projects]]\npath = \"/a\"\nmanifest = \"m\"\nkeep_warm = true\n";
        let error = parse(source, Path::new("/state/registry.toml"))
            .err()
            .ok_or("accepted an unknown key")?;
        let rendered = error.diagnostic().to_string();
        for part in [
            "registry.toml",           // the file
            ":4:1",                    // the failing position
            "keep_warm",               // the unknown key
            "path, manifest",          // the accepted key set at that position
            env!("CARGO_PKG_VERSION"), // the CLI version
        ] {
            assert!(rendered.contains(part), "missing {part:?} in:\n{rendered}");
        }
        Ok(())
    }

    /// Pins the read-modify-write round trip and the `0600`/`0700` modes ADR-0053 fixes.
    #[test]
    fn an_update_publishes_privately_and_reads_back() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");

        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/backend"), "rust-web".to_owned());
        })?;
        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/site"), "node".to_owned());
        })?;
        // Re-binding the same project replaces its entry, so one directory never holds two.
        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/backend"), "rust-cli".to_owned());
        })?;

        let registry = read(&state)?;
        assert_eq!(registry.bindings().len(), 2);
        assert_eq!(
            registry
                .lookup(Path::new("/home/alice/backend"))
                .map(|b| b.manifest.as_str()),
            Some("rust-cli")
        );

        let file_mode = fs::metadata(registry_path(&state))?.permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600, "registry is not private");
        let dir_mode = fs::metadata(&state)?.permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "state root is not private");
        Ok(())
    }

    /// Pins the state root's mode as an invariant of the directory, not of its creation moment.
    #[test]
    fn a_widened_state_root_is_made_private_again() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        fs::create_dir_all(&state)?;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o755))?;

        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/backend"), "rust-web".to_owned());
        })?;
        assert_eq!(fs::metadata(&state)?.permissions().mode() & 0o777, 0o700);
        Ok(())
    }

    /// Pins spec/02's `75`: a contended registry reports rather than hangs.
    #[test]
    fn a_held_lock_is_transient_rather_than_a_hang() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/backend"), "rust-web".to_owned());
        })?;

        // A second exclusive holder, standing in for a concurrent `viv init --write`.
        let held = fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(state.join("registry.toml.lock"))?;
        held.lock()?;

        let error = update(&state, |_| ())
            .err()
            .ok_or("expected the second writer to be refused")?;
        assert_eq!(error.exit_code(), ExitKind::TempFail);
        assert_eq!(
            error.diagnostic().id().to_string(),
            "state.lock-unavailable"
        );

        held.unlock()?;
        // The lock is advisory and released, so the same call now succeeds.
        update(&state, |_| ())?;
        Ok(())
    }

    /// Pins the atomicity ADR-0053 buys: a reader sees the old file or the new one, never a
    /// partial one. Asserted through the published inode rather than by racing a reader, because
    /// a race that happens not to interleave would pass without evidence.
    #[test]
    fn publication_replaces_rather_than_rewrites() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::MetadataExt as _;

        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/backend"), "rust-web".to_owned());
        })?;
        let first = fs::metadata(registry_path(&state))?.ino();

        update(&state, |registry| {
            registry.bind(PathBuf::from("/home/alice/site"), "node".to_owned());
        })?;
        let second = fs::metadata(registry_path(&state))?.ino();

        assert_ne!(
            first, second,
            "the registry was rewritten in place, so a concurrent reader could see it torn"
        );
        // And nothing staged is left behind for a library listing or a later run to trip over.
        let strays: Vec<_> = fs::read_dir(&state)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".new"))
            .collect();
        assert!(strays.is_empty(), "staged siblings left behind: {strays:?}");
        Ok(())
    }

    /// Pins the read-inside-the-lock rule: an update sees what a concurrent one already committed.
    #[test]
    fn an_update_never_loses_a_concurrent_binding() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let state = scratch.path().join("state");
        update(&state, |registry| {
            registry.bind(PathBuf::from("/a"), "one".to_owned());
        })?;

        // A caller holding a stale snapshot from before the other write.
        let snapshot = read(&state)?;
        update(&state, |registry| {
            registry.bind(PathBuf::from("/b"), "two".to_owned());
        })?;
        assert_eq!(snapshot.bindings().len(), 1);

        update(&state, |registry| {
            registry.bind(PathBuf::from("/c"), "three".to_owned());
        })?;
        let final_registry = read(&state)?;
        assert_eq!(
            final_registry.bindings().len(),
            3,
            "an update re-read inside the lock would have kept all three"
        );
        Ok(())
    }
}
