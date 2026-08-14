//! Host-side configuration path and name resolution.
//!
//! Path and name resolution create nothing: the config root is the complete artifact search path
//! fixed by N13 and ADR-0061, with no bundled fallback and no write beneath it. The one boundary
//! here that does write is the state registry, which is the exception ADR-0011 draws — the tool
//! never writes config, so the project→manifest binding lives in state instead, behind an explicit
//! `viv init --write`. Project identity and its collision allocation are still later slice work
//! and are deliberately absent.

mod artifact;
mod atomic;
mod error;
// Public as modules rather than through flat re-exports: their names say what they mean only next
// to the noun they belong to — `evaluate::report`, `merged::analyze`.
pub mod evaluate;
mod flake;
mod identity;
mod input;
mod manifest;
mod materialize;
pub mod merged;
// Public as a module rather than through flat re-exports: its verbs are `read`, `parse`, and
// `update`, which say what they mean only next to the noun they act on.
pub mod registry;
mod resolve;
mod roots;
// Public as a module for the reason `registry` is: `read`, `parse`, and `write` say what they mean
// only next to the noun they act on, and `volumes::read` reads better than `read_volume_record`.
pub mod volumes;

#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactForm, ArtifactKind, ResolvedArtifact, resolve_artifact};
pub use error::{
    EvaluationError, GeneratedFlakeError, InputError, ManifestError, RegistryError, ResolutionError,
};
pub use flake::{
    BASELINE_MICROVM_VARIABLE, BASELINE_NIXPKGS_VARIABLE, BaselineInputs, EffectiveLock,
    FlakeInput, GeneratedFlakePaths, GeneratedFlakePlan, PreparedFlake, ResolvedComposition,
    target_paths,
};
pub use identity::{
    Forgotten, IDENTITY_FILE, Identity, IdentityIndex, MARKER_DIR, forget as forget_identity,
    identity_path, mint as mint_identity, project_basename, read_index as read_identity_index,
    resolve as resolve_identity, sanitize_project_name,
};
pub use manifest::{
    DefaultVolume, Egress, EgressMode, Manifest, ManifestOrigin, Mount, Resources, Volume,
    parse_manifest,
};
pub use materialize::{persist_created_lock, prepare_generated_flake};
pub use registry::{Binding, REGISTRY_FILE, Registry, registry_path};
pub use resolve::{
    BindingSource, MANIFEST_VARIABLE, ResolvedBinding, canonical_project, resolve_binding,
    unbound_hint,
};
pub use roots::{
    Environment, XdgRoots, effective_gid, effective_uid, resolve_runtime_root, resolve_xdg_roots,
};
