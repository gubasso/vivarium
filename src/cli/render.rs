//! The concrete output shapes spec/01 fixes, held apart from the commands that produce them.
//!
//! Separate from `mod.rs` because the two answer different questions. A command decides what is
//! true; a renderer decides how it is spelled — and the spelling is the published half. spec/01
//! names each key and each nesting, ADR-0075 freezes them before 1.0, and a shape assembled inline
//! at the end of a command would be a contract with no single place to read it.
//!
//! One rule runs through all of it: an absent declaration renders as `null`, never as a default.
//! spec/03 is explicit that an absent table is never an empty one — an undeclared memory ceiling
//! resolves from the host at launch, and `0` would be a different and wrong answer. So the JSON
//! keys are always present, because a consumer needs a stable shape, and their values distinguish
//! "not declared" from "declared as this".

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::config::evaluate::KeyClass;
use crate::config::merged::{Analysis, ConflictKind};
use crate::config::{
    Egress, EgressMode, Manifest, ResolvedArtifact, ResolvedBinding, Resources, XdgRoots,
};
use crate::ui::style::Palette;
use crate::ui::table;

use super::grammar::COMMANDS;
use super::lifecycle::{Report, RuntimeReadings};

/// `viv --help` and `viv <verb> --help` — the summary, or one verb's usage.
///
/// An unknown verb falls back to the summary rather than failing: help never fails, and the
/// summary is the answer to "what is there" whatever prompted the question. Padded spans are
/// padded before styling, because format width counts escape bytes.
pub fn help_human(palette: &Palette, verb: Option<&str>) -> String {
    if let Some((name, usage, blurb)) =
        verb.and_then(|requested| COMMANDS.iter().find(|(name, ..)| *name == requested))
    {
        return format!(
            "{} — {blurb}\n\n{} {usage}\n",
            palette.accent.apply_to(*name),
            palette.label.apply_to("usage:"),
        );
    }
    let mut rendered = format!(
        "{} — each project in its own microVM\n\n{} viv <command> [options]\n\n",
        palette.accent.apply_to("viv"),
        palette.label.apply_to("usage:"),
    );
    let _ = writeln!(rendered, "{}", palette.label.apply_to("commands:"));
    for (name, _, blurb) in COMMANDS {
        let _ = writeln!(
            rendered,
            "  {} {blurb}",
            palette.accent.apply_to(format!("{name:<12}")),
        );
    }
    let _ = writeln!(rendered, "\n{}", palette.label.apply_to("global flags:"));
    for (flag, blurb) in [
        ("-v, --verbose", "more stderr detail; stackable"),
        ("-q, --quiet", "suppress progress; errors still print"),
        ("-h, --help", "this summary, or one command's usage"),
        ("--version", "the version"),
    ] {
        let _ = writeln!(
            rendered,
            "  {} {blurb}",
            palette.accent.apply_to(format!("{flag:<14}")),
        );
    }
    let _ = writeln!(
        rendered,
        "\nrun `viv <command> --help` for one command's flags; \
        `--json` on a data command emits one machine record on stdout",
    );
    rendered
}

/// `viv config --json` — the binding record.
pub fn binding_json(
    binding: &ResolvedBinding,
    roots: &XdgRoots,
    flake: &Path,
    lock: &Path,
) -> String {
    line(&json!({
        "manifest": binding.manifest,
        "source": binding.source.as_str(),
        "paths": {
            "config": display(&roots.config),
            "state": display(&roots.state),
            "data": display(&roots.data),
            "cache": display(&roots.cache),
            "flake": display(flake),
            "lock": display(lock),
        },
    }))
}

/// `viv config` — the same record, one fact per line: dim labels, the manifest accented.
pub fn binding_human(
    binding: &ResolvedBinding,
    roots: &XdgRoots,
    flake: &Path,
    lock: &Path,
    palette: &Palette,
) -> String {
    let mut rows: Vec<(&str, String)> = vec![
        (
            "manifest",
            palette.accent.apply_to(&binding.manifest).to_string(),
        ),
        ("source", binding.source.as_str().to_owned()),
    ];
    for (label, path) in [
        ("config", roots.config.as_path()),
        ("state", roots.state.as_path()),
        ("data", roots.data.as_path()),
        ("cache", roots.cache.as_path()),
        ("flake", flake),
        ("lock", lock),
    ] {
        rows.push((label, path.display().to_string()));
    }
    table::record(palette, &rows)
}

/// `viv config eval --json` — the merged configuration, nested under the identity that produced it.
pub fn config_eval_json(
    binding: &ResolvedBinding,
    manifest: &Manifest,
    analysis: &Analysis,
) -> String {
    line(&json!({
        "manifest": binding.manifest,
        "image": manifest.image,
        "pieces": manifest.pieces,
        "config": analysis.configuration(),
    }))
}

/// `viv config eval` — TOML-shaped, so the merged result reads in the language it was authored in.
///
/// An undeclared key is omitted rather than printed as a null, because TOML has no null and a `0`
/// would be a different and wrong answer: spec/03 resolves an undeclared ceiling from the host at
/// launch. The JSON form is where a consumer reads the distinction, and it keeps the key.
pub fn config_eval_human(analysis: &Analysis) -> String {
    let mut rendered = String::new();
    table(
        &mut rendered,
        "credentials",
        &[("agents", analysis.effective("credentials.agents"))],
    );
    table(
        &mut rendered,
        "resources",
        &[
            ("mem_mib", analysis.effective("resources.mem_mib")),
            ("vcpu", analysis.effective("resources.vcpu")),
        ],
    );
    table(
        &mut rendered,
        "sandbox.egress",
        &[
            ("mode", analysis.effective("sandbox.egress.mode")),
            ("allow", analysis.effective("sandbox.egress.allow")),
        ],
    );
    let environment: Vec<(&str, Option<&Value>)> = analysis
        .values
        .iter()
        .filter_map(|view| {
            view.key
                .strip_prefix("env.")
                .map(|name| (name, view.effective.as_ref()))
        })
        .collect();
    table(&mut rendered, "env", &environment);
    array_of_tables(
        &mut rendered,
        "workspaces",
        analysis.effective("workspaces"),
        &["source"],
    );
    array_of_tables(
        &mut rendered,
        "mounts",
        analysis.effective("mounts"),
        &["source", "target", "readonly"],
    );
    array_of_tables(
        &mut rendered,
        "volumes",
        analysis.effective("volumes"),
        &["name", "mount", "size_gib"],
    );
    table(
        &mut rendered,
        "volume",
        &[
            ("size_gib", analysis.effective("volume.size_gib")),
            ("persist", analysis.effective("volume.persist")),
        ],
    );
    rendered
}

/// `viv config sources --json` — provenance and defects, at exit `0`.
pub fn config_sources_json(
    binding: &ResolvedBinding,
    manifest: &Manifest,
    analysis: &Analysis,
) -> String {
    let mut values = Map::new();
    for view in &analysis.values {
        let contributors: Vec<Value> = view
            .contributors
            .iter()
            .map(|contributor| {
                json!({
                    "layer": contributor.layer,
                    "kind": contributor.kind.as_str(),
                    "priority": contributor.priority,
                    "value": contributor.value,
                    "winner": contributor.winner,
                })
            })
            .collect();
        values.insert(
            view.key.clone(),
            json!({
                // Both null for a key carrying a defect: the merge threw and produced no value, and
                // naming a winner anyway would resurrect the declaration-order tiebreak ADR-0042
                // removed. The contributors stay, which is the whole point of this view.
                "effective": view.effective.clone().unwrap_or(Value::Null),
                "winner": view.winner.clone().map_or(Value::Null, Value::from),
                "contributors": contributors,
            }),
        );
    }
    let conflicts: Vec<Value> = analysis
        .conflicts
        .iter()
        .map(|conflict| {
            json!({
                "kind": conflict.kind.as_str(),
                "key": conflict.key,
                "layers": conflict.layers,
            })
        })
        .collect();
    line(&json!({
        "manifest": binding.manifest,
        "image": manifest.image,
        "pieces": manifest.pieces,
        "values": values,
        "conflicts": conflicts,
    }))
}

/// `viv config sources` — one block per key: the effective value, then every layer under it.
pub fn config_sources_human(analysis: &Analysis) -> String {
    let mut rendered = String::new();
    for view in &analysis.values {
        if view.contributors.is_empty() {
            continue;
        }
        let _ = writeln!(
            rendered,
            "{} = {}",
            view.key,
            view.effective
                .as_ref()
                .map_or_else(|| "(no value)".to_owned(), scalar)
        );
        for contributor in &view.contributors {
            let _ = writeln!(
                rendered,
                "  {:<16} {:<9} {:<10} {:<12} {}",
                contributor.layer,
                contributor.kind.as_str(),
                contributor.priority,
                scalar(&contributor.value),
                // A list has no loser: every contributor's elements are in the result, so calling
                // one shadowed would report a collision where the layers cooperated.
                if view.class == KeyClass::List {
                    "(merged)"
                } else if contributor.winner {
                    "(winner)"
                } else {
                    "(shadowed)"
                }
            );
        }
        rendered.push('\n');
    }
    rendered
}

/// The `[tie]` marker, which goes to stderr in both output modes.
///
/// stderr rather than stdout so `… --json 2>/dev/null | jq` stays clean, which is spec/01's reason;
/// the same placement in human mode keeps one behavior rather than two. A script detects a defect
/// through `conflicts`, never by scanning for this.
pub fn tie_notes(analysis: &Analysis) -> String {
    let mut rendered = String::new();
    for conflict in &analysis.conflicts {
        if conflict.kind != ConflictKind::Tie {
            continue;
        }
        let _ = writeln!(
            rendered,
            "[tie] {}   (equal priority — evaluation will fail)",
            conflict.key
        );
        if let Some(view) = analysis.value_of(&conflict.key) {
            for contributor in &view.contributors {
                let _ = writeln!(
                    rendered,
                    "  {:<16} {:<9} {:<10} {}",
                    contributor.layer,
                    contributor.kind.as_str(),
                    contributor.priority,
                    scalar(&contributor.value)
                );
            }
        }
        let _ = writeln!(
            rendered,
            concat!(
                "  hint: a shared piece should propose with mkDefault so your manifest can\n",
                "        decide; failing that, drop one piece or override through extends"
            )
        );
    }
    rendered
}

/// One `[name]` table, omitted entirely when it would carry no key.
fn table(rendered: &mut String, name: &str, entries: &[(&str, Option<&Value>)]) {
    let present: Vec<(&str, &Value)> = entries
        .iter()
        .filter_map(|(key, value)| {
            value.and_then(|value| present(value).map(|value| (*key, value)))
        })
        .collect();
    if present.is_empty() {
        return;
    }
    if !rendered.is_empty() {
        rendered.push('\n');
    }
    let _ = writeln!(rendered, "[{name}]");
    let width = present.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in present {
        let _ = writeln!(rendered, "{key:<width$} = {}", scalar(value));
    }
}

/// One `[[name]]` array of tables, one block per element.
fn array_of_tables(rendered: &mut String, name: &str, value: Option<&Value>, fields: &[&str]) {
    let Some(elements) = value.and_then(Value::as_array) else {
        return;
    };
    let width = fields.iter().map(|field| field.len()).max().unwrap_or(0);
    for element in elements {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        let _ = writeln!(rendered, "[[{name}]]");
        for field in fields {
            let Some(field_value) = element.get(*field).and_then(present) else {
                continue;
            };
            let _ = writeln!(rendered, "{field:<width$} = {}", scalar(field_value));
        }
    }
}

/// A value worth printing, which an absent declaration is not.
const fn present(value: &Value) -> Option<&Value> {
    match value {
        Value::Null => None,
        Value::Array(elements) if elements.is_empty() => None,
        value => Some(value),
    }
}

/// One value in TOML spelling, which is JSON's for everything the manifest surface admits.
fn scalar(value: &Value) -> String {
    value.to_string()
}

/// `viv manifest list --json` — one row per defined manifest, wrapped in a named key.
///
/// Wrapped rather than a bare array so later metadata can join it without a breaking re-wrap, which
/// is the rule spec/01 states once for every list this CLI emits.
pub fn manifest_list_json(rows: &[(String, ResolvedArtifact, Manifest)]) -> String {
    let manifests: Vec<Value> = rows
        .iter()
        .map(|(name, selected, manifest)| {
            json!({
                "name": name,
                "path": display(&selected.path),
                "image": manifest.image,
                "pieces": manifest.pieces,
            })
        })
        .collect();
    line(&json!({ "manifests": manifests }))
}

/// `viv manifest list` — one row per line, and nothing at all when the library is empty.
pub fn manifest_list_human(
    rows: &[(String, ResolvedArtifact, Manifest)],
    palette: &Palette,
) -> String {
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|(name, _, manifest)| {
            let mut row = vec![
                palette.accent.apply_to(name).to_string(),
                manifest.image.clone(),
            ];
            if !manifest.pieces.is_empty() {
                row.push(
                    palette
                        .label
                        .apply_to(manifest.pieces.join(", "))
                        .to_string(),
                );
            }
            row
        })
        .collect();
    table::columns(&table_rows)
}

/// `viv volume list --json` — one row per volume, wrapped for the reason `manifests` is.
///
/// The rows arrive already shaped: which columns a volume has is spec/01's business and the
/// command's, and duplicating the key names here would put the published half in two places.
pub fn volume_list_json(rows: &[Value]) -> String {
    line(&json!({ "volumes": rows }))
}

/// `viv volume list` — one row per line, and nothing at all when there is nothing to say.
pub fn volume_list_human(rows: &[Vec<String>], palette: &Palette) -> String {
    let styled: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(index, cell)| {
                    if index == 0 {
                        palette.accent.apply_to(cell).to_string()
                    } else {
                        cell.clone()
                    }
                })
                .collect()
        })
        .collect();
    table::columns(&styled)
}

/// `viv volume prune --json` — the rows removed, and what that reclaimed.
///
/// `reclaimed_bytes` sits beside the list rather than inside it because it is a property of the
/// run: spec/01 fixes `{"volumes": [], "reclaimed_bytes": 0}` as the nothing-to-do record, and a
/// per-row total could not express that.
pub fn volume_prune_json(rows: &[Value], reclaimed_bytes: u64) -> String {
    line(&json!({ "volumes": rows, "reclaimed_bytes": reclaimed_bytes }))
}

/// `viv volume prune` — the same rows the prompt showed, then the measurement.
///
/// The measurement always prints, including the zero: spec/01 lists `prune` among the commands
/// that print their measurement on stdout, and "nothing to prune" is a result rather than silence.
pub fn volume_prune_human(rows: &[String], reclaimed_bytes: u64) -> String {
    let mut rendered = String::new();
    for row in rows {
        rendered.push_str(row);
        rendered.push('\n');
    }
    let _ = writeln!(rendered, "reclaimed\t{reclaimed_bytes}");
    rendered
}

/// `viv generations list --json` — one row per retained generation, wrapped for the reason
/// every list this CLI emits is (spec/01).
pub fn generations_list_json(rows: &[Value]) -> String {
    line(&json!({ "generations": rows }))
}

/// `viv generations list` — one row per line, oldest first, nothing at all for a project never
/// built.
pub fn generations_list_human(rows: &[Vec<String>], palette: &Palette) -> String {
    volume_list_human(rows, palette)
}

/// `viv generations prune --json` — the rows unlinked, and how many stayed.
///
/// `kept` sits beside the list for the reason `reclaimed_bytes` does on `volume prune`: it is a
/// property of the run, and nothing-to-do is `{"generations": [], "kept": n}` at `0`.
pub fn generations_prune_json(rows: &[Value], kept: usize) -> String {
    line(&json!({ "generations": rows, "kept": kept }))
}

/// `viv generations prune` — the rows unlinked, then the count that stayed. The count always
/// prints, including after a run that unlinked nothing: "nothing to prune" is a result.
pub fn generations_prune_human(rows: &[Vec<String>], kept: usize) -> String {
    let mut rendered = table::columns(rows);
    let _ = writeln!(rendered, "kept	{kept}");
    rendered
}

/// `viv generations activate`/`rollback` `--json` — what moved, and from where.
pub fn generations_switch_json(action: &str, current: u64, previous: Option<u64>) -> String {
    line(&json!({ "action": action, "current": current, "previous": previous }))
}

/// The human form of the same move: one line naming both ends.
pub fn generations_switch_human(current: u64, previous: Option<u64>) -> String {
    match previous {
        Some(previous) if previous != current => {
            format!("current: generation {previous} -> generation {current}\n")
        }
        _ => format!("current: generation {current}\n"),
    }
}

/// `viv update --json` — the lock written and one row per reported input (spec/01): the
/// requested set whole, including unchanged rows, plus any public input whose pin moved by
/// following a requested one. `before` is `null` for a newly created pin, and a hashless
/// relative base reports `null` on both sides because its content is a layer (ADR-0112).
pub fn update_json(manifest: &str, lock: &Path, rows: &[Value]) -> String {
    line(&json!({ "manifest": manifest, "lock": lock.display().to_string(), "inputs": rows }))
}

/// `viv update` — one line per reported input, then the lock that holds the result.
pub fn update_human(rows: &[Vec<String>], lock: &Path, palette: &Palette) -> String {
    let mut rendered = volume_list_human(rows, palette);
    let _ = writeln!(rendered, "lock\t{}", lock.display());
    rendered
}

/// `viv gc --json` — the collector's own accounting line, relayed rather than re-derived.
pub fn gc_json(summary: Option<&str>) -> String {
    line(&json!({ "summary": summary }))
}

/// `viv gc` — the same line for a human, with a fallback for a collector that said nothing.
pub fn gc_human(summary: Option<&str>) -> String {
    format!("{}\n", summary.unwrap_or("collected"))
}

/// `viv manifest show --json` — one manifest as authored.
pub fn manifest_show_json(name: &str, selected: &ResolvedArtifact, manifest: &Manifest) -> String {
    line(&json!({
        "manifest": name,
        "path": display(&selected.path),
        "image": manifest.image,
        "pieces": manifest.pieces,
        "resources": resources_json(manifest.resources.as_ref()),
        "egress": egress_json(manifest.egress.as_ref()),
        "extends": manifest.extends,
    }))
}

/// `viv manifest show` — the same, in the manifest's own authoring language.
pub fn manifest_show_human(name: &str, selected: &ResolvedArtifact, manifest: &Manifest) -> String {
    let mut rendered = format!(
        "manifest: {name}\npath: {}\nimage: {}\n",
        selected.path.display(),
        manifest.image
    );
    if !manifest.pieces.is_empty() {
        let _ = writeln!(rendered, "pieces: {}", manifest.pieces.join(", "));
    }
    if let Some(extends) = &manifest.extends {
        let _ = writeln!(rendered, "extends: {extends}");
    }
    if let Some(resources) = &manifest.resources {
        rendered.push_str("\n[resources]\n");
        if let Some(mem_mib) = resources.mem_mib {
            let _ = writeln!(rendered, "mem_mib = {mem_mib}");
        }
        if let Some(vcpu) = resources.vcpu {
            let _ = writeln!(rendered, "vcpu = {vcpu}");
        }
    }
    if let Some(egress) = &manifest.egress {
        rendered.push_str("\n[egress]\n");
        if let Some(mode) = egress.mode {
            let _ = writeln!(rendered, "mode = \"{}\"", egress_mode(mode));
        }
        if !egress.allow.is_empty() {
            let _ = writeln!(rendered, "allow = {:?}", egress.allow);
        }
    }
    rendered
}

/// The `resources` object, always present with both keys.
///
/// Present-with-nulls rather than omitted, because a consumer addressing `resources.mem_mib` needs
/// the object to exist whether or not the manifest declared one; `null` is what says "undeclared"
/// without inventing the host-resolved number this view has no business guessing.
fn resources_json(resources: Option<&Resources>) -> Value {
    let mut object = Map::new();
    object.insert(
        "mem_mib".to_owned(),
        resources
            .and_then(|r| r.mem_mib)
            .map_or(Value::Null, Into::into),
    );
    object.insert(
        "vcpu".to_owned(),
        resources
            .and_then(|r| r.vcpu)
            .map_or(Value::Null, Into::into),
    );
    Value::Object(object)
}

/// The `egress` object, always present with both keys.
fn egress_json(egress: Option<&Egress>) -> Value {
    let mut object = Map::new();
    object.insert(
        "mode".to_owned(),
        egress
            .and_then(|e| e.mode)
            .map_or(Value::Null, |mode| Value::from(egress_mode(mode))),
    );
    // `allow` is a list, and an undeclared list and an empty one mean the same thing to every
    // reader of it — so this one flattens rather than carrying a null the caller must handle.
    object.insert(
        "allow".to_owned(),
        Value::from(egress.map(|e| e.allow.clone()).unwrap_or_default()),
    );
    Value::Object(object)
}

const fn egress_mode(mode: EgressMode) -> &'static str {
    match mode {
        EgressMode::Open => "open",
        EgressMode::Allowlist => "allowlist",
    }
}

/// `viv status --json` — one record of operational state (spec/01 "Status output").
///
/// Every key is present whatever the state, because a consumer needs a stable shape. A field the
/// state cannot fill honestly is `null` rather than fabricated: `generation` for a project never
/// built, `reason` outside `failed`.
///
/// `resources` and `runtime` stay separate objects even when both are absent. spec/01 is explicit
/// that a consumer must never have to guess which it is holding, so collapsing them while one
/// happens to be empty would publish a shape that changes meaning with the state.
pub fn status_json(report: &Report) -> String {
    line(&json!({
        "manifest": report.manifest,
        "state": report.state.as_str(),
        // Required whenever `state` is `failed` (spec/01), and present as `null` otherwise for the
        // same reason `generation` is: a key that appears and disappears is a shape a consumer has
        // to branch on before it can read the record.
        "reason": report.reason,
        // Meaningful only while running (ADR-0030), and reported as a boolean regardless so the
        // key does not appear and disappear.
        "stale": report.stale,
        "generation": report.generation,
        "store_path": report.store_path,
        "uptime_seconds": report.uptime_seconds,
        "resources": report.resources.as_ref().map(|resources| json!({
            "mem_mib": resources.mem_mib,
            "vcpu": resources.vcpu,
        })),
        // The running VM's record generation when another vivarium version wrote it, and `null`
        // for a VM this version booted. Beside `running` on purpose: `status` keeps answering
        // where the session verbs refuse with `78`.
        "record_schema_skew": report.record_skew.map(|theirs| json!({
            "record": theirs,
            "binary": crate::launch::LAUNCH_SCHEMA_VERSION,
        })),
        "runtime": runtime_json(&report.runtime),
    }))
}

/// The measured `runtime` object both status faces share (spec/01, spec/17).
///
/// Per-field `null` rather than a vanishing object, for the same stable-shape reason the record
/// itself keeps every key: an unavailable reading and a stopped VM both leave a field a consumer
/// can still address.
pub fn runtime_json(readings: &RuntimeReadings) -> Value {
    json!({
        "mem_used_bytes": readings.mem_used_bytes,
        "disk_allocated_bytes": readings.disk_allocated_bytes,
        "disk_virtual_bytes": readings.disk_virtual_bytes,
        "sessions": readings.sessions,
        "pressure_some_avg60": readings.pressure_some_avg60,
    })
}

/// `viv status` — the human reading of the same record: dim labels, the state carrying the one
/// colored glyph, durations humanized. spec/01 fixes what it names, not how it lays out.
pub fn status_human(report: &Report, palette: &Palette) -> String {
    let mut rows: Vec<(&str, String)> = Vec::new();
    if let Some(manifest) = &report.manifest {
        rows.push(("manifest", palette.accent.apply_to(manifest).to_string()));
    }
    rows.push(("state", state_view(report.state.as_str(), palette)));
    if let Some(reason) = report.reason {
        rows.push(("reason", reason.to_owned()));
    }
    if let Some(theirs) = report.record_skew {
        rows.push((
            "record",
            format!(
                "launch schema {theirs}; this viv speaks {}. `viv stop`, then `viv start`, \
                reboots it under this version",
                crate::launch::LAUNCH_SCHEMA_VERSION
            ),
        ));
    }
    if let Some(store_path) = &report.store_path {
        rows.push(("build", store_path.clone()));
    }
    if let Some(uptime) = report.uptime_seconds {
        rows.push(("uptime", duration(uptime)));
    }
    // The ceiling beside what is actually being used (spec/17): a row appears as soon as either
    // half exists, and the half that does not reads `-` rather than a fabricated number.
    if report.resources.is_some() || report.runtime.mem_used_bytes.is_some() {
        let used = report
            .runtime
            .mem_used_bytes
            .map_or_else(|| "-".to_owned(), bytes);
        let ceiling = report.resources.as_ref().map_or_else(
            || "-".to_owned(),
            |resources| bytes(resources.mem_mib * 1024 * 1024),
        );
        rows.push(("memory", format!("{used} used / {ceiling} ceiling")));
    }
    if let Some(resources) = &report.resources {
        rows.push(("vcpu", resources.vcpu.to_string()));
    }
    if let Some(sessions) = report.runtime.sessions {
        rows.push(("sessions", sessions.to_string()));
    }
    // Present in every state, because volumes persist across `stop` (N18) and zero occupancy is
    // a reading; `-` is volume state that could not be read.
    rows.push((
        "disk",
        match (
            report.runtime.disk_allocated_bytes,
            report.runtime.disk_virtual_bytes,
        ) {
            (Some(allocated), Some(apparent)) => format!(
                "{} allocated / {} virtual",
                bytes(allocated),
                bytes(apparent)
            ),
            _ => "-".to_owned(),
        },
    ));
    let mut rendered = table::record(palette, &rows);
    if report.stale {
        // The remedy beside the fact, because a stale VM is a state a user acts on rather than one
        // they only read (spec/01).
        let _ = writeln!(
            rendered,
            "\n{} this VM is stale: a newer build exists. `viv start --rebuild` replaces it",
            palette.warn.apply_to("!"),
        );
    }
    rendered
}

/// The state cell: one glyph, colored by what the state means, beside the word itself.
fn state_view(state: &str, palette: &Palette) -> String {
    let style = match state {
        "running" => &palette.good,
        "failed" => &palette.error,
        "starting" | "stopping" => &palette.warn,
        "built" => &palette.accent,
        _ => &palette.label,
    };
    format!("{} {state}", style.apply_to("●"))
}

/// A byte count in the table's own units: GiB to one decimal from a gibibyte up (a whole GiB
/// unadorned), whole MiB below that, raw bytes below a mebibyte. The JSON face keeps the raw
/// number.
pub(super) fn bytes(count: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if count >= GIB {
        let tenths = count / (GIB / 10);
        if tenths.is_multiple_of(10) {
            format!("{} GiB", tenths / 10)
        } else {
            format!("{}.{} GiB", tenths / 10, tenths % 10)
        }
    } else if count >= MIB {
        format!("{} MiB", count / MIB)
    } else {
        format!("{count} B")
    }
}

/// `viv stop --json` — the record spec/01 fixes: the resulting state and the rung that ended it.
///
/// `rung` is `null` for the idempotent no-op — nothing was running, so no rung ran. The state is
/// re-discriminated after the teardown, so the record reports what a `status` run now would.
pub fn stop_json(manifest: &str, state: &str, rung: Option<&'static str>) -> String {
    line(&stop_row(manifest, state, rung))
}

/// One sandbox's stop record — the object `stop_json` wraps and the sweep's rows repeat.
pub fn stop_row(manifest: &str, state: &str, rung: Option<&'static str>) -> Value {
    json!({
        "manifest": manifest,
        "state": state,
        "rung": rung,
    })
}

/// `viv stop --all --json` — the acted-on sandboxes under the fleet's own key (spec/01).
///
/// Rows only for sandboxes the sweep acted on: a resting sandbox was not stopped by this
/// invocation, and an empty sweep renders an empty list rather than nothing.
pub fn stop_all_json(rows: &[Value]) -> String {
    line(&json!({ "projects": rows }))
}

/// `viv volume trim --json` — the rows under their named key, and the sum beside them.
///
/// The top-level `reclaimed_bytes` is the sum of the rows so the common question needs no
/// client-side arithmetic, and `{"volumes": [], "reclaimed_bytes": 0}` is the record of a
/// project whose volumes were never materialized (spec/01).
pub fn volume_trim_json(rows: &[Value]) -> String {
    line(&volume_trim_record(rows))
}

/// The object `volume_trim_json` wraps and the `viv trim` fan-out nests verbatim (spec/01).
pub fn volume_trim_record(rows: &[Value]) -> Value {
    let reclaimed: u64 = rows
        .iter()
        .filter_map(|row| row["reclaimed_bytes"].as_u64())
        .sum();
    json!({ "volumes": rows, "reclaimed_bytes": reclaimed })
}

/// `viv volume trim` — one row per volume, then a line naming what was measured.
///
/// The rows carry raw byte counts like `volume list`'s; the summary line spells the figure in
/// the table units and names the metric, because a reclaimed figure whose metric is unnamed is
/// exactly what ADR-0082 warns reads as success that did not happen.
pub fn volume_trim_human(rows: &[String], reclaimed_bytes: u64) -> String {
    let mut rendered = String::new();
    for row in rows {
        rendered.push_str(row);
        rendered.push('\n');
    }
    let _ = writeln!(
        rendered,
        "reclaimed {} of image allocation",
        bytes(reclaimed_bytes)
    );
    rendered
}

/// `viv memory trim --json` — spec/01's record: the sandbox key, then the measured object.
pub fn memory_trim_json(manifest: &str, reading: &super::trim::MemoryTrimReading) -> String {
    let mut record = Map::new();
    record.insert("manifest".to_owned(), Value::String(manifest.to_owned()));
    if let Value::Object(measured) = memory_trim_record(reading) {
        record.extend(measured);
    }
    line(&Value::Object(record))
}

/// The measured object alone — what the `viv trim` fan-out nests verbatim under `memory`,
/// with the sandbox key hoisted (spec/01).
pub fn memory_trim_record(reading: &super::trim::MemoryTrimReading) -> Value {
    json!({
        "target_mib": reading.target_mib,
        "mem_used_before_bytes": reading.before,
        "mem_used_after_bytes": reading.after,
        // Floored at zero: the command's promise is what the host got back, not a signed
        // account, so a guest that grew during the operation reports nothing (spec/01).
        "reclaimed_bytes": reading.before.saturating_sub(reading.after),
    })
}

/// `viv memory trim` — one line naming what the host got back, and on what instrument.
pub fn memory_trim_line(reading: &super::trim::MemoryTrimReading) -> String {
    format!(
        "reclaimed {} of host memory (sandbox scope charge {} -> {}, target {} MiB)\n",
        bytes(reading.before.saturating_sub(reading.after)),
        bytes(reading.before),
        bytes(reading.after),
        reading.target_mib
    )
}

/// `viv trim --json` — one subtree per resource, each verbatim its own command's record, the
/// sandbox key hoisted, and deliberately no top-level total: host memory bytes and
/// image-allocated bytes are different quantities whose sum names nothing a reader could check
/// (ADR-0113, spec/01).
pub fn trim_json(
    manifest: &str,
    memory: &super::trim::MemoryTrimReading,
    volume_rows: &[Value],
) -> String {
    line(&json!({
        "manifest": manifest,
        "memory": memory_trim_record(memory),
        "disk": volume_trim_record(volume_rows),
    }))
}

/// `viv destroy --json` — this run's removal plan as executed, beside what it spared.
///
/// A path the plan names outright stays in `removed` even when already absent (the idempotent
/// success spec/10 fixes), while the `--keep-volumes` carve-out reports only what was actually
/// found beside the kept name at run time (spec/01).
pub fn destroy_json(
    manifest: &str,
    removed: &[std::path::PathBuf],
    spared: &[std::path::PathBuf],
    volumes_kept: bool,
) -> String {
    let paths = |list: &[std::path::PathBuf]| -> Vec<Value> {
        list.iter()
            .map(|path| Value::String(path.display().to_string()))
            .collect()
    };
    line(&json!({
        "manifest": manifest,
        "removed": paths(removed),
        "spared": paths(spared),
        "volumes_kept": volumes_kept,
    }))
}

/// `viv status -g --json` — the fleet under its named key beside the host's own reading.
///
/// `host` is always present, even over an empty fleet: it is what lets one command answer
/// whether the host is overcommitted (spec/01).
pub fn fleet_json(projects: &[Value], host: &Value) -> String {
    line(&json!({ "projects": projects, "host": host }))
}

/// `viv status -g` — the fleet table under its header, then the host line (spec/17).
///
/// An empty fleet renders the host line alone: the table would be a header over nothing.
pub fn fleet_human(rows: &[Vec<String>], host_line: &str, palette: &Palette) -> String {
    let mut rendered = String::new();
    if rows.len() > 1 {
        let styled: Vec<Vec<String>> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                row.iter()
                    .enumerate()
                    .map(|(column, cell)| {
                        if index == 0 {
                            palette.label.apply_to(cell).to_string()
                        } else if column == 0 {
                            palette.accent.apply_to(cell).to_string()
                        } else {
                            cell.clone()
                        }
                    })
                    .collect()
            })
            .collect();
        rendered.push_str(&table::columns(&styled));
        rendered.push('\n');
    }
    rendered.push_str(host_line);
    rendered.push('\n');
    rendered
}

/// Seconds, humanized: `13s`, `2m 14s`, `3h 21m`. The JSON face keeps the raw number.
pub(super) fn duration(seconds: u64) -> String {
    let (hours, minutes, rest) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {rest}s")
    } else {
        format!("{rest}s")
    }
}

/// `viv doctor` — the category-grouped report with bracketed word markers, never glyphs
/// (spec/13), and the closing summary line naming the exit.
pub fn doctor_human(
    findings: &[crate::doctor::Finding],
    code: crate::exit::ExitKind,
    palette: &Palette,
) -> String {
    use crate::doctor::Status;
    let id_width = findings
        .iter()
        .map(|finding| finding.probe.id.len())
        .max()
        .unwrap_or(0);
    // Grouped by category — each category once, in first-appearance order, holding every one of
    // its findings in catalog order — because the catalog interleaves categories and a header per
    // run of consecutive rows would print `permissions` four times.
    let mut categories = Vec::new();
    for finding in findings {
        if !categories.contains(&finding.probe.category) {
            categories.push(finding.probe.category);
        }
    }
    let mut rendered = String::new();
    for (index, category) in categories.iter().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        let _ = writeln!(rendered, "{}", palette.label.apply_to(category.as_str()));
        for finding in findings
            .iter()
            .filter(|finding| finding.probe.category == *category)
        {
            let marker = format!("[{}]", finding.status.as_str());
            let style = match finding.status {
                Status::Pass => &palette.good,
                Status::Warn => &palette.warn,
                Status::Fail => &palette.error,
                Status::Skipped => &palette.label,
            };
            let message = doctor_message(finding);
            let _ = writeln!(
                rendered,
                "  {}  {:<id_width$}  {message}",
                style.apply_to(format!("{marker:<9}")),
                finding.probe.id,
            );
            if let Some(hint) = &finding.hint {
                let _ = writeln!(
                    rendered,
                    "{}{} {hint}",
                    " ".repeat(13 + 4),
                    palette.label.apply_to("hint:"),
                );
            }
        }
    }
    let (mut passed, mut warned, mut failed, mut skipped) = (0u32, 0u32, 0u32, 0u32);
    for finding in findings {
        match finding.status {
            Status::Pass => passed += 1,
            Status::Warn => warned += 1,
            Status::Fail => failed += 1,
            Status::Skipped => skipped += 1,
        }
    }
    let _ = writeln!(
        rendered,
        "\n{passed} pass - {warned} warn - {failed} fail - {skipped} skipped -> exit {}",
        code.code(),
    );
    rendered
}

/// What a report row says: the finding's message, or a skip reason's fixed wording.
fn doctor_message(finding: &crate::doctor::Finding) -> String {
    if finding.status == crate::doctor::Status::Skipped && finding.message.is_empty() {
        return match finding.reason {
            Some("no-manifest-bound") => "no manifest declares this workspace".to_owned(),
            Some("offline-mode") => "offline - run with `--online`".to_owned(),
            Some(reason) => reason.to_owned(),
            None => String::new(),
        };
    }
    finding.message.clone()
}

/// `viv doctor --json` — the enveloped record spec/13 fixes, with `summary.hard_failures` so a
/// script gates without re-deriving severity, and `schema_version` because the envelope carries
/// one (unlike the reader records).
pub fn doctor_json(findings: &[crate::doctor::Finding]) -> String {
    use crate::doctor::{Severity, Status};
    let checks: Vec<Value> = findings
        .iter()
        .map(|finding| {
            let mut check = Map::new();
            check.insert("id".to_owned(), finding.probe.id.into());
            check.insert(
                "category".to_owned(),
                finding.probe.category.as_str().into(),
            );
            check.insert("scope".to_owned(), finding.probe.scope.as_str().into());
            check.insert(
                "severity".to_owned(),
                finding.probe.severity.as_str().into(),
            );
            check.insert("status".to_owned(), finding.status.as_str().into());
            if !finding.message.is_empty() {
                check.insert("message".to_owned(), finding.message.clone().into());
            }
            if let Some(hint) = &finding.hint {
                check.insert("hint".to_owned(), hint.clone().into());
            }
            if let Some(reason) = finding.reason {
                check.insert("reason".to_owned(), reason.into());
            }
            // `doc_url` is omitted, never null: the path scheme is fixed but no site exists yet,
            // and consumers test key presence (spec/13).
            Value::Object(check)
        })
        .collect();
    let hard_failures = findings
        .iter()
        .filter(|finding| {
            finding.status == Status::Fail && finding.probe.severity == Severity::Hard
        })
        .count();
    let count = |status: Status| {
        findings
            .iter()
            .filter(|finding| finding.status == status)
            .count()
    };
    let overall = if hard_failures > 0 {
        "fail"
    } else if count(Status::Warn) > 0 {
        "warn"
    } else {
        "pass"
    };
    line(&json!({
        "status": overall,
        "checks": checks,
        "summary": {
            "total": findings.len(),
            "passed": count(Status::Pass),
            "warned": count(Status::Warn),
            "failed": count(Status::Fail),
            "skipped": count(Status::Skipped),
            "hard_failures": hard_failures,
        },
        "schema_version": "1",
    }))
}

/// `viv doctor --list` — the catalog enumerated, nothing probed.
pub fn doctor_list_human(palette: &Palette) -> String {
    let rows: Vec<Vec<String>> = crate::doctor::CATALOG
        .iter()
        .map(|probe| {
            vec![
                palette.accent.apply_to(probe.id).to_string(),
                probe.category.as_str().to_owned(),
                probe.scope.as_str().to_owned(),
                probe.severity.as_str().to_owned(),
                probe.title.to_owned(),
            ]
        })
        .collect();
    table::columns(&rows)
}

/// `viv doctor --list --json` — the same enumeration, one keyed object.
pub fn doctor_list_json() -> String {
    let checks: Vec<Value> = crate::doctor::CATALOG
        .iter()
        .map(|probe| {
            json!({
                "id": probe.id,
                "category": probe.category.as_str(),
                "scope": probe.scope.as_str(),
                "severity": probe.severity.as_str(),
                "title": probe.title,
            })
        })
        .collect();
    line(&json!({ "checks": checks }))
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Machine output is one object and one newline. `--json` emits exactly one line so a consumer can
/// read it without knowing how long it is.
fn line(value: &Value) -> String {
    format!("{value}\n")
}

#[cfg(test)]
mod tests {
    use super::bytes;

    /// The unit ladder spec/17's table shows: GiB to one decimal, MiB whole, bytes raw.
    #[test]
    fn bytes_take_the_tables_units() {
        assert_eq!(bytes(2_254_857_830), "2.1 GiB");
        assert_eq!(bytes(8 * 1024 * 1024 * 1024), "8 GiB");
        assert_eq!(bytes(34_359_738_368), "32 GiB");
        assert_eq!(bytes(512 * 1024 * 1024), "512 MiB");
        assert_eq!(bytes(900), "900 B");
        assert_eq!(bytes(0), "0 B");
    }
}
