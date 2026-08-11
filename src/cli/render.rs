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
