//! What the last successful `start` knew about a project's volumes.
//!
//! `volumes.toml` under the state root at `projects/<project-id>/<target>/`, beside the images it
//! describes. spec/02 fixes two state-root keys as a supported interface and leaves every other
//! file there unspecified (ADR-0052), so this shape is the tool's own and may change with it.
//!
//! It exists because `viv volume list` and `viv volume prune` have to name each volume's declaring
//! layer and say which images on disk no layer declares any more — and provenance lives only in a
//! merged evaluation, which spec/14 does not let either verb run: both are read-only rows that
//! answer `78` and nothing else, while an evaluation can answer `65`, `69`, `70`, or `74`. So the
//! evaluating verb writes down what it learned and the reading verbs read it. spec/01 already
//! requires exactly this of `prune`, whose `mount` column is "the guest path the volume used to
//! occupy, carried from the state that outlived the declaration" — a sentence with no other
//! possible source, since an orphan is by definition declared by nothing current.
//!
//! The record is stale by construction, which is why [`Record::live`] is careful rather than
//! trusting: it vouches for a remembered volume only while the composition that produced it is
//! unchanged, and never for one the manifest could have spoken about itself.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use rustix::fs::RenameFlags;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::atomic::{
    Fault, PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, StageFault, create_temp_file, rename_with,
    sync_directory,
};
use super::error::{RegistryError, RegistryErrorKind};
use super::manifest::Manifest;
use crate::diagnostic::Locus;

/// The file name, under the project's own target directory.
pub const VOLUMES_FILE: &str = "volumes.toml";

/// The `-->` slot a failure with no parser position reports.
const LOCUS: Locus = Locus::Named("volume record");

/// What one layer declared, as the merge reported it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredVolume {
    /// The volume's name, which is also its image's basename under `volumes/`.
    pub name: String,
    /// The guest path it mounts at.
    pub mount: String,
    /// The artifact that declared it — an image, a piece, `extends`, or the manifest itself.
    pub declared_by: String,
    /// Which of those four it was, as [`crate::config::evaluate::LayerKind`] spells it.
    pub kind: String,
    /// The declared ceiling in GiB, absent when the layer left it to the per-volume default.
    pub size_gib: Option<u32>,
}

/// The composition a record was written under.
///
/// Not a timestamp and not a hash of the manifest: what matters is whether the same set of layers
/// is still in play, because that is exactly the condition under which a remembered piece-declared
/// volume is still worth believing. A manifest that edited only its `[resources]` has the same
/// layers and the same volumes; one that dropped a piece has neither.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Composition {
    pub image: String,
    pub pieces: Vec<String>,
    pub extends: Option<String>,
}

impl Composition {
    /// The composition a manifest describes right now, read with no Nix evaluation.
    #[must_use]
    pub fn of(manifest: &Manifest) -> Self {
        Self {
            image: manifest.image.clone(),
            pieces: manifest.pieces.clone(),
            extends: manifest.extends.clone(),
        }
    }
}

/// One project target's remembered volumes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Record {
    pub composition: Composition,
    pub volumes: Vec<DeclaredVolume>,
}

impl Record {
    /// Every volume name a current layer declares, for the reserved-name-free part of the join.
    ///
    /// Three sources, and the order of the argument matters more than it looks:
    ///
    /// 1. What the manifest itself declares right now. Fresh, needs no Nix, and is what makes a
    ///    `[[volumes]]` entry removed from the manifest orphan on the very next command.
    /// 2. What this record remembers, but only while the composition is unchanged and only for
    ///    volumes a layer other than the manifest declared. Both halves are load-bearing: the
    ///    composition check retires every remembered row when a piece is dropped, and the
    ///    `kind != manifest` check stops a stale record vouching for a manifest declaration the
    ///    manifest no longer makes.
    ///
    /// The remaining error is one-directional and stated rather than hidden: a piece that changes
    /// its own `vivarium.volumes` while the manifest's composition is unchanged keeps its old
    /// volume alive until the next `viv start` refreshes this file. Late, never false — which is
    /// the bias a verb that deletes user data has to have.
    #[must_use]
    pub fn live(&self, manifest: &Manifest) -> BTreeSet<String> {
        let mut live: BTreeSet<String> = manifest
            .volumes
            .iter()
            .map(|volume| volume.name.clone())
            .collect();
        if self.composition == Composition::of(manifest) {
            live.extend(
                self.volumes
                    .iter()
                    .filter(|volume| volume.kind != "manifest")
                    .map(|volume| volume.name.clone()),
            );
        }
        live
    }

    /// What this record says about one volume, for the columns a manifest read cannot supply.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&DeclaredVolume> {
        self.volumes.iter().find(|volume| volume.name == name)
    }
}

/// Where one project target's record lives.
#[must_use]
pub fn record_path(state_root: &Path, project_id: &str, target: &str) -> PathBuf {
    state_root
        .join("projects")
        .join(project_id)
        .join(target)
        .join(VOLUMES_FILE)
}

/// Reads the record, treating an absent or empty file as "nothing is remembered".
///
/// # Errors
///
/// Returns [`RegistryError`] when the file cannot be read (`74`) or does not parse (`78`). Absent
/// is neither: a project that has never started has no record and that is not a defect.
pub fn read(state_root: &Path, project_id: &str, target: &str) -> Result<Record, RegistryError> {
    let path = record_path(state_root, project_id, target);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Record::default()),
        Err(error) => {
            return Err(RegistryError::io(
                "unreadable",
                path.clone(),
                format!("could not read the volume record `{}`", path.display()),
                error,
                false,
            ));
        }
    };
    parse(&source, &path)
}

/// Parses record text. Touches no filesystem.
///
/// # Errors
///
/// Returns [`RegistryError`] for malformed TOML or a key holding the wrong type, both `78`.
pub fn parse(source: &str, path: &Path) -> Result<Record, RegistryError> {
    if source.trim().is_empty() {
        return Ok(Record::default());
    }
    let document = DeTable::parse(source).map_err(|error| {
        fault(
            "syntax",
            format!("the volume record `{}` is not valid TOML", path.display()),
            error.message().to_owned(),
        )
    })?;
    let root = document.get_ref();

    // Absent and wrong-typed are two answers, never one. A key this file cannot read is a record it
    // cannot vouch for, and the consumer of the failure is `viv volume prune`: a `Composition` that
    // silently defaulted a corrupted `image` or dropped a non-string `pieces` element would no
    // longer equal the manifest's, [`Record::live`] would retire every remembered piece-declared
    // volume, and prune would classify their images as orphans and delete them. So the direction of
    // failure is fixed at `78` here rather than at a deletion later.
    let string =
        |table: &toml::de::DeTable<'_>, key: &str| -> Result<Option<String>, RegistryError> {
            match table.get(key).map(Spanned::get_ref) {
                None => Ok(None),
                Some(DeValue::String(text)) => Ok(Some(text.clone().into_owned())),
                Some(_) => Err(wrong_type(path, key, "a string")),
            }
        };
    let strings =
        |table: &toml::de::DeTable<'_>, key: &str| -> Result<Vec<String>, RegistryError> {
            match table.get(key).map(Spanned::get_ref) {
                None => Ok(Vec::new()),
                Some(DeValue::Array(items)) => items
                    .iter()
                    .map(|item| match item.get_ref() {
                        DeValue::String(text) => Ok(text.clone().into_owned()),
                        _ => Err(wrong_type(path, key, "an array of strings")),
                    })
                    .collect(),
                Some(_) => Err(wrong_type(path, key, "an array of strings")),
            }
        };

    let composition = Composition {
        image: string(root, "image")?.unwrap_or_default(),
        pieces: strings(root, "pieces")?,
        // An absent `extends` and one recorded as the empty string mean the same thing, because
        // TOML has no way to write `None` and this file is never hand-edited.
        extends: string(root, "extends")?.filter(|value| !value.is_empty()),
    };

    let mut volumes = Vec::new();
    if let Some(DeValue::Array(entries)) = root.get("volumes").map(Spanned::get_ref) {
        for entry in entries {
            let DeValue::Table(table) = entry.get_ref() else {
                return Err(fault(
                    "type",
                    format!("the volume record `{}` is malformed", path.display()),
                    "each `[[volumes]]` entry must be a table".to_owned(),
                ));
            };
            let (Some(name), Some(mount), Some(declared_by), Some(kind)) = (
                string(table, "name")?,
                string(table, "mount")?,
                string(table, "declared_by")?,
                string(table, "kind")?,
            ) else {
                return Err(fault(
                    "missing-key",
                    format!("the volume record `{}` is malformed", path.display()),
                    "a `[[volumes]]` entry needs `name`, `mount`, `declared_by`, and `kind`"
                        .to_owned(),
                ));
            };
            let size_gib = match table.get("size_gib").map(Spanned::get_ref) {
                None => None,
                Some(DeValue::Integer(number)) => Some(
                    u32::from_str_radix(number.as_str(), number.radix()).map_err(|_| {
                        wrong_type(path, "size_gib", "a ceiling that fits in 32 bits")
                    })?,
                ),
                Some(_) => return Err(wrong_type(path, "size_gib", "an integer")),
            };
            volumes.push(DeclaredVolume {
                name,
                mount,
                declared_by,
                kind,
                size_gib,
            });
        }
    }

    Ok(Record {
        composition,
        volumes,
    })
}

/// Publishes the record for one project target.
///
/// Takes no lock of its own. Every writer already holds the per-target `flock`, which is the lock
/// ADR-0053 assigns to per-target state; taking a second one here would add a link to that order
/// for a file only one process per target ever writes.
///
/// # Errors
///
/// Returns [`RegistryError`] when the directory cannot be created, or the replacement cannot be
/// staged, flushed, or published.
pub fn write(
    state_root: &Path,
    project_id: &str,
    target: &str,
    record: &Record,
) -> Result<(), RegistryError> {
    let destination = record_path(state_root, project_id, target);
    let directory = destination
        .parent()
        .ok_or_else(|| {
            fault(
                "write",
                "the volume record has no parent directory",
                destination.display().to_string(),
            )
        })?
        .to_path_buf();
    fs::create_dir_all(&directory).map_err(|source| {
        RegistryError::io(
            "write",
            directory.clone(),
            format!(
                "could not create the project state directory `{}`",
                directory.display()
            ),
            source,
            true,
        )
    })?;

    let (temporary, mut file) = create_temp_file(&directory, VOLUMES_FILE, PRIVATE_FILE_MODE)
        .map_err(|stage| {
            stage_fault(
                "write-temporary",
                "could not stage the volume record",
                stage,
            )
        })?;
    let result = (|| {
        file.write_all(render(record).as_bytes())
            .map_err(|source| {
                write_io(
                    "write",
                    &temporary,
                    "could not write the staged volume record",
                    source,
                )
            })?;
        file.sync_all().map_err(|source| {
            write_io(
                "write-sync",
                &temporary,
                "could not flush the staged volume record",
                source,
            )
        })?;
        drop(file);
        rename_with(&temporary, &destination, RenameFlags::empty()).map_err(|source| {
            write_io(
                "publish",
                &destination,
                "could not publish the volume record",
                source,
            )
        })?;
        sync_directory(&directory).map_err(|fault| {
            write_io_at(
                "directory-sync",
                fault,
                "could not flush the project state directory",
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn render(record: &Record) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "image = {}", escape(&record.composition.image));
    out.push_str("pieces = [");
    for (index, piece) in record.composition.pieces.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, " {}", escape(piece));
    }
    out.push_str(if record.composition.pieces.is_empty() {
        "]\n"
    } else {
        " ]\n"
    });
    if let Some(extends) = &record.composition.extends {
        let _ = writeln!(out, "extends = {}", escape(extends));
    }
    for volume in &record.volumes {
        out.push_str("\n[[volumes]]\n");
        let _ = writeln!(out, "name = {}", escape(&volume.name));
        let _ = writeln!(out, "mount = {}", escape(&volume.mount));
        let _ = writeln!(out, "declared_by = {}", escape(&volume.declared_by));
        let _ = writeln!(out, "kind = {}", escape(&volume.kind));
        if let Some(size) = volume.size_gib {
            let _ = writeln!(out, "size_gib = {size}");
        }
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A key holding something other than what the renderer writes there.
fn wrong_type(path: &Path, key: &str, expected: &str) -> RegistryError {
    fault(
        "type",
        format!("the volume record `{}` is malformed", path.display()),
        format!("`{key}` must be {expected}"),
    )
}

fn fault(
    condition: &'static str,
    message: impl Into<String>,
    why: impl Into<String>,
) -> RegistryError {
    RegistryError::plain(
        RegistryErrorKind::Config,
        condition,
        LOCUS,
        message,
        why.into(),
    )
}

fn stage_fault(condition: &'static str, message: &str, stage: StageFault) -> RegistryError {
    match stage {
        StageFault::Io(io) => write_io_at(condition, io, message),
        StageFault::Exhausted(path) => RegistryError::plain(
            RegistryErrorKind::Io,
            condition,
            Locus::File(path),
            message.to_owned(),
            "no unique temporary name was available in the project state directory",
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
        message.to_owned(),
        source,
        true,
    )
}

/// The mode the project state directory carries, kept beside the file that needs it.
const _: u32 = PRIVATE_DIR_MODE;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn manifest(image: &str, pieces: &[&str], volumes: &[(&str, &str)]) -> Manifest {
        Manifest {
            image: image.to_owned(),
            pieces: pieces.iter().map(|piece| (*piece).to_owned()).collect(),
            volumes: volumes
                .iter()
                .map(|(name, mount)| super::super::manifest::Volume {
                    name: (*name).to_owned(),
                    mount: (*mount).to_owned(),
                    size_gib: None,
                })
                .collect(),
            ..Manifest::default()
        }
    }

    fn declared(name: &str, kind: &str) -> DeclaredVolume {
        DeclaredVolume {
            name: name.to_owned(),
            mount: format!("/mnt/{name}"),
            declared_by: "some-layer".to_owned(),
            kind: kind.to_owned(),
            size_gib: Some(20),
        }
    }

    /// A well-formed record head plus one `[[volumes]]` entry, for the malformed-value cases.
    const VOLUME_ROW: &str = concat!(
        "image = \"rust\"\npieces = []\n\n",
        "[[volumes]]\nname = \"c\"\nmount = \"/m\"\ndeclared_by = \"p\"\nkind = \"piece\"\n",
    );

    #[test]
    fn an_absent_record_is_empty_and_a_malformed_one_is_a_config_error() {
        assert_eq!(parse("", Path::new("/v")).unwrap(), Record::default());
        assert_eq!(parse("  \n", Path::new("/v")).unwrap(), Record::default());
        assert!(parse("image = ", Path::new("/v")).is_err());
        assert!(parse("[[volumes]]\nname = \"c\"\n", Path::new("/v")).is_err());
    }

    #[test]
    fn a_wrong_typed_key_is_refused_rather_than_defaulted() {
        // The failure direction that matters, and the reason these are errors rather than
        // fallbacks: a record whose `image` or `pieces` quietly read as empty describes a
        // composition no manifest matches, `live` then vouches for none of the remembered
        // volumes, and `viv volume prune` deletes their images as orphans. Refusing at `78`
        // costs a diagnostic; defaulting costs the user's data.
        for source in [
            "image = 1\npieces = []\n",
            "image = \"rust\"\npieces = 1\n",
            "image = \"rust\"\npieces = [\"git\", 2]\n",
            "image = \"rust\"\npieces = []\nextends = true\n",
            // A ceiling that is a string, and one that overflows `u32`. Both used to read as
            // "no ceiling declared", which lists as `null` rather than as the defect it is.
            &format!("{VOLUME_ROW}size_gib = \"20\"\n"),
            &format!("{VOLUME_ROW}size_gib = 4294967296\n"),
        ] {
            let error = parse(source, Path::new("/v"));
            assert!(error.is_err(), "accepted: {source}");
            assert_eq!(
                error.unwrap_err().exit_code(),
                crate::exit::ExitKind::Config,
                "{source}"
            );
        }
    }

    /// Pins the rendered text as the on-disk contract, then proves the reader
    /// accepts that literal — rather than a render-parse loop, which would hold
    /// even if writer and reader drifted from the format together.
    #[test]
    fn a_record_renders_the_pinned_text_and_the_parser_accepts_it() {
        let record = Record {
            composition: Composition {
                image: "rust".to_owned(),
                pieces: vec!["git".to_owned(), "ssh-agent".to_owned()],
                extends: Some("./module.nix".to_owned()),
            },
            volumes: vec![declared("cache", "piece"), {
                let mut volume = declared("build", "manifest");
                volume.size_gib = None;
                volume
            }],
        };
        let pinned = concat!(
            "image = \"rust\"\n",
            "pieces = [ \"git\", \"ssh-agent\" ]\n",
            "extends = \"./module.nix\"\n",
            "\n[[volumes]]\n",
            "name = \"cache\"\n",
            "mount = \"/mnt/cache\"\n",
            "declared_by = \"some-layer\"\n",
            "kind = \"piece\"\n",
            "size_gib = 20\n",
            "\n[[volumes]]\n",
            "name = \"build\"\n",
            "mount = \"/mnt/build\"\n",
            "declared_by = \"some-layer\"\n",
            "kind = \"manifest\"\n",
        );
        assert_eq!(render(&record), pinned);
        assert_eq!(parse(pinned, Path::new("/v")).unwrap(), record);
    }

    #[test]
    fn the_manifest_is_believed_immediately_and_the_record_only_while_the_layers_hold() {
        let record = Record {
            composition: Composition {
                image: "rust".to_owned(),
                pieces: vec!["cache-piece".to_owned()],
                extends: None,
            },
            volumes: vec![
                declared("from-piece", "piece"),
                declared("from-toml", "manifest"),
            ],
        };

        // Same composition: the piece's volume is still believed, and so is anything the manifest
        // declares right now.
        let unchanged = manifest("rust", &["cache-piece"], &[("fresh", "/mnt/fresh")]);
        let live = record.live(&unchanged);
        assert!(live.contains("from-piece"), "{live:?}");
        assert!(live.contains("fresh"), "{live:?}");

        // A `[[volumes]]` entry dropped from the manifest orphans at once. Without the
        // `kind != manifest` filter the stale row below would keep vouching for it, and a
        // declaration removed on purpose would never become reclaimable.
        assert!(!live.contains("from-toml"), "{live:?}");

        // A dropped piece retires every remembered row, including volumes other pieces declared:
        // the record describes a composition that no longer exists, so none of it is evidence.
        let dropped = manifest("rust", &[], &[]);
        assert!(record.live(&dropped).is_empty());

        // And so does a changed image, for the same reason.
        let rebased = manifest("go", &["cache-piece"], &[]);
        assert!(record.live(&rebased).is_empty());
    }

    #[test]
    fn a_project_that_never_started_has_no_record_and_that_is_not_an_error() {
        let scratch = crate::test_support::ScratchDirectory::new().unwrap();
        let root = scratch.path().join("never-started");
        assert_eq!(read(&root, "p", "default").unwrap(), Record::default());

        let record = Record {
            composition: Composition {
                image: "rust".to_owned(),
                pieces: Vec::new(),
                extends: None,
            },
            volumes: vec![declared("cache", "piece")],
        };
        write(&root, "p", "default", &record).unwrap();
        assert_eq!(read(&root, "p", "default").unwrap(), record);
    }
}
