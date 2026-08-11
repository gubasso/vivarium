//! Host-side configuration path and name resolution.
//!
//! This boundary resolves paths but never creates them. In particular, the config root is the
//! complete artifact search path fixed by N13 and ADR-0061: there is no bundled fallback and no
//! write beneath it. Stateful project identity, bindings, and collision allocation belong to later
//! slice work and are deliberately absent here.

mod artifact;
mod error;
mod flake;
mod identity;
mod input;
mod manifest;
mod materialize;
mod roots;

#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactForm, ArtifactKind, ResolvedArtifact, resolve_artifact};
pub use error::{GeneratedFlakeError, InputError, ManifestError, ResolutionError};
pub use flake::{
    EffectiveLock, FlakeInput, GeneratedFlakePlan, PreparedFlake, ResolvedComposition,
};
pub use identity::sanitize_project_name;
pub use manifest::{
    DefaultVolume, Egress, EgressMode, Manifest, ManifestOrigin, Mount, Resources, Volume,
    parse_manifest,
};
pub use materialize::{persist_created_lock, prepare_generated_flake};
pub use roots::{Environment, XdgRoots, resolve_runtime_root, resolve_xdg_roots};
