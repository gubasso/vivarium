//! Host-side configuration path and name resolution.
//!
//! Path and name resolution create nothing: the config root is the complete artifact search path
//! fixed by N13 and ADR-0061, with no bundled fallback and no write beneath it. Workspace ownership
//! is derived from that library and cached under the cache root; the cache is never authority.

mod artifact;
mod atomic;
mod error;
// Public as modules rather than through flat re-exports: their names say what they mean only next
// to the noun they belong to — `evaluate::report`, `merged::analyze`.
pub mod evaluate;
mod flake;
// Public as a module because `generations::list` and `generations::append` read best beside the
// noun they act on, exactly as `volumes` does.
pub mod generations;
mod input;
mod lock;
mod manifest;
mod materialize;
pub mod merged;
// Public as a module rather than through flat re-exports: the index lifecycle reads best beside
// the noun it acts on.
pub mod registry;
mod resolve;
mod roots;
// Public as a module because `volumes::read` says what it reads better than a flat re-export.
pub mod volumes;

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactForm, ArtifactKind, ResolvedArtifact, artifact_names, resolve_artifact,
};
pub use error::{
    EvaluationError, GeneratedFlakeError, InputError, ManifestError, RegistryError, ResolutionError,
};
#[cfg(test)]
pub use flake::embedded_file;
pub use flake::{
    BASELINE_MICROVM_VARIABLE, BASELINE_NIXPKGS_VARIABLE, BaselineInputs, EffectiveLock,
    FlakeInput, GeneratedFlakePaths, GeneratedFlakePlan, PreparedFlake, ResolvedComposition,
    target_paths,
};
pub use manifest::{
    DefaultVolume, Egress, EgressMode, Manifest, ManifestOrigin, Mount, Resources, Volume,
    Workspace, parse_manifest,
};
pub use materialize::{persist_created_lock, prepare_generated_flake};
pub use registry::{INDEX_FILE, IndexedManifest, ManifestStamp, WorkspaceIndex, index_path};
pub use resolve::{
    BindingSource, MANIFEST_VARIABLE, ResolvedBinding, canonical_project, resolve_binding,
    unbound_hint,
};
pub use roots::{
    Environment, XdgRoots, effective_gid, effective_uid, resolve_runtime_root, resolve_xdg_roots,
};
