//! `viv status -g` — the manifest-keyed fleet, enumerated read-only (spec/01, spec/17).
//!
//! A row per indexed manifest with retained state, each carrying its declared ceiling beside its
//! measured use, and the host's own reading beside the fleet. Enumeration reads the derived
//! workspace index (ADR-0107) and per-sandbox state the same readers the project-local report
//! uses; it removes nothing and warns rather than repairs (ADR-0054's surviving judgment). The
//! one write on this path is the index republication `workspace_index` performs, which spec/14
//! classifies as recomputable cache, not state.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::grammar::Output;
use super::lifecycle::{self, State};
use super::{Context, Failure, Success, render};
use crate::config::{self, Environment};

/// One enumerated sandbox, resolved without writing anything.
struct FleetRow {
    manifest: String,
    /// The expanded declared workspaces, manifest order; a source that cannot expand is omitted.
    workspaces: Vec<PathBuf>,
    /// The subset of `workspaces` whose directory no longer exists — named, never removed.
    path_missing: Vec<PathBuf>,
    state: State,
    stale: bool,
    generation: Option<u64>,
    uptime_seconds: Option<u64>,
    resources: Option<lifecycle::Resources>,
    readings: lifecycle::RuntimeReadings,
}

/// The host's own reading, present beside every fleet — including an empty one — because it is
/// what lets one command answer whether the host is overcommitted (spec/01).
struct HostReadings {
    mem_available_bytes: Option<u64>,
    mem_total_bytes: Option<u64>,
    pressure_some_avg60: Option<f64>,
}

/// The enumeration itself. Read-only; an empty fleet is a result, not a failure.
///
/// # Errors
///
/// Returns [`Failure`] for an unreadable manifest library or index (`74`) or an unusable
/// runtime root — the ordering mirrors ADR-0109: the user's declarations first, the host after.
/// Per-row readings never fail the enumeration; each degrades to its own `null`.
pub(super) async fn report<E: Environment + Sync>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let index = super::workspace_index(context, true)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;

    let mut rows = Vec::new();
    for indexed in index.manifests() {
        if let Some(row) = row(context, &runtime_root, indexed).await? {
            rows.push(row);
        }
    }
    let host = host_readings();

    let missing = missing_workspaces(&rows);
    let stdout = if output.is_json() {
        let projects: Vec<Value> = rows.iter().map(row_json).collect();
        render::fleet_json(&projects, &host_json(&host))
    } else {
        render::fleet_human(
            &rows_human(&rows),
            &host_line(&host),
            context.ui.palette_out(),
        )
    };
    Ok(Success {
        stdout,
        // The warning goes beside the result, not into it: spec/01 keeps stdout clean for
        // `--json | jq` and requires each affected declaration named on stderr.
        notes: missing_note(&missing),
    })
}

/// One sandbox's row, or `None` for a manifest that is configuration only.
///
/// A manifest with neither retained state nor runtime records has never been a sandbox — it is
/// visible in `viv manifest list`, not here (spec/01's enumeration domain).
async fn row<E: Environment + Sync>(
    context: &Context<'_, E>,
    runtime_root: &Path,
    indexed: &config::IndexedManifest,
) -> Result<Option<FleetRow>, Failure> {
    let sandbox_id = &indexed.name;
    let target = super::DEFAULT_TARGET;
    let runtime = lifecycle::Runtime::for_report(runtime_root, sandbox_id, target);
    let state_dir = context.roots.state.join("projects").join(sandbox_id);
    if !state_dir.is_dir() && !runtime.directory.is_dir() {
        return Ok(None);
    }

    let recorded = lifecycle::last_build(&context.roots, sandbox_id, target);
    let state = lifecycle::discriminate(&runtime, recorded.is_some());
    let running = matches!(state, State::Running | State::Stopping);
    let record = if running {
        lifecycle::read_boot_record(&runtime)
    } else {
        lifecycle::BootRecord::Absent
    };
    let readings =
        lifecycle::runtime_readings(context, &runtime, sandbox_id, running, &record).await;

    // The ceiling: a running row reports what its launch recorded (`null` when that record is
    // unreadable — Q-015's exit). A resting row reports the ceiling resolved from the
    // manifest's own `[resources]` table and the host by spec/17's arithmetic — the merged
    // `vivarium.resources` channel a piece may propose is applied by evaluation, and a
    // read-only enumeration runs none, so the declaration in reach is what is reported and
    // spec/01 says exactly that. A manifest that no longer parses resolves nothing, and
    // fabricating a ceiling for it would put a constant where a reader reads a declaration.
    let resources = if running {
        lifecycle::declared_resources(&runtime)
    } else {
        super::resolve_manifest(context, sandbox_id)
            .ok()
            .and_then(|selected| super::read_manifest(context, &selected).ok())
            .map(|(_, manifest)| lifecycle::effective_resources(manifest.resources.as_ref()))
    };

    let workspaces = super::expanded_workspace_sources(context, &indexed.workspace_sources);
    let path_missing = workspaces
        .iter()
        .filter(|path| !path.exists())
        .cloned()
        .collect();

    Ok(Some(FleetRow {
        manifest: sandbox_id.clone(),
        workspaces,
        path_missing,
        state,
        stale: matches!(state, State::Running)
            && lifecycle::running_build(&context.roots, sandbox_id, target).is_some_and(
                |launched| {
                    recorded
                        .as_ref()
                        .is_some_and(|current| *current != launched)
                },
            ),
        generation: config::generations::current_number(&lifecycle::generation_paths(
            &context.roots,
            sandbox_id,
            target,
        )),
        uptime_seconds: running
            .then(|| lifecycle::uptime_seconds(&runtime))
            .flatten(),
        resources,
        readings,
    }))
}

/// Every missing declared directory, paired with the manifest that declares it.
fn missing_workspaces(rows: &[FleetRow]) -> Vec<(String, PathBuf)> {
    rows.iter()
        .flat_map(|row| {
            row.path_missing
                .iter()
                .map(|path| (row.manifest.clone(), path.clone()))
        })
        .collect()
}

/// spec/01's warning: each declaration named, and why nothing was removed said out loud.
fn missing_note(missing: &[(String, PathBuf)]) -> String {
    if missing.is_empty() {
        return String::new();
    }
    let mut note = format!(
        "warning: {} declared workspace director{} no longer exist{}:\n",
        missing.len(),
        if missing.len() == 1 { "y" } else { "ies" },
        if missing.len() == 1 { "s" } else { "" },
    );
    for (manifest, path) in missing {
        let _ = writeln!(note, "           {manifest}  →  {}", path.display());
    }
    note.push_str(concat!(
        "         It may just be on an unmounted filesystem, so vivarium will not\n",
        "         remove or rewrite the manifest on its own.\n"
    ));
    note
}

/// One row, in the shape spec/01's `-g` record fixes.
fn row_json(row: &FleetRow) -> Value {
    json!({
        "manifest": row.manifest,
        "workspaces": row.workspaces.iter().map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "path_missing": row.path_missing.iter().map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "state": row.state.as_str(),
        "stale": row.stale,
        "generation": row.generation,
        "uptime_seconds": row.uptime_seconds,
        "resources": row.resources.as_ref().map(|resources| json!({
            "mem_mib": resources.mem_mib,
            "vcpu": resources.vcpu,
        })),
        "runtime": render::runtime_json(&row.readings),
    })
}

/// The `host` object, every field `null` where the host will not say (spec/01).
fn host_json(host: &HostReadings) -> Value {
    json!({
        "mem_available_bytes": host.mem_available_bytes,
        "mem_total_bytes": host.mem_total_bytes,
        "pressure_some_avg60": host.pressure_some_avg60,
    })
}

/// The fleet table's cells, header first — spec/17's reporting table.
fn rows_human(rows: &[FleetRow]) -> Vec<Vec<String>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let dash = || "-".to_owned();
    let mut table = vec![
        [
            "PROJECT",
            "STATE",
            "MEM (used/ceiling)",
            "VCPU",
            "DISK (alloc/virtual)",
            "SESS",
            "UP",
        ]
        .map(str::to_owned)
        .to_vec(),
    ];
    for row in rows {
        let used = row.readings.mem_used_bytes.map_or_else(dash, render::bytes);
        let ceiling = row.resources.as_ref().map_or_else(dash, |resources| {
            render::bytes(resources.mem_mib * 1024 * 1024)
        });
        let mut cells = vec![
            row.manifest.clone(),
            row.state.as_str().to_owned(),
            format!("{used} / {ceiling}"),
            row.resources
                .as_ref()
                .map_or_else(dash, |resources| resources.vcpu.to_string()),
            match (
                row.readings.disk_allocated_bytes,
                row.readings.disk_virtual_bytes,
            ) {
                (Some(allocated), Some(apparent)) => {
                    format!("{} / {}", render::bytes(allocated), render::bytes(apparent))
                }
                _ => "- / -".to_owned(),
            },
            row.readings
                .sessions
                .map_or_else(dash, |sessions| sessions.to_string()),
            row.uptime_seconds.map_or_else(dash, render::duration),
        ];
        if near_ceiling(
            row.readings.mem_used_bytes,
            row.resources.as_ref().map(|resources| resources.mem_mib),
        ) {
            cells.push("(near ceiling)".to_owned());
        }
        table.push(cells);
    }
    table
}

/// spec/17's suggestion pairing: a sandbox at 90% of its ceiling while host memory is low
/// earns the `viv memory trim` note — on stderr, as a suggestion, never an action (N23).
///
/// "Host memory is also low" is the admission table's own non-proceed condition, computed from
/// the same readings `start` acts on, so the suggestion and the admission warning can never
/// disagree about what "low" means. Every reading that cannot be taken silences the note: a
/// suggestion is the one output with no honest degraded form.
pub(super) fn trim_suggestion<E: Environment>(
    context: &Context<'_, E>,
    manifest: &str,
    used: Option<u64>,
    ceiling_mib: Option<u64>,
) -> String {
    if !near_ceiling(used, ceiling_mib) {
        return String::new();
    }
    let Ok(runtime_root) =
        config::resolve_runtime_root(context.environment, config::effective_uid())
    else {
        return String::new();
    };
    let Ok(fleet) = measured_use(context, &runtime_root) else {
        return String::new();
    };
    let (available, _) = host_memory();
    if matches!(
        lifecycle::admit(available, fleet.measured_bytes),
        lifecycle::Admission::Proceed
    ) {
        return String::new();
    }
    trim_suggestion_line(manifest)
}

/// The note's one line, separated so its wording is testable without a host to read.
pub(super) fn trim_suggestion_line(manifest: &str) -> String {
    format!(
        "`{manifest}` is near its memory ceiling while host memory is low; \
        `viv memory trim` reclaims cached guest memory\n"
    )
}

/// spec/17's near-ceiling threshold: measured use at or past 90% of the ceiling in force.
pub(super) const fn near_ceiling(used: Option<u64>, ceiling_mib: Option<u64>) -> bool {
    match (used, ceiling_mib) {
        (Some(used), Some(mib)) => {
            let ceiling = mib.saturating_mul(1024 * 1024);
            ceiling > 0 && used.saturating_mul(10) >= ceiling.saturating_mul(9)
        }
        _ => false,
    }
}

/// The fleet term spec/17's admission acts on: measured use of the running fleet.
pub(super) struct FleetUse {
    /// Sandboxes whose state is `Running` or `Stopping` — the same definition the report uses.
    pub running: usize,
    /// The subset of `running` whose unit reports `MemoryCurrent`.
    pub measured: usize,
    /// Whether the enumeration saw the whole domain. A runtime subtree that could not be read
    /// may hide a running member, so an incomplete scan turns the figures below into lower
    /// bounds rather than counts — silently presenting them as exact is how a warning that
    /// should fire goes quiet.
    pub complete: bool,
    /// The saturating sum over the measured members. `Some(0)` for a fleet that is measurably
    /// empty — a defined quantity — and `None` when members run and none reports (the
    /// controller is not delegated) or when an incomplete scan found nothing at all, because a
    /// figure the host did not measure is unavailable, not zero. When `measured < running` or
    /// the scan was incomplete, the sum is an honest lower bound, and the warning says so.
    pub measured_bytes: Option<u64>,
}

/// The running fleet's measured memory use, enumerated read-only.
///
/// The domain is wider than the report's: the union of the indexed sandboxes and the live
/// runtime target directories, deduplicated by (sandbox, target). The report deliberately keys
/// on the manifest library (spec/01's enumeration domain); admission measures what is running,
/// and a VM whose manifest was removed still consumes memory (spec/17 reads the fleet, never
/// the library).
///
/// # Errors
///
/// Returns [`Failure`] for an unreadable manifest library or index (`74`) — the same reader the
/// report opens with. The caller degrades this to a fleet term that is unavailable rather than
/// refusing a launch over a report.
pub(super) fn measured_use<E: Environment>(
    context: &Context<'_, E>,
    runtime_root: &Path,
) -> Result<FleetUse, Failure> {
    let index = super::workspace_index(context, true)?;
    let mut members: std::collections::BTreeSet<(String, String)> = index
        .manifests()
        .iter()
        .map(|indexed| (indexed.name.clone(), super::DEFAULT_TARGET.to_owned()))
        .collect();
    let (runtime_members, complete) = runtime_members(runtime_root);
    members.extend(runtime_members);
    let mut fleet = FleetUse {
        running: 0,
        measured: 0,
        complete,
        measured_bytes: None,
    };
    let mut sum: u64 = 0;
    for (sandbox_id, target) in members {
        let runtime = lifecycle::Runtime::for_report(runtime_root, &sandbox_id, &target);
        let has_build = lifecycle::last_build(&context.roots, &sandbox_id, &target).is_some();
        if !matches!(
            lifecycle::discriminate(&runtime, has_build),
            State::Running | State::Stopping
        ) {
            continue;
        }
        fleet.running += 1;
        if let (Some(bytes), _) = lifecycle::unit_memory_and_cgroup(&runtime.unit) {
            fleet.measured += 1;
            sum = sum.saturating_add(bytes);
        }
    }
    fleet.measured_bytes = fleet_sum(fleet.running, fleet.measured, sum, fleet.complete);
    Ok(fleet)
}

/// The live runtime target directories — `<sandbox_id>/<target>` under the runtime root — and
/// whether the walk saw all of them.
///
/// Scan errors are carried, never flattened: a subtree this walk could not read, or an entry
/// whose type could not be determined, may hide a running member, and pretending the walk was
/// whole would present a lower bound as a count. `Path::is_dir` is avoided on purpose — it
/// coerces a metadata failure to `false`. An absent root is the one quiet case: no VM has run
/// since boot, and that is a reading.
fn runtime_members(runtime_root: &Path) -> (std::collections::BTreeSet<(String, String)>, bool) {
    let mut members = std::collections::BTreeSet::new();
    let mut complete = true;
    match std::fs::read_dir(runtime_root) {
        Ok(sandboxes) => {
            for sandbox in sandboxes {
                let Ok(sandbox) = sandbox else {
                    complete = false;
                    continue;
                };
                match sandbox.file_type() {
                    Ok(kind) if kind.is_dir() => {}
                    Ok(_) => continue,
                    Err(_) => {
                        complete = false;
                        continue;
                    }
                }
                let Ok(sandbox_id) = sandbox.file_name().into_string() else {
                    // A non-UTF-8 name cannot be a sandbox id; foreign, not fleet.
                    continue;
                };
                match std::fs::read_dir(sandbox.path()) {
                    Ok(targets) => {
                        for target in targets {
                            let Ok(target) = target else {
                                complete = false;
                                continue;
                            };
                            match target.file_type() {
                                Ok(kind) if kind.is_dir() => {}
                                Ok(_) => continue,
                                Err(_) => {
                                    complete = false;
                                    continue;
                                }
                            }
                            if let Ok(target) = target.file_name().into_string() {
                                members.insert((sandbox_id.clone(), target));
                            }
                        }
                    }
                    Err(_) => complete = false,
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => complete = false,
    }
    (members, complete)
}

/// [`FleetUse::measured_bytes`]'s contract as a total function: a measurably empty fleet is a
/// zero, an incomplete scan that found nothing knows nothing, a fleet nobody measures is
/// unavailable, and a partial sum stands as the lower bound it is.
const fn fleet_sum(running: usize, measured: usize, sum: u64, complete: bool) -> Option<u64> {
    if running == 0 {
        if complete { Some(0) } else { None }
    } else if measured > 0 {
        Some(sum)
    } else {
        None
    }
}

/// One `/proc/meminfo` read feeding both host figures — spec/17's one-reader rule, shared by
/// the fleet report and `start`'s admission gate.
pub(super) fn host_memory() -> (Option<u64>, Option<u64>) {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok();
    (
        meminfo
            .as_deref()
            .and_then(crate::doctor::available_memory_bytes),
        meminfo
            .as_deref()
            .and_then(crate::doctor::total_memory_bytes),
    )
}

fn host_readings() -> HostReadings {
    let (mem_available_bytes, mem_total_bytes) = host_memory();
    HostReadings {
        mem_available_bytes,
        mem_total_bytes,
        pressure_some_avg60: lifecycle::host_pressure_some_avg60(),
    }
}

/// spec/17's closing line: `host: 9.6 GiB available of 31.2 GiB - memory pressure (60s): 0.4%`.
fn host_line(host: &HostReadings) -> String {
    let memory = match (host.mem_available_bytes, host.mem_total_bytes) {
        (Some(available), Some(total)) => format!(
            "{} available of {}",
            render::bytes(available),
            render::bytes(total)
        ),
        _ => "memory reading unavailable".to_owned(),
    };
    host.pressure_some_avg60.map_or_else(
        || format!("host: {memory}"),
        |pressure| format!("host: {memory} - memory pressure (60s): {pressure}%"),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{fleet_sum, missing_note, near_ceiling, runtime_members};
    use std::path::PathBuf;

    /// The runtime walk reports every `<sandbox>/<target>` directory pair, skips plain files,
    /// and calls an absent root a complete, empty reading.
    #[test]
    fn runtime_members_walks_sandbox_target_pairs() {
        let scratch = crate::test_support::ScratchDirectory::new().unwrap();
        let root = scratch.path().join("vivarium");
        let (members, complete) = runtime_members(&root);
        assert!(members.is_empty());
        assert!(complete);

        std::fs::create_dir_all(root.join("api").join("default")).unwrap();
        std::fs::create_dir_all(root.join("web").join("default")).unwrap();
        std::fs::create_dir_all(root.join("web").join("staging")).unwrap();
        std::fs::write(root.join("stray-file"), b"not a sandbox").unwrap();
        std::fs::write(root.join("api").join("stray"), b"not a target").unwrap();
        let (members, complete) = runtime_members(&root);
        assert!(complete);
        let expected: Vec<(String, String)> =
            [("api", "default"), ("web", "default"), ("web", "staging")]
                .map(|(sandbox, target)| (sandbox.to_owned(), target.to_owned()))
                .to_vec();
        assert_eq!(members.into_iter().collect::<Vec<_>>(), expected);
    }

    /// A sandbox directory the walk cannot read costs completeness, not silence: the members
    /// that were seen are still reported, and the walk says it did not see everything.
    #[test]
    fn runtime_members_reports_an_unreadable_subtree_as_incomplete() {
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = crate::test_support::ScratchDirectory::new().unwrap();
        let root = scratch.path().join("vivarium");
        std::fs::create_dir_all(root.join("open").join("default")).unwrap();
        let sealed = root.join("sealed");
        std::fs::create_dir_all(sealed.join("default")).unwrap();
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();

        let (members, complete) = runtime_members(&root);
        // Restore before asserting so a failure does not leave an unremovable tree behind.
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!complete);
        assert!(members.contains(&("open".to_owned(), "default".to_owned())));
    }

    /// The fleet term's contract: a measurably empty fleet is a zero, a fleet nobody measures
    /// is unavailable rather than zero, a partial sum stands as the lower bound it is, and an
    /// incomplete scan that found nothing claims nothing.
    #[test]
    fn fleet_sum_distinguishes_empty_unmeasured_partial_and_incomplete() {
        assert_eq!(fleet_sum(0, 0, 0, true), Some(0));
        assert_eq!(fleet_sum(2, 0, 0, true), None);
        assert_eq!(fleet_sum(2, 2, 512, true), Some(512));
        assert_eq!(fleet_sum(3, 1, 256, true), Some(256));
        assert_eq!(fleet_sum(0, 0, 0, false), None);
        assert_eq!(fleet_sum(2, 2, 512, false), Some(512));
    }

    /// The 90% rule holds at the boundary and never fires on an unknown half.
    #[test]
    fn the_trim_suggestion_names_the_sandbox_the_condition_and_the_verb() {
        let line = super::trim_suggestion_line("rust-web");
        assert_eq!(
            line,
            "`rust-web` is near its memory ceiling while host memory is low; \
            `viv memory trim` reclaims cached guest memory\n"
        );
    }

    #[test]
    fn near_ceiling_is_ninety_percent_of_a_known_pair() {
        let mib = 1024 * 1024_u64;
        assert!(near_ceiling(Some(922 * mib), Some(1024)));
        assert!(!near_ceiling(Some(900 * mib), Some(1024)));
        assert!(near_ceiling(Some(8 * 1024 * mib), Some(8 * 1024)));
        assert!(!near_ceiling(None, Some(1024)));
        assert!(!near_ceiling(Some(mib), None));
        assert!(!near_ceiling(Some(0), Some(0)));
    }

    /// The warning names each declaration, and says why nothing was removed.
    #[test]
    fn missing_note_names_each_declaration() {
        assert_eq!(missing_note(&[]), "");
        let note = missing_note(&[("rust-web".to_owned(), PathBuf::from("/home/alice/backend"))]);
        assert!(note.starts_with("warning: 1 declared workspace directory no longer exists:"));
        assert!(note.contains("rust-web  →  /home/alice/backend"));
        assert!(note.contains("unmounted filesystem"));
    }
}
