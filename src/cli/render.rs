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

/// `viv config` — the same record, one fact per line.
pub fn binding_human(
    binding: &ResolvedBinding,
    roots: &XdgRoots,
    flake: &Path,
    lock: &Path,
) -> String {
    let mut rendered = format!(
        "manifest: {}\nsource:   {}\n",
        binding.manifest,
        binding.source.as_str()
    );
    for (label, path) in [
        ("config", roots.config.as_path()),
        ("state", roots.state.as_path()),
        ("data", roots.data.as_path()),
        ("cache", roots.cache.as_path()),
        ("flake", flake),
        ("lock", lock),
    ] {
        let _ = writeln!(rendered, "{label:<9} {}", path.display());
    }
    rendered
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

/// `viv init --json`, read-only: what would be recorded, having recorded nothing.
pub fn init_preview_json(manifest: &str, project: &Path, path: &Path) -> String {
    init_json(manifest, project, path, false)
}

/// `viv init --write --json`: what was recorded.
pub fn init_written_json(manifest: &str, project: &Path, path: &Path) -> String {
    init_json(manifest, project, path, true)
}

/// One shape for both, because the two differ in exactly one field.
///
/// `written` is that field, and it is a fact rather than a formality: spec/01 makes the read-only
/// form and the `--write` form equally supported, so a consumer has to be able to tell which one
/// it just ran without inferring it from the flags it passed.
fn init_json(manifest: &str, project: &Path, path: &Path, written: bool) -> String {
    line(&json!({
        "manifest": manifest,
        "path": display(path),
        "project": display(project),
        "written": written,
    }))
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
pub fn manifest_list_human(rows: &[(String, ResolvedArtifact, Manifest)]) -> String {
    let mut rendered = String::new();
    for (name, _, manifest) in rows {
        let _ = write!(rendered, "{name}\t{}", manifest.image);
        if !manifest.pieces.is_empty() {
            let _ = write!(rendered, "\t{}", manifest.pieces.join(", "));
        }
        rendered.push('\n');
    }
    rendered
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

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Machine output is one object and one newline. `--json` emits exactly one line so a consumer can
/// read it without knowing how long it is.
fn line(value: &Value) -> String {
    format!("{value}\n")
}
