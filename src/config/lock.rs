//! The one place vivarium reads a lockfile's contents.
//!
//! ADR-0102's migration: a lock retained from the shape slice 012 built carries a dead
//! `vivarium` node, and the next preparation sheds it — never silently, and never moving a
//! surviving pin, which is ADR-0059's line. Everything else the tool does with a lock treats it
//! as opaque bytes, and this module exists so that stays true.

use serde_json::Value;

/// Whether a lock's `nodes` carry a `vivarium` entry or the root declares one as an input.
#[must_use]
pub(super) fn carries_vivarium(bytes: &[u8]) -> bool {
    let Ok(lock) = serde_json::from_slice::<Value>(bytes) else {
        // Not this module's defect to report: an unparsable lock has no `vivarium` node to
        // shed, and Nix names what is actually wrong with it at evaluation.
        return false;
    };
    let root = lock["root"].as_str().unwrap_or("root");
    lock["nodes"].get("vivarium").is_some()
        || lock["nodes"][root]["inputs"].get("vivarium").is_some()
}

/// Sheds the dead `vivarium` node: the root edge, the node, and everything only it reached.
///
/// Returns the migrated bytes, or `None` when there is nothing to shed. The surviving nodes'
/// objects are carried over untouched, so no pin moves — nodes such as `flake-utils` disappear
/// exactly when the removed subtree was their only path from the root, which is Nix's own
/// reachability rule for what a lock retains.
#[must_use]
pub(super) fn shed_vivarium(bytes: &[u8]) -> Option<Vec<u8>> {
    if !carries_vivarium(bytes) {
        return None;
    }
    let mut lock = serde_json::from_slice::<Value>(bytes).ok()?;
    let root = lock["root"].as_str().unwrap_or("root").to_owned();
    let nodes = lock.get_mut("nodes")?.as_object_mut()?;
    if let Some(inputs) = nodes
        .get_mut(&root)
        .and_then(|node| node.get_mut("inputs"))
        .and_then(Value::as_object_mut)
    {
        inputs.remove("vivarium");
    }
    nodes.remove("vivarium");

    // Reachability from the root, following both edge shapes a lock uses: a string names a node
    // directly, and an array is a `follows` path resolved from the root.
    let mut keep = std::collections::BTreeSet::new();
    let mut queue = vec![root.clone()];
    while let Some(name) = queue.pop() {
        if !keep.insert(name.clone()) {
            continue;
        }
        let Some(inputs) = nodes
            .get(&name)
            .and_then(|node| node.get("inputs"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for edge in inputs.values() {
            match edge {
                Value::String(next) => queue.push(next.clone()),
                Value::Array(path) => {
                    if let Some(next) = resolve_follows(nodes, &root, path, 0) {
                        queue.push(next);
                    }
                }
                _ => {}
            }
        }
    }
    nodes.retain(|name, _| keep.contains(name));

    let mut rendered = serde_json::to_vec_pretty(&lock).ok()?;
    rendered.push(b'\n');
    Some(rendered)
}

/// Resolves a `follows` path from the root, one label at a time.
///
/// Depth-bounded because a hand-edited lock can make follows edges cycle, and this walk must
/// terminate on any input rather than trust the file it is migrating.
fn resolve_follows(
    nodes: &serde_json::Map<String, Value>,
    root: &str,
    path: &[Value],
    depth: usize,
) -> Option<String> {
    if depth > 32 {
        return None;
    }
    let mut current = root.to_owned();
    for label in path {
        let label = label.as_str()?;
        let edge = nodes.get(&current)?.get("inputs")?.get(label)?;
        current = match edge {
            Value::String(next) => next.clone(),
            Value::Array(next_path) => resolve_follows(nodes, root, next_path, depth + 1)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{carries_vivarium, shed_vivarium};
    use serde_json::{Value, json};

    /// The retained shape slice 012 left behind: `vivarium` pinned beside the two baselines,
    /// pulling `flake-utils` (and through it `systems`) that nothing else reaches, while
    /// `microvm` reaches `spectrum` on its own.
    fn retained_lock() -> Vec<u8> {
        serde_json::to_vec_pretty(&json!({
            "nodes": {
                "flake-utils": {
                    "inputs": { "systems": "systems" },
                    "locked": { "narHash": "sha256-futils", "type": "github" }
                },
                "microvm": {
                    "inputs": { "nixpkgs": ["nixpkgs"], "spectrum": "spectrum" },
                    "locked": { "narHash": "sha256-microvm", "type": "github" }
                },
                "nixpkgs": {
                    "locked": { "narHash": "sha256-nixpkgs", "type": "github" }
                },
                "root": {
                    "inputs": { "microvm": "microvm", "nixpkgs": "nixpkgs", "vivarium": "vivarium" }
                },
                "spectrum": {
                    "locked": { "narHash": "sha256-spectrum", "type": "git" }
                },
                "systems": {
                    "locked": { "narHash": "sha256-systems", "type": "github" }
                },
                "vivarium": {
                    "inputs": {
                        "flake-utils": "flake-utils",
                        "microvm": ["microvm"],
                        "nixpkgs": ["nixpkgs"]
                    },
                    "locked": { "narHash": "sha256-vivarium", "type": "path" }
                }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap()
    }

    #[test]
    fn sheds_the_node_its_edge_and_its_private_subtree_only() {
        let migrated = shed_vivarium(&retained_lock()).unwrap();
        let lock: Value = serde_json::from_slice(&migrated).unwrap();
        let names: Vec<&str> = lock["nodes"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // `flake-utils` and `systems` were reachable only through `vivarium`; `spectrum` stays
        // because `microvm` reaches it on its own.
        assert_eq!(names, ["microvm", "nixpkgs", "root", "spectrum"]);
        assert_eq!(
            lock["nodes"]["root"]["inputs"],
            json!({ "microvm": "microvm", "nixpkgs": "nixpkgs" })
        );
        assert!(!carries_vivarium(&migrated));
    }

    /// ADR-0059: a build may shed the dead node, and no surviving pin moves.
    #[test]
    fn the_surviving_pins_do_not_move() {
        let original: Value = serde_json::from_slice(&retained_lock()).unwrap();
        let migrated: Value =
            serde_json::from_slice(&shed_vivarium(&retained_lock()).unwrap()).unwrap();
        for name in ["nixpkgs", "microvm", "spectrum"] {
            assert_eq!(
                original["nodes"][name]["locked"], migrated["nodes"][name]["locked"],
                "the `{name}` pin moved during migration"
            );
        }
    }

    #[test]
    fn a_clean_lock_and_an_unparsable_file_shed_nothing() {
        let clean = serde_json::to_vec_pretty(&json!({
            "nodes": {
                "nixpkgs": { "locked": { "narHash": "x" } },
                "root": { "inputs": { "nixpkgs": "nixpkgs" } }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap();
        assert!(!carries_vivarium(&clean));
        assert!(shed_vivarium(&clean).is_none());
        // Corruption is Nix's to name at evaluation, not this migration's.
        assert!(!carries_vivarium(b"not a lock"));
        assert!(shed_vivarium(b"not a lock").is_none());
    }
}
