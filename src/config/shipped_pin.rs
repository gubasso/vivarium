//! The pin the binary ships (ADR-0116).
//!
//! `nix/flake.lock` is vivarium's one pin, and the host binary carries it compiled in. Two
//! readers use it, and nothing else does. The pre-flight hands its bytes to Nix as the reference
//! lock when a target has no lock in force, so the user's first lock is this pin. The renderer
//! reads each baseline branch reference from the `original` field of its root input, so
//! `nix/flake.nix` stays the one place that names a branch.
//!
//! The module compiles only with the `host` feature, which is why the guest-agent derivation in
//! `nix/default.nix` never needs the lock in its source: a lock move must not rebuild the guest.

use std::sync::OnceLock;

use serde_json::{Map, Value};

use super::GeneratedFlakeError;
use super::error::GeneratedFlakeErrorKind;
use crate::diagnostic::{Locus, Namespace};

/// The product lock, compiled in. The only place the crate names `nix/flake.lock`.
pub(super) const LOCK: &[u8] = include_bytes!("../../nix/flake.lock");

/// The two branch references every generated flake renders for its baseline inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Baseline {
    /// The `nixpkgs` flake reference.
    pub nixpkgs: String,
    /// The `microvm` flake reference.
    pub microvm: String,
}

/// The baseline references of the shipped pin, parsed once.
///
/// # Errors
///
/// Returns an internal [`GeneratedFlakeError`] when the shipped lock names no reference the
/// renderer can express. The unit trial `the_rendered_references_come_from_the_shipped_pin`
/// holds that this never reaches a user: the same bytes fail the crate's own tests first.
pub(super) fn baseline() -> Result<&'static Baseline, GeneratedFlakeError> {
    static PARSED: OnceLock<Result<Baseline, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| baseline_of(LOCK))
        .as_ref()
        .map_err(|reason| {
            GeneratedFlakeError::plain(
                GeneratedFlakeErrorKind::Internal,
                Namespace::Internal,
                "shipped-pin",
                Locus::Named("shipped pin"),
                "the pin this binary ships names no usable baseline reference",
                reason.clone(),
            )
        })
}

/// Reads both baseline references out of a lock's root originals.
fn baseline_of(bytes: &[u8]) -> Result<Baseline, String> {
    let lock: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    Ok(Baseline {
        nixpkgs: reference(root_original(&lock, "nixpkgs")?)?,
        microvm: reference(root_original(&lock, "microvm")?)?,
    })
}

/// The `original` object of the node a root input names.
fn root_original<'lock>(lock: &'lock Value, input: &str) -> Result<&'lock Value, String> {
    let root = lock
        .get("root")
        .and_then(Value::as_str)
        .ok_or("the lock names no root node")?;
    let node = lock
        .pointer(&format!("/nodes/{root}/inputs/{input}"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the root takes no input `{input}` from a node of its own"))?;
    lock.pointer(&format!("/nodes/{node}/original"))
        .ok_or_else(|| format!("the node `{node}` carries no `original`"))
}

/// The flake reference an `original` names, for the one type the shipped pin uses.
///
/// Every field is accounted for: a field this function does not render, such as `dir` or `rev`,
/// would silently drop out of the reference, so it is refused with its name instead.
fn reference(original: &Value) -> Result<String, String> {
    let fields: &Map<String, Value> = original.as_object().ok_or("the original is no object")?;
    let text = |name: &str| fields.get(name).and_then(Value::as_str);
    match text("type") {
        Some("github") => {}
        Some(other) => return Err(format!("the original type `{other}` is not `github`")),
        None => return Err("the original names no type".to_owned()),
    }
    if let Some(unknown) = fields
        .keys()
        .find(|key| !matches!(key.as_str(), "type" | "owner" | "repo" | "ref"))
    {
        return Err(format!(
            "the original carries `{unknown}`, which is not rendered"
        ));
    }
    let owner = text("owner").ok_or("the original names no owner")?;
    let repo = text("repo").ok_or("the original names no repo")?;
    Ok(text("ref").map_or_else(
        || format!("github:{owner}/{repo}"),
        |branch| format!("github:{owner}/{repo}/{branch}"),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{LOCK, baseline, reference};

    /// The renderer reads the shipped pin, and the shipped pin carries the `follows` the
    /// generated flake declares, so the pre-flight can keep every baseline node it holds.
    #[test]
    fn the_rendered_references_come_from_the_shipped_pin() -> Result<(), Box<dyn std::error::Error>>
    {
        let lock: Value = serde_json::from_slice(LOCK)?;
        let nodes = &lock["nodes"];
        let shipped = baseline()?;
        assert_eq!(shipped.nixpkgs, reference(&nodes["nixpkgs"]["original"])?);
        assert_eq!(shipped.microvm, reference(&nodes["microvm"]["original"])?);
        assert_eq!(
            nodes["microvm"]["inputs"]["nixpkgs"],
            json!(["nixpkgs"]),
            "the shipped `microvm` must take `nixpkgs` from the root"
        );
        Ok(())
    }

    #[test]
    fn a_github_original_becomes_a_flake_reference() {
        assert_eq!(
            reference(
                &json!({"type": "github", "owner": "NixOS", "repo": "nixpkgs",
                "ref": "nixos-unstable"})
            ),
            Ok("github:NixOS/nixpkgs/nixos-unstable".to_owned())
        );
        assert_eq!(
            reference(&json!({"type": "github", "owner": "astro", "repo": "microvm.nix"})),
            Ok("github:astro/microvm.nix".to_owned())
        );
        let refused = reference(&json!({"type": "git", "url": "https://example.org/x"}));
        assert!(
            matches!(&refused, Err(reason) if reason.contains("`git`")),
            "{refused:?}"
        );
        let refused = reference(&json!({"type": "github", "owner": "o", "repo": "r", "dir": "d"}));
        assert!(
            matches!(&refused, Err(reason) if reason.contains("`dir`")),
            "{refused:?}"
        );
    }
}
