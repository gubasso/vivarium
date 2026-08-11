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
mod flake;
mod identity;
mod input;
mod manifest;
mod materialize;
// Public as a module rather than through flat re-exports: its verbs are `read`, `parse`, and
// `update`, which say what they mean only next to the noun they act on.
pub mod registry;
mod resolve;
mod roots;

#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactForm, ArtifactKind, ResolvedArtifact, resolve_artifact};
pub use error::{GeneratedFlakeError, InputError, ManifestError, RegistryError, ResolutionError};
pub use flake::{
    EffectiveLock, FlakeInput, GeneratedFlakePaths, GeneratedFlakePlan, PreparedFlake,
    ResolvedComposition, target_paths,
};
pub use identity::sanitize_project_name;
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
pub use roots::{Environment, XdgRoots, resolve_runtime_root, resolve_xdg_roots};
