//! The `generations` family and `gc`: read, switch, and unlink retained builds, and sweep.
//!
//! The state behind every verb here is `config::generations`' per-project profile; this file owns
//! only the verbs — selection, guards, and rendering. The split mirrors `volume.rs` over
//! `config::volumes`, and for the same reason: the profile is written by the one path that
//! builds, and the readers must never need Nix to answer.

use std::process::Command;

use serde_json::Value;

use super::grammar::Output;
use super::lifecycle;
use super::{Context, Failure, Success, diagnosed, render};
use crate::config::{self, Environment, generations};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::ui::watch;

/// `viv generations list` (spec/01, spec/11): read-only, `78` and nothing else (spec/14).
pub(super) fn list<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let resolved = super::resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name;
    let rows = generations::list(&lifecycle::generation_paths(
        &context.roots,
        &sandbox_id,
        super::DEFAULT_TARGET,
    ));
    Ok(Success::plain(if output.is_json() {
        render::generations_list_json(&rows_json(&rows, &sandbox_id))
    } else {
        render::generations_list_human(&rows_human(&rows), context.ui.palette_out())
    }))
}

/// `viv generations prune` (spec/11): unlink what the retention argument selects, and only that.
pub(super) fn prune<E: Environment>(
    context: &Context<'_, E>,
    keep: Option<u64>,
    older_than_seconds: Option<u64>,
    output: Output,
) -> Result<Success, Failure> {
    let resolved = super::resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let runtime = lifecycle::Runtime::locate(&runtime_root, &sandbox_id, super::DEFAULT_TARGET)?;
    // The lock before the state is read (ADR-0053), for the reason `volume prune` states: a check
    // made outside it answers about a moment that is over.
    let _lock = lifecycle::TargetLock::acquire(&runtime)?;

    let paths = lifecycle::generation_paths(&context.roots, &sandbox_id, super::DEFAULT_TARGET);
    let rows = generations::list(&paths);
    let selected = select(&rows, keep, older_than_seconds, lifecycle::epoch_now());

    // The running-VM guard ADR-0085 fixes: read the boot record, never `current`, because
    // `--generation <n>` and a stale VM both name a generation `current` has moved past. Refused
    // whole rather than trimmed around: a prune that silently kept one extra generation would
    // report a retention the flags did not say.
    let running = !matches!(
        lifecycle::discriminate(
            &runtime,
            lifecycle::last_build(&context.roots, &sandbox_id, super::DEFAULT_TARGET).is_some()
        ),
        lifecycle::State::Absent | lifecycle::State::Built
    );
    if running
        && let Some(booted) =
            lifecycle::running_build(&context.roots, &sandbox_id, super::DEFAULT_TARGET)
        && let Some(generation) = selected
            .iter()
            .find(|row| row.store_path.as_deref() == Some(booted.as_str()))
    {
        return Err(diagnosed(
            Namespace::Vm,
            "generation-in-use",
            format!(
                "generation {} is what the running VM was booted from",
                generation.number
            ),
            Locus::Named("build history"),
            "its root is what keeps the running guest's own closure out of a \
            collection's reach (ADR-0085)",
            ExitKind::TempFail,
        )
        .with_hint("`viv stop` first, or keep more generations"));
    }

    for row in &selected {
        generations::unlink(&paths, row.number).map_err(|error| super::registry_failure(&error))?;
    }
    let kept = rows.len() - selected.len();
    Ok(Success::plain(if output.is_json() {
        render::generations_prune_json(&rows_json(&selected, &sandbox_id), kept)
    } else {
        render::generations_prune_human(&rows_human(&selected), kept)
    }))
}

/// `viv generations activate <n>` and `rollback` (spec/11): repoint `current`, nothing else.
///
/// One routine for both because rollback is activate with the target computed — the previous
/// retained generation — and two spellings of the repoint would be two products.
pub(super) fn activate<E: Environment>(
    context: &Context<'_, E>,
    number: Option<u64>,
    output: Output,
) -> Result<Success, Failure> {
    let verb = if number.is_some() {
        "activate"
    } else {
        "rollback"
    };
    let resolved = super::resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let runtime = lifecycle::Runtime::locate(&runtime_root, &sandbox_id, super::DEFAULT_TARGET)?;
    // Under the per-target lock like every other writer of this project's state (ADR-0053).
    let _lock = lifecycle::TargetLock::acquire(&runtime)?;

    let paths = lifecycle::generation_paths(&context.roots, &sandbox_id, super::DEFAULT_TARGET);
    // Retained means bootable here, not merely listed: the lenient list also renders crash
    // remnants — metadata without a link — and links whose output was collected, and moving
    // `current` onto either would report success while breaking the next `--no-rebuild`.
    let retained: Vec<u64> = generations::list(&paths)
        .iter()
        .filter(|row| generations::store_path(&paths, row.number).is_some())
        .map(|row| row.number)
        .collect();
    let previous = generations::current_number(&paths);

    // spec/14 makes an unknown generation `64` on this family: the number is an argument, and an
    // argument naming nothing retained is a malformed request rather than a state fault.
    let unknown = |message: String, why: &str| {
        Err(diagnosed(
            Namespace::State,
            "unknown-generation",
            message,
            Locus::Named("build history"),
            why.to_owned(),
            ExitKind::Usage,
        )
        .with_hint("`viv generations list` names what is retained"))
    };
    let target = if let Some(number) = number {
        if !retained.contains(&number) {
            return unknown(
                format!("generation {number} is not retained"),
                "`activate` moves `current` to a retained generation",
            );
        }
        number
    } else {
        let Some(current) = previous else {
            return unknown(
                "this project has no current generation".to_owned(),
                "`rollback` steps back from `current`, which does not exist",
            );
        };
        let Some(target) = retained.iter().rev().find(|&&n| n < current) else {
            return unknown(
                format!("no generation is retained before {current}"),
                "`rollback` needs an older generation to step back to",
            );
        };
        *target
    };
    generations::set_current(&paths, target).map_err(|error| super::registry_failure(&error))?;
    Ok(Success::plain(if output.is_json() {
        render::generations_switch_json(verb, target, previous)
    } else {
        render::generations_switch_human(target, previous)
    }))
}

/// `viv gc` (spec/11): the whole-store sweep, global by design — no manifest, no binding.
///
/// The collector reclaims what no root pins, vivarium's or not, and never touches what one does.
/// `nix-store --gc` is the stable spelling; its progress streams through the step like a build's.
pub(super) fn gc<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let step = context.ui.step("collecting the store");
    let mut command = Command::new("nix-store");
    command.arg("--gc");
    let captured = watch::output(&mut command, &step).map_err(|source| {
        let code = if source.kind() == std::io::ErrorKind::PermissionDenied {
            ExitKind::NoPerm
        } else {
            ExitKind::Unavailable
        };
        diagnosed(
            Namespace::Store,
            "gc-unavailable",
            "could not run `nix-store --gc`",
            Locus::Named("store collection"),
            source.to_string(),
            code,
        )
    })?;
    if !captured.status.success() {
        let stderr = captured.stderr;
        // spec/14 splits the sweep's failures: a denied store is `77`, everything else `70`.
        let code = if stderr.to_lowercase().contains("permission denied") {
            ExitKind::NoPerm
        } else {
            ExitKind::Software
        };
        return Err(diagnosed(
            Namespace::Store,
            "gc-failed",
            "the store collection failed",
            Locus::Named("store collection"),
            stderr.trim().to_owned(),
            code,
        ));
    }
    step.done("collected the store");
    // The collector's own accounting line — "n store paths deleted, m MiB freed" — is the
    // result, relayed rather than re-derived: a second measurement would be a second answer.
    // It is the last thing the collector prints on stdout; stderr carries progress and
    // housekeeping notes — measured, because the first draft read stderr's last line and got
    // the hard-linking note — so stderr is only the fallback for a collector that said
    // nothing on stdout.
    let stdout = String::from_utf8_lossy(&captured.stdout);
    let summary = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            captured
                .stderr
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(ToOwned::to_owned)
        });
    Ok(Success::plain(if output.is_json() {
        render::gc_json(summary.as_deref())
    } else {
        render::gc_human(summary.as_deref())
    }))
}

/// The rows a retention argument selects for unlinking. The generation `current` points at is
/// never a candidate: pruning it would silently break `--no-rebuild`, and repointing is
/// `activate`'s job — recorded in spec/02's reconciliation note rather than decided per call.
fn select(
    rows: &[generations::Generation],
    keep: Option<u64>,
    older_than_seconds: Option<u64>,
    now: u64,
) -> Vec<generations::Generation> {
    let candidates = rows.iter().filter(|row| !row.current);
    match (keep, older_than_seconds) {
        (Some(keep), None) => {
            // The n newest survive; the rows arrive oldest first, so they are the tail.
            let newest: std::collections::BTreeSet<u64> = rows
                .iter()
                .rev()
                .take(usize::try_from(keep).unwrap_or(usize::MAX))
                .map(|row| row.number)
                .collect();
            candidates
                .filter(|row| !newest.contains(&row.number))
                .cloned()
                .collect()
        }
        (None, Some(age)) => {
            let threshold = now.saturating_sub(age);
            candidates
                .filter(|row| {
                    // An unreadable `built_at` is an unknown age, and unknown is retained:
                    // deleting on a guess fails in the unrecoverable direction.
                    row.record
                        .as_ref()
                        .and_then(|record| generations::parse_rfc3339_utc(&record.built_at))
                        .is_some_and(|built| built < threshold)
                })
                .cloned()
                .collect()
        }
        // The grammar admits exactly one retention argument; anything else selects nothing.
        _ => Vec::new(),
    }
}

/// The published row shape (spec/01): all seven keys, `null` for what a row cannot fill honestly.
fn rows_json(rows: &[generations::Generation], sandbox_id: &str) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            serde_json::json!({
                "number": row.number,
                "current": row.current,
                "store_path": row.store_path,
                "manifest": row
                    .record
                    .as_ref()
                    .map_or(sandbox_id, |record| record.manifest.as_str()),
                "lock_digest": row.record.as_ref().map(|record| &record.lock_digest),
                "backend": row.record.as_ref().map(|record| &record.backend),
                "built_at": row.record.as_ref().map(|record| &record.built_at),
            })
        })
        .collect()
}

fn rows_human(rows: &[generations::Generation]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| {
            let field = |value: Option<&str>| value.unwrap_or("-").to_owned();
            vec![
                row.number.to_string(),
                if row.current { "current" } else { "-" }.to_owned(),
                field(row.record.as_ref().map(|record| record.built_at.as_str())),
                field(
                    row.record
                        .as_ref()
                        .map(|record| record.lock_digest.as_str()),
                ),
                field(row.store_path.as_deref()),
            ]
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(number: u64, current: bool, built_at: Option<&str>) -> generations::Generation {
        generations::Generation {
            number,
            store_path: Some(format!("/nix/store/out-{number}")),
            record: built_at.map(|built_at| generations::GenerationRecord {
                store_path: format!("/nix/store/out-{number}"),
                lock_digest: "sha256:x".to_owned(),
                manifest: "demo".to_owned(),
                backend: "cloud-hypervisor".to_owned(),
                built_at: built_at.to_owned(),
            }),
            current,
        }
    }

    fn numbers(rows: &[generations::Generation]) -> Vec<u64> {
        rows.iter().map(|row| row.number).collect()
    }

    #[test]
    fn keep_retains_the_newest_and_always_the_current() {
        let rows = vec![row(1, false, None), row(2, false, None), row(3, true, None)];
        assert_eq!(numbers(&select(&rows, Some(1), None, 0)), vec![1, 2]);
        assert_eq!(numbers(&select(&rows, Some(2), None, 0)), vec![1]);
        assert_eq!(numbers(&select(&rows, Some(0), None, 0)), vec![1, 2]);

        // A rolled-back `current` is retained beside the newest, never traded for it.
        let rolled = vec![row(1, true, None), row(2, false, None), row(3, false, None)];
        assert_eq!(numbers(&select(&rolled, Some(1), None, 0)), vec![2]);
    }

    #[test]
    fn older_than_reads_ages_and_keeps_the_unknown() {
        let now = 1_000_000_000; // 2001-09-09T01:46:40Z
        let rows = vec![
            row(1, false, Some("2000-01-01T00:00:00Z")), // old
            row(2, false, Some("not a date")),           // unknown age: retained
            row(3, false, None),                         // no record at all: retained
            row(4, true, Some("2000-01-01T00:00:00Z")),  // old but current: retained
            row(5, false, Some("2001-09-09T00:00:00Z")), // young
        ];
        assert_eq!(numbers(&select(&rows, None, Some(86_400), now)), vec![1]);
    }
}
