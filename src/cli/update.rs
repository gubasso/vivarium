//! `viv update`: the one command that moves a pin (spec/01, ADR-0059, ADR-0112).
//!
//! The order below is the contract. Everything that can refuse without writing refuses
//! first — an unbound project, a team override lock, an unknown input name. Only then is
//! the per-target lock taken, the private tree rendered, and Nix run against it with the
//! owned lock as the reference and a staged sibling as the output, so the durable lock is
//! either untouched or replaced whole. The shared published cache is never the tree Nix
//! reads: unlocked readers exchange it at will, and the target lock cannot stop them.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use super::grammar::{Output, UsageError};
use super::{Context, Failure, Success, diagnosed, lifecycle, render};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::ui::watch;

/// The two inputs every generated flake carries, always updatable by name.
const BASELINE_NAMES: [&str; 2] = ["nixpkgs", "microvm"];

/// `viv update [<input>...]` — re-resolve, report each row's before and after, build nothing.
pub(super) fn run<E: Environment>(
    context: &Context<'_, E>,
    inputs: &[String],
    output: Output,
) -> Result<Success, Failure> {
    let resolved = super::resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name.clone();

    refuse_override(context, &sandbox_id, &resolved.selected)?;

    // The unknown-name refusal, from a read-only resolution and before anything else: a
    // usage answer (`64`) is owed deterministically, so it must precede the target lock —
    // a contended lock would turn a typo into `75` — and precede every write, including
    // the runtime directory the lock lives in and the private tree.
    let (declared, probed_base) = config::resolve_update_inputs(
        &context.roots,
        &sandbox_id,
        super::DEFAULT_TARGET,
        &resolved.selected,
        &resolved.source,
        &resolved.manifest,
    )
    .map_err(|error| super::flake_failure(&error))?;
    requested_names(inputs, &declared, probed_base.as_ref())?;

    // The per-target lock, before the effective lock is selected and its bytes staged into
    // the private tree: a snapshot taken outside it answers about a moment that is over
    // (ADR-0053), and a copy taken before acquisition could be overwritten by a `start`
    // persisting a first pin — or by another update — between the copy and the lock, after
    // which this update would re-resolve from the stale bytes and replace the newer pin.
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let runtime = lifecycle::Runtime::locate(&runtime_root, &sandbox_id, super::DEFAULT_TARGET)?;
    let _lock = lifecycle::TargetLock::acquire(&runtime)?;

    // The private tree: resolution, the base probe, and rendering — no durable write, and
    // not the shared cache tree the unlocked readers republish. It re-resolves and
    // re-probes under the lock; the pre-lock resolution above answered only the usage
    // question, and this one is the state the update acts on.
    let prepared = config::prepare_update_tree(
        &context.roots,
        &sandbox_id,
        super::DEFAULT_TARGET,
        &resolved.selected,
        &resolved.source,
        &resolved.manifest,
        &resolved.workspace_paths,
        &config::BaselineInputs::from_environment(context.environment),
    )
    .map_err(|error| super::flake_failure(&error))?;

    // The authoritative derivation, from the state the lock now protects. The config root
    // is user-owned and unlocked, so its topology can change between the pre-lock usage
    // answer and this preparation; acting on the earlier snapshot could run Nix on targets
    // translated for a topology the tree no longer has. A name that change invalidated is
    // a transient race, not a usage fault.
    let Ok((requested, translated)) =
        requested_names(inputs, &prepared.declared, prepared.base.as_ref())
    else {
        return Err(diagnosed(
            Namespace::Manifest,
            "input-set-changed",
            "the project's declared inputs changed while the update was preparing",
            Locus::Named("update preparation"),
            "a requested name no longer resolves against the current configuration".to_owned(),
            ExitKind::TempFail,
        )
        .with_hint("re-run the update against the configuration as it now is"));
    };

    let before = prepared
        .staged_reference
        .as_deref()
        .map(read_pins)
        .transpose()?
        .unwrap_or_default();

    let owned_path = prepared.effective_lock.path().to_path_buf();
    let created = matches!(
        prepared.effective_lock,
        config::EffectiveLock::OwnedMissing { .. }
    );
    let parent = owned_path.parent().ok_or_else(|| {
        diagnosed(
            Namespace::Lock,
            "lock-parent",
            "the owned lock has no parent directory",
            Locus::File(owned_path.clone()),
            "the lock path escaped its fixed data-root layout".to_owned(),
            ExitKind::Software,
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|source| {
        diagnosed(
            Namespace::Lock,
            "persist-parent",
            "could not create the lock directory",
            Locus::File(parent.to_path_buf()),
            source.to_string(),
            ExitKind::IoErr,
        )
    })?;
    // Unique per process, beside the owned lock so the final rename never crosses a
    // filesystem; removed on every failure path below so no residue sits beside the lock.
    let staged = parent.join(format!("flake.lock.update-{}", std::process::id()));

    let result = run_update(
        context,
        &prepared,
        &requested,
        &translated,
        &staged,
        &owned_path,
        &before,
    );
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    let (rows_json, rows_human) = result?;

    let notes = notes_for(created, &owned_path, prepared.base.as_ref());
    Ok(Success {
        stdout: if output.is_json() {
            render::update_json(&sandbox_id, &owned_path, &rows_json)
        } else {
            render::update_human(&rows_human, &owned_path, context.ui.palette_out())
        },
        notes,
    })
}

/// The stderr notes: the created pin (spec/04 requires it reported), and the base
/// sub-lock's seeding rule (ADR-0112 requires the held pins explained).
fn notes_for(created: bool, owned_path: &Path, base: Option<&config::ImageBase>) -> String {
    let mut notes = String::new();
    if created {
        let _ = std::fmt::Write::write_fmt(
            &mut notes,
            format_args!(
                "created the pin for this target: {}\n",
                owned_path.display()
            ),
        );
    }
    if let Some(base) = base
        && base.flake_path.with_file_name("flake.lock").is_file()
    {
        let _ = std::fmt::Write::write_fmt(
            &mut notes,
            format_args!(
                "the base `{}` carries its own `flake.lock`; the pins it holds re-seed \
                every re-resolution and move only when that file does\n",
                base.input_name
            ),
        );
    }
    notes
}

/// The ADR-0062 refusal, before any lock, any probe, and any write — not even the private
/// tree is rendered for a project whose pin is not vivarium's to move. Both files are
/// named, per spec/02: the file that is read, and the file this update would have written.
fn refuse_override<E: Environment>(
    context: &Context<'_, E>,
    sandbox_id: &str,
    selected: &crate::config::ResolvedArtifact,
) -> Result<(), Failure> {
    let paths = config::target_paths(&context.roots, sandbox_id, super::DEFAULT_TARGET, selected)
        .map_err(|error| super::flake_failure(&error))?;
    let selected_lock = paths
        .select_lock()
        .map_err(|error| super::flake_failure(&error))?;
    if let config::EffectiveLock::Override { path } = &selected_lock {
        return Err(diagnosed(
            Namespace::Lock,
            "override-in-force",
            "a team override lock is in force",
            Locus::File(path.clone()),
            format!(
                "`{}` is read-only to the tool and shadows `{}`, which this update would \
                have written",
                path.display(),
                paths.owned_lock.display()
            ),
            ExitKind::Config,
        )
        .with_hint("moving a shared pin is the team's own act, outside vivarium"));
    }
    Ok(())
}

/// The known public set, `64` for a name outside it, and the follows translation.
///
/// The name check is decided here, never delegated: Nix answers an unknown name with a
/// warning and a successful no-op, which would read as "nothing moved" (spec/01 fixes
/// `64`). And a requested baseline the base flake declares is a `follows` alias at the
/// root, which Nix updates by warning and moving nothing; the input that owns the node is
/// the base itself, so the request is translated to it (findings register, 2026-08-26).
fn requested_names<'names>(
    inputs: &'names [String],
    declared: &'names [String],
    base: Option<&'names config::ImageBase>,
) -> Result<(Vec<&'names str>, Vec<&'names str>), Failure> {
    let known: Vec<&str> = BASELINE_NAMES
        .iter()
        .copied()
        .chain(declared.iter().map(String::as_str))
        .chain(base.iter().map(|base| base.input_name.as_str()))
        .collect();
    if let Some(unknown) = inputs.iter().find(|name| !known.contains(&name.as_str())) {
        return Err(Failure::Usage(UsageError {
            message: format!(
                "`viv update` knows no input `{unknown}` (this project has: {})",
                known.join(", ")
            ),
            usage: Some(super::grammar::UPDATE_USAGE),
        }));
    }
    let mut requested: Vec<&str> = Vec::new();
    let mut translated: Vec<&str> = Vec::new();
    for name in inputs {
        if !requested.contains(&name.as_str()) {
            requested.push(name.as_str());
        }
        let target = match base {
            Some(base)
                if (name == "nixpkgs" && base.declares_nixpkgs)
                    || (name == "microvm" && base.declares_microvm) =>
            {
                base.input_name.as_str()
            }
            _ => name.as_str(),
        };
        if !translated.contains(&target) {
            translated.push(target);
        }
    }
    Ok((requested, translated))
}

/// The Nix run, validation, persistence, and row derivation — everything after which a
/// failure must also remove the staged output file, grouped so the caller can.
#[allow(clippy::too_many_arguments)]
fn run_update<E: Environment>(
    context: &Context<'_, E>,
    prepared: &config::UpdatePreparation,
    requested: &[&str],
    translated: &[&str],
    staged: &Path,
    owned_path: &Path,
    before: &std::collections::BTreeMap<String, config::RootPin>,
) -> Result<(Vec<Value>, Vec<Vec<String>>), Failure> {
    let step = context.ui.step("re-resolving the project's pinned inputs");
    let mut command = Command::new("nix");
    command.args(["flake", "update"]);
    command.args(translated);
    command.args(config::evaluate::FEATURE_FLAGS);
    command.arg("--flake");
    command.arg(&prepared.directory);
    command.arg("--output-lock-file");
    command.arg(staged);
    // Only when a lock is in force: the flag naming a missing file is silently treated as
    // absent by Nix, and relying on that would make a typo in the path a full re-resolve
    // (findings register, 2026-08-26).
    if let Some(reference) = &prepared.staged_reference {
        command.arg("--reference-lock-file");
        command.arg(reference);
    }
    let captured = watch::output(&mut command, &step).map_err(|source| {
        let code = if source.kind() == std::io::ErrorKind::PermissionDenied {
            ExitKind::NoPerm
        } else {
            ExitKind::Unavailable
        };
        diagnosed(
            Namespace::Store,
            "update-unavailable",
            "could not run `nix flake update`",
            Locus::Named("input re-resolution"),
            source.to_string(),
            code,
        )
    })?;
    if !captured.status.success() {
        drop(step);
        let stderr = captured.stderr;
        // spec/14 splits the failures: an input that cannot be reached is `69`, a Nix fault
        // `70`. Matched on the message, as `names_a_missing_lock_node` already is, and a
        // miss degrades to the honest `70`.
        let lowered = stderr.to_lowercase();
        let unreachable = lowered.contains("unable to download")
            || lowered.contains("couldn't resolve host")
            || lowered.contains("cannot fetch");
        return Err(if unreachable {
            diagnosed(
                Namespace::Store,
                "input-unreachable",
                "an input could not be reached to re-resolve",
                Locus::Named("input re-resolution"),
                stderr.trim().to_owned(),
                ExitKind::Unavailable,
            )
        } else {
            diagnosed(
                Namespace::Store,
                "update-failed",
                "the input re-resolution failed",
                Locus::Named("input re-resolution"),
                stderr.trim().to_owned(),
                ExitKind::Software,
            )
        });
    }
    step.done("re-resolved the project's pinned inputs");

    let bytes = std::fs::read(staged).map_err(|source| {
        diagnosed(
            Namespace::Lock,
            "update-read",
            "could not read the lock the update produced",
            Locus::File(staged.to_path_buf()),
            source.to_string(),
            ExitKind::IoErr,
        )
    })?;
    // The composed rule over the candidate, before anything durable moves: a defect
    // discards it and the owned lock stays byte-identical (ADR-0112).
    config::composed_lock_failure(&bytes).map_err(|error| super::flake_failure(&error))?;
    let after = config::lock_pins(&bytes).ok_or_else(|| {
        diagnosed(
            Namespace::Lock,
            "update-undecodable",
            "the lock the update produced cannot be read back",
            Locus::File(staged.to_path_buf()),
            "expected the JSON lock schema `nix flake update` writes".to_owned(),
            ExitKind::Software,
        )
    })?;
    config::persist_updated_lock(owned_path, staged)
        .map_err(|error| super::flake_failure(&error))?;

    Ok(derive_rows(before, &after, requested))
}

/// Rows: the requested set whole — unchanged rows included, every `changed` false being a
/// fact about upstream — plus any public input whose pin moved by following one that was
/// requested. No names means every input (spec/01).
fn derive_rows(
    before: &std::collections::BTreeMap<String, config::RootPin>,
    after: &std::collections::BTreeMap<String, config::RootPin>,
    requested: &[&str],
) -> (Vec<Value>, Vec<Vec<String>>) {
    // Row selection keys on the names the user passed, verbatim — a followed baseline is
    // requested as itself even though Nix is invoked on the base that owns its node
    // (spec/01: the string a user passes is the `name` the record reports).
    let requested: Vec<&str> = if requested.is_empty() {
        after.keys().map(String::as_str).collect()
    } else {
        requested.to_vec()
    };
    let mut rows_json = Vec::new();
    let mut rows_human = Vec::new();
    for (name, pin) in after {
        let previous = before.get(name);
        // Movement is the whole resolved subtree, not the row's own node: the pin a user
        // moves can sit below a hashless base node, and a created pin is a change.
        let changed =
            previous.map(|prior| prior.fingerprint.as_str()) != Some(pin.fingerprint.as_str());
        let previous = previous.and_then(|prior| prior.display.clone());
        // The requested public names are always reported, unchanged rows included; every
        // other public root appears exactly when its pin moved.
        if !requested.contains(&name.as_str()) && !changed {
            continue;
        }
        rows_json.push(json!({
            "name": name,
            "before": previous,
            "after": pin.display,
            "changed": changed,
        }));
        rows_human.push(vec![
            name.clone(),
            previous.unwrap_or_else(|| "-".to_owned()),
            pin.display.clone().unwrap_or_else(|| "-".to_owned()),
            if changed { "moved" } else { "unchanged" }.to_owned(),
        ]);
    }
    (rows_json, rows_human)
}

/// The before-pins, from the lock staged into the private tree.
fn read_pins(path: &Path) -> Result<std::collections::BTreeMap<String, config::RootPin>, Failure> {
    let bytes = std::fs::read(path).map_err(|source| {
        diagnosed(
            Namespace::Lock,
            "update-read",
            "could not read the lock in force",
            Locus::File(path.to_path_buf()),
            source.to_string(),
            ExitKind::IoErr,
        )
    })?;
    // An unparsable lock in force has no pins to report; every `before` is unknown rather
    // than the run refused — Nix names what is wrong with it if anything is.
    Ok(config::lock_pins(&bytes).unwrap_or_default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use super::requested_names;
    use crate::cli::Failure;
    use crate::config::ImageBase;

    fn base(nixpkgs: bool, microvm: bool) -> ImageBase {
        ImageBase {
            input_name: "my-base".to_owned(),
            flake_path: PathBuf::from("/config/images/my-base/flake.nix"),
            declares_nixpkgs: nixpkgs,
            declares_microvm: microvm,
        }
    }

    /// The requested list keeps the user's own names, deduplicated; the translated list
    /// maps a followed baseline to the base that owns its node, and only that (spec/01,
    /// ADR-0112). An unknown name is the usage refusal, decided before any lock exists.
    #[test]
    fn requested_and_translated_stay_separate_and_unknown_is_usage() {
        let inputs = vec![
            "nixpkgs".to_owned(),
            "my-base".to_owned(),
            "nixpkgs".to_owned(),
        ];
        let declared: Vec<String> = Vec::new();
        let with_base = base(true, false);
        let (requested, translated) =
            requested_names(&inputs, &declared, Some(&with_base)).unwrap();
        assert_eq!(requested, vec!["nixpkgs", "my-base"]);
        assert_eq!(translated, vec!["my-base"]);

        // Undeclared baseline: no translation happens.
        let first = inputs[..1].to_vec();
        let undeclared = base(false, false);
        let (requested, translated) =
            requested_names(&first, &declared, Some(&undeclared)).unwrap();
        assert_eq!(requested, vec!["nixpkgs"]);
        assert_eq!(translated, vec!["nixpkgs"]);

        let unknown = vec!["bogus".to_owned()];
        let refused = requested_names(&unknown, &declared, None);
        assert!(
            matches!(&refused, Err(Failure::Usage(error)) if error.message.contains("bogus")),
            "expected a usage refusal naming the input"
        );
    }
}
