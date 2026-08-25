//! What volumes a project has, and removing the ones no layer declares.
//!
//! `list` and `prune` are one enumeration and two renderings of it. ADR-0067 is explicit that
//! prune "introduces no new predicate: its candidates are exactly the orphans spec/06 defines and
//! `volume list` surfaces", so the orphan test exists once, here, and both verbs read the same
//! rows. A second notion of "unused" is the failure that decision exists to prevent.
//!
//! Neither verb evaluates anything. spec/14 gives both a single failure column — `78`, no manifest
//! bound — while a Nix evaluation can answer `65`, `69`, `70`, or `74`, so provenance comes from
//! the record `viv start` leaves behind (see [`crate::config::volumes`]) and sizes come from one
//! `stat` of each image.

use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::grammar::Output;
use super::{Context, Failure, Success, diagnosed, lifecycle, render};
use crate::config::{self, Environment, volumes::Record};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;

/// The reserved name of the home volume every project has (spec/06, spec/02).
pub(super) const DEFAULT_VOLUME: &str = "default";

/// The undeclared volume backing the guest store's writable layer (spec/06, ADR-0087).
pub(super) const STORE_VOLUME: &str = "store";

/// Where the home volume mounts inside the guest.
///
/// A second spelling of a guest fact, and the only kind this file has. `nix/guest.nix` owns the
/// value; `tests/nix/contract.sh` compares this pair against the guest's own fstab, so the copy is
/// checked against the original rather than left to drift. The alternative — reading it out of the
/// last build's launch contract — would drag a build into a reader spec/14 gives one failure mode.
const DEFAULT_VOLUME_MOUNT: &str = "/home/vivarium";

/// Where the store volume's writable layer mounts (ADR-0087, `nix/store-layout.nix`).
const STORE_VOLUME_MOUNT: &str = "/nix/.rw-store";

/// The suffix every volume image carries under `volumes/`.
const IMAGE_SUFFIX: &str = ".img";

/// Which arm of the enumeration produced a row.
///
/// `orphan` is derived from this rather than tested for by name, which is what keeps the reserved
/// exclusion structural: see [`enumerate`].
#[derive(Clone, Debug, Eq, PartialEq)]
enum Origin {
    /// One of the two volumes that exist without ever being declared.
    Undeclared,
    /// A layer declares it. The layer's name, when the record remembers which.
    Declared(Option<String>),
    /// An image on disk that no current layer declares (spec/06).
    Orphan,
}

/// One volume, as both verbs see it.
#[derive(Clone, Debug)]
struct Row {
    name: String,
    /// The guest path, or `None` for an orphan whose mount nothing remembers.
    mount: Option<String>,
    origin: Origin,
    /// The image, when one exists on disk.
    image: Option<PathBuf>,
    /// What the sparse image occupies, `0` when it has never been created.
    allocated_bytes: u64,
    /// The declared ceiling, or `None` when no layer declared one and no image exists to measure.
    virtual_bytes: Option<u64>,
}

impl Row {
    const fn is_orphan(&self) -> bool {
        matches!(self.origin, Origin::Orphan)
    }

    fn declared_by(&self) -> Option<&str> {
        match &self.origin {
            Origin::Declared(layer) => layer.as_deref(),
            // spec/01 fixes `null` for the home volume and the store's: they are not declared, so
            // there is no layer to name. An orphan's declaration is gone by definition.
            Origin::Undeclared | Origin::Orphan => None,
        }
    }
}

/// `viv volume list` (spec/01, ADR-0019).
pub(super) fn list<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let (rows, _) = survey(context)?;
    Ok(Success::plain(if output.is_json() {
        render::volume_list_json(&rows_json(&rows))
    } else {
        render::volume_list_human(&rows_human(&rows), context.ui.palette_out())
    }))
}

/// `viv volume prune` (ADR-0067, spec/01).
pub(super) fn prune<E: Environment>(
    context: &Context<'_, E>,
    dry_run: bool,
    yes: bool,
    output: Output,
) -> Result<Success, Failure> {
    let (rows, sandbox_id) = survey(context)?;

    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let runtime = lifecycle::Runtime::locate(&runtime_root, &sandbox_id, super::DEFAULT_TARGET)?;

    // The lock comes first, before the state is read rather than after (ADR-0053). `viv start`
    // takes this same lock to boot and releases it once the guest is up, so a check made outside
    // it answers about a moment that is over: a start beginning right after an unlocked check
    // would boot, release, and leave this prune deleting the images of a running VM. Held from
    // here through the removals, the check below is decisive for as long as its answer is used.
    let _lock = lifecycle::TargetLock::acquire(&runtime)?;

    // spec/01 and ADR-0067: removal refuses while the VM runs, and the user clears it in one step.
    // Checked before the candidates are computed so a running VM is refused even when there is
    // nothing to remove — the answer is about the project's state, not about this run's luck.
    let built = lifecycle::last_build(&context.roots, &sandbox_id, super::DEFAULT_TARGET).is_some();
    if !matches!(
        lifecycle::discriminate(&runtime, built),
        lifecycle::State::Absent | lifecycle::State::Built
    ) {
        return Err(diagnosed(
            Namespace::Vm,
            "volume-in-use",
            "this project's VM is running",
            Locus::Named("volume store"),
            "a volume attached to a running guest cannot be removed",
            ExitKind::TempFail,
        )
        .with_hint("`viv stop` first, then `viv volume prune`"));
    }

    let candidates: Vec<Row> = rows.into_iter().filter(Row::is_orphan).collect();
    let record = |rows: &[Row], reclaimed: u64| {
        Ok(Success::plain(if output.is_json() {
            render::volume_prune_json(&prune_rows_json(rows), reclaimed)
        } else {
            render::volume_prune_human(&prune_rows_human(rows), reclaimed)
        }))
    };

    // spec/14: nothing to prune is `0`, never an error — it is a fact about the project rather
    // than a failure. And nothing to approve, so nothing is asked.
    if candidates.is_empty() || dry_run {
        let previewed = candidates.iter().map(|row| row.allocated_bytes).sum();
        return record(&candidates, if dry_run { previewed } else { 0 });
    }

    if !yes && !confirm_prune(context, &candidates)? {
        return record(&[], 0);
    }

    let mut reclaimed = 0;
    for row in &candidates {
        let Some(image) = &row.image else { continue };
        std::fs::remove_file(image).map_err(|source| removal_failure(image, &source))?;
        reclaimed += row.allocated_bytes;
    }
    record(&candidates, reclaimed)
}

/// The two disk sums `status` reports: what one sandbox's images occupy, against their apparent
/// sizes.
///
/// Only images on disk are summed — a declared ceiling with no image occupies nothing yet, and
/// arms 1 and 2 of [`enumerate`] contribute no image — so the pair is exactly `viv volume list`'s
/// arm-3 reading joined, never a second measurement (spec/17). A sandbox with no images reports
/// `0`, which is a reading, not an absence.
pub(super) fn disk_totals(
    roots: &config::XdgRoots,
    sandbox_id: &str,
) -> Result<(u64, u64), Failure> {
    let directory = lifecycle::volume_directory(roots, sandbox_id, super::DEFAULT_TARGET);
    let mut allocated: u64 = 0;
    let mut apparent: u64 = 0;
    for image in images_in(&directory)? {
        let metadata = std::fs::metadata(&image).map_err(|source| stat_failure(&image, &source))?;
        allocated += metadata.blocks() * 512;
        apparent += metadata.len();
    }
    Ok((allocated, apparent))
}

/// The binding and rows — the part `list` and `prune` share exactly.
fn survey<E: Environment>(context: &Context<'_, E>) -> Result<(Vec<Row>, String), Failure> {
    // spec/01: both read the project's own state and both need a bound manifest, failing closed
    // with `78` when none resolves. No Nix runs, which is what keeps that the only failure.
    let resolved = super::resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name.clone();

    let record = config::volumes::read(&context.roots.state, &sandbox_id, super::DEFAULT_TARGET)
        .map_err(|error| super::registry_failure(&error))?;
    let directory = lifecycle::volume_directory(&context.roots, &sandbox_id, super::DEFAULT_TARGET);
    // The manifest's own artifact name, which is the identity `src/config/flake.rs` gives the
    // manifest layer in the generated `layers` list — so a volume this reader attributes to the
    // manifest carries the same string the record would carry after the next `viv start`.
    let rows = enumerate(
        &directory,
        &record,
        &resolved.manifest,
        &resolved.selected.name,
    )?;
    Ok((rows, sandbox_id))
}

/// Every volume this project has, in three arms whose order is the whole reserved exclusion.
///
/// A name inserted by arm 1 or arm 2 is already present when arm 3 walks the directory, so arm 3
/// cannot emit it — spec/06's "the default home volume and the store volume are excluded by
/// construction" held as a property of insertion order rather than as an `if name == "default"`
/// somewhere in a predicate, which is the kind of special case that drifts.
fn enumerate(
    directory: &Path,
    record: &Record,
    manifest: &config::Manifest,
    manifest_layer: &str,
) -> Result<Vec<Row>, Failure> {
    let mut rows: Vec<Row> = Vec::new();

    // Arm 1: the two that exist without being declared.
    for (name, mount) in [
        (DEFAULT_VOLUME, DEFAULT_VOLUME_MOUNT),
        (STORE_VOLUME, STORE_VOLUME_MOUNT),
    ] {
        rows.push(Row {
            name: name.to_owned(),
            mount: Some(mount.to_owned()),
            origin: Origin::Undeclared,
            image: None,
            allocated_bytes: 0,
            virtual_bytes: None,
        });
    }

    // Arm 2: everything a current layer declares, by the freshness rule the record owns.
    let live: BTreeSet<String> = record.live(manifest);
    for name in &live {
        if rows.iter().any(|row| &row.name == name) {
            continue;
        }
        let declaration = record.find(name);
        let leaf = manifest.volumes.iter().find(|volume| &volume.name == name);
        rows.push(Row {
            name: name.clone(),
            mount: declaration
                .map(|volume| volume.mount.clone())
                .or_else(|| leaf.map(|volume| volume.mount.clone())),
            // The record names the declaring layer when it remembers one, because only a merge can
            // tell a piece's declaration from the manifest's. When it does not — a project that has
            // never started, or a `[[volumes]]` entry added since the last one — the manifest's own
            // text is proof enough that the manifest declares it, so it is named. spec/01 reserves
            // `null` for the two volumes no layer declares at all; leaving it null here would make
            // a fresh project's declared volume indistinguishable from the reserved pair.
            origin: Origin::Declared(
                declaration
                    .map(|volume| volume.declared_by.clone())
                    .or_else(|| leaf.map(|_| manifest_layer.to_owned())),
            ),
            image: None,
            allocated_bytes: 0,
            virtual_bytes: declaration
                .and_then(|volume| volume.size_gib)
                .or_else(|| leaf.and_then(|volume| volume.size_gib))
                .map(gib_to_bytes),
        });
    }

    // Arm 3: images on disk, which both fills in the sizes above and finds what nothing declares.
    for image in images_in(directory)? {
        let Some(name) = image
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(IMAGE_SUFFIX))
        else {
            continue;
        };
        let metadata = std::fs::metadata(&image).map_err(|source| stat_failure(&image, &source))?;
        // `st_blocks` is defined in 512-byte units by POSIX whatever the filesystem's block size,
        // so this is the sparse image's real occupancy rather than its apparent length. The
        // apparent length is the ceiling actually in force: the supervisor creates the image with
        // `truncate -s <sizeMiB>M`, and spec/06 lets a ceiling be raised between boots.
        let allocated_bytes = metadata.blocks() * 512;
        let virtual_bytes = metadata.len();
        if let Some(row) = rows.iter_mut().find(|row| row.name == name) {
            row.image = Some(image);
            row.allocated_bytes = allocated_bytes;
            row.virtual_bytes = Some(virtual_bytes);
            continue;
        }
        rows.push(Row {
            name: name.to_owned(),
            // spec/01: an orphan's mount is "the guest path the volume used to occupy, carried
            // from the state that outlived the declaration". Absent when no start ever recorded
            // one, which is the honest answer rather than a guess.
            mount: record.find(name).map(|volume| volume.mount.clone()),
            origin: Origin::Orphan,
            image: Some(image),
            allocated_bytes,
            virtual_bytes: Some(virtual_bytes),
        });
    }

    Ok(rows)
}

/// The `<name>.img` entries under a project's volume directory, sorted, absent directory included.
fn images_in(directory: &Path) -> Result<Vec<PathBuf>, Failure> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        // A project that has never started has no volume directory, which is not a defect.
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(read_failure(directory, &source)),
    };
    let mut images = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| read_failure(directory, &source))?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "img") && path.is_file() {
            images.push(path);
        }
    }
    // Sorted so two runs over one directory render the same order; `read_dir` promises none.
    images.sort();
    Ok(images)
}

fn confirm_prune<E: Environment>(
    context: &Context<'_, E>,
    candidates: &[Row],
) -> Result<bool, Failure> {
    // spec/01 makes this normative rather than cosmetic: "the human confirmation prompt renders
    // those same rows, so what a user approves and what a script reads are one thing". So the rows
    // come from the same renderer the success output uses.
    let question = super::prompt::Question {
        headline: &format!("remove {} orphaned volume image(s)?", candidates.len()),
        scope: prune_rows_human(candidates),
        spared: Vec::new(),
    };
    super::prompt::confirm(
        &question,
        context.ui.palette(),
        &mut std::io::stdin().lock(),
        &mut std::io::stderr().lock(),
    )
    .map_err(|source| {
        let _ = context;
        diagnosed(
            Namespace::Host,
            "prompt-unavailable",
            "could not ask for confirmation",
            Locus::Named("terminal"),
            source.to_string(),
            ExitKind::IoErr,
        )
    })
}

fn rows_json(rows: &[Row]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            serde_json::json!({
                "name": row.name,
                "mount": row.mount,
                "declared_by": row.declared_by(),
                "orphan": row.is_orphan(),
                "allocated_bytes": row.allocated_bytes,
                "virtual_bytes": row.virtual_bytes,
            })
        })
        .collect()
}

fn rows_human(rows: &[Row]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| {
            vec![
                row.name.clone(),
                row.mount.clone().unwrap_or_else(|| "-".to_owned()),
                row.declared_by().unwrap_or("-").to_owned(),
                if row.is_orphan() { "orphan" } else { "-" }.to_owned(),
                row.allocated_bytes.to_string(),
                row.virtual_bytes
                    .map_or_else(|| "-".to_owned(), |bytes| bytes.to_string()),
            ]
        })
        .collect()
}

fn prune_rows_json(rows: &[Row]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            serde_json::json!({
                "name": row.name,
                "mount": row.mount,
                "allocated_bytes": row.allocated_bytes,
            })
        })
        .collect()
}

fn prune_rows_human(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}\t{}\t{}",
                row.name,
                row.mount.clone().unwrap_or_else(|| "-".to_owned()),
                row.allocated_bytes
            )
        })
        .collect()
}

const fn gib_to_bytes(gib: u32) -> u64 {
    gib as u64 * 1024 * 1024 * 1024
}

/// spec/14 splits every channel failure in this file the same way: permission is `77` and
/// everything else is `74`. Kept in one place so the three sites below cannot answer differently
/// for the same `errno`.
fn channel_code(source: &std::io::Error) -> ExitKind {
    if source.kind() == std::io::ErrorKind::PermissionDenied {
        ExitKind::NoPerm
    } else {
        ExitKind::IoErr
    }
}

fn stat_failure(path: &Path, source: &std::io::Error) -> Failure {
    diagnosed(
        Namespace::State,
        "volume-unreadable",
        "could not measure a volume image",
        Locus::File(path.to_path_buf()),
        source.to_string(),
        channel_code(source),
    )
}

fn read_failure(path: &Path, source: &std::io::Error) -> Failure {
    diagnosed(
        Namespace::State,
        "volume-directory-unreadable",
        "could not read the project's volume directory",
        Locus::File(path.to_path_buf()),
        source.to_string(),
        channel_code(source),
    )
}

fn removal_failure(path: &Path, source: &std::io::Error) -> Failure {
    let code = channel_code(source);
    diagnosed(
        Namespace::State,
        "volume-removal",
        "could not remove a volume image",
        Locus::File(path.to_path_buf()),
        source.to_string(),
        code,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::volumes::{Composition, DeclaredVolume};
    use crate::test_support::ScratchDirectory;

    fn scratch() -> ScratchDirectory {
        ScratchDirectory::new().unwrap()
    }

    fn image(directory: &Path, name: &str, bytes: u64) {
        let file = std::fs::File::create(directory.join(format!("{name}.img"))).unwrap();
        file.set_len(bytes).unwrap();
    }

    fn record_with(volumes: Vec<DeclaredVolume>) -> Record {
        Record {
            composition: Composition::default(),
            volumes,
        }
    }

    #[test]
    fn the_reserved_volumes_are_never_orphans_even_when_nothing_is_declared() {
        // The case the insertion order exists for: images on disk for both reserved names, an
        // empty declaration set, and a third image nothing knows about. A predicate that tested
        // for "no layer declares it" without the arms would flag all three.
        let directory = scratch();
        image(directory.path(), DEFAULT_VOLUME, 4096);
        image(directory.path(), STORE_VOLUME, 4096);
        image(directory.path(), "old", 4096);

        let rows = enumerate(
            directory.path(),
            &record_with(Vec::new()),
            &config::Manifest::default(),
            "demo",
        )
        .unwrap();

        let orphans: Vec<&str> = rows
            .iter()
            .filter(|row| row.is_orphan())
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(orphans, vec!["old"]);
        // And both keep the mount and the `null` declaring layer spec/01 fixes for them.
        let home = rows.iter().find(|row| row.name == DEFAULT_VOLUME).unwrap();
        assert_eq!(home.mount.as_deref(), Some(DEFAULT_VOLUME_MOUNT));
        assert_eq!(home.declared_by(), None);
    }

    #[test]
    fn a_declared_volume_with_no_image_lists_at_zero_and_is_not_an_orphan() {
        // spec/01, verbatim: "A project whose volumes have never been created still lists its
        // declared volumes with `allocated_bytes` of `0`."
        let directory = scratch();
        let record = record_with(vec![DeclaredVolume {
            name: "cache".to_owned(),
            mount: "/var/cache/project".to_owned(),
            declared_by: "cache-piece".to_owned(),
            kind: "piece".to_owned(),
            size_gib: Some(20),
        }]);
        let rows = enumerate(
            directory.path(),
            &record,
            &config::Manifest::default(),
            "demo",
        )
        .unwrap();

        let cache = rows.iter().find(|row| row.name == "cache").unwrap();
        assert!(!cache.is_orphan());
        assert_eq!(cache.allocated_bytes, 0);
        assert_eq!(cache.declared_by(), Some("cache-piece"));
        assert_eq!(cache.virtual_bytes, Some(gib_to_bytes(20)));
    }

    #[test]
    fn a_manifest_declaration_the_record_has_not_seen_names_the_manifest() {
        // The never-started project, and the `[[volumes]]` entry added since the last start. Both
        // reach `enumerate` with a leaf declaration and no record row, and spec/01 reserves a null
        // `declared_by` for the two volumes no layer declares — so naming the manifest is the only
        // answer that does not report a declared volume as undeclared.
        let directory = scratch();
        let manifest = config::Manifest {
            volumes: vec![config::Volume {
                name: "cache".to_owned(),
                mount: "/var/cache/project".to_owned(),
                size_gib: Some(20),
            }],
            ..config::Manifest::default()
        };
        let rows = enumerate(
            directory.path(),
            &record_with(Vec::new()),
            &manifest,
            "demo",
        )
        .unwrap();

        let cache = rows.iter().find(|row| row.name == "cache").unwrap();
        assert!(!cache.is_orphan());
        assert_eq!(cache.declared_by(), Some("demo"));
        assert_eq!(cache.mount.as_deref(), Some("/var/cache/project"));
        assert_eq!(cache.virtual_bytes, Some(gib_to_bytes(20)));
        // And the reserved pair keeps its null, which is what the row above must not look like.
        let home = rows.iter().find(|row| row.name == DEFAULT_VOLUME).unwrap();
        assert_eq!(home.declared_by(), None);
    }

    #[test]
    fn a_sparse_image_reports_what_it_occupies_and_not_what_it_reserves() {
        // The distinction ADR-0037 turns on: a 1 GiB hole costs nothing on disk, and reading
        // `len()` for both columns would report a project as using a gibibyte it has not touched.
        let directory = scratch();
        image(directory.path(), DEFAULT_VOLUME, 1024 * 1024 * 1024);
        let rows = enumerate(
            directory.path(),
            &record_with(Vec::new()),
            &config::Manifest::default(),
            "demo",
        )
        .unwrap();

        let home = rows.iter().find(|row| row.name == DEFAULT_VOLUME).unwrap();
        assert_eq!(home.virtual_bytes, Some(1024 * 1024 * 1024));
        assert!(
            home.allocated_bytes < 1024 * 1024,
            "a hole occupied {} bytes",
            home.allocated_bytes
        );
    }

    #[test]
    fn an_absent_volume_directory_is_the_two_reserved_rows_and_no_error() {
        let rows = enumerate(
            Path::new("/nonexistent/vivarium/volumes"),
            &record_with(Vec::new()),
            &config::Manifest::default(),
            "demo",
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.allocated_bytes == 0));
    }
}
