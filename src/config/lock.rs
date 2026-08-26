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

/// One composed-lock defect nothing downstream would name.
///
/// Deliberately not a general validity check: an unparsable or incomplete lock is Nix's to
/// report, and a missing baseline node already fails as the missing-node refusal spec/04
/// fixes. What nothing else catches is the split — `nixpkgs` and `microvm`'s own `nixpkgs`
/// resolving to two different trees is legal Nix and a guest that fails at boot rather than
/// at evaluation, which is the hazard the render-time `follows` edge existed to prevent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ComposedLockDefect {
    /// The root resolves no node for a baseline the generated flake always declares.
    BaselineMissing { name: &'static str },
    /// `microvm`'s own `nixpkgs` resolves to a different node than the root's.
    BaselineSplit { root: String, microvm: String },
}

/// Checks the one composed rule over a lock's bytes; unparsable bytes pass (Nix's domain).
pub(super) fn validate_composed(bytes: &[u8]) -> Result<(), ComposedLockDefect> {
    let Ok(lock) = serde_json::from_slice::<Value>(bytes) else {
        return Ok(());
    };
    let root = lock["root"].as_str().unwrap_or("root").to_owned();
    let Some(nodes) = lock.get("nodes").and_then(Value::as_object) else {
        return Ok(());
    };
    let resolve = |name: &str| -> Option<String> {
        let edge = nodes.get(&root)?.get("inputs")?.get(name)?;
        match edge {
            Value::String(label) => Some(label.clone()),
            Value::Array(path) => resolve_follows(nodes, &root, path, 0),
            _ => None,
        }
    };
    let Some(nixpkgs) = resolve("nixpkgs") else {
        return Err(ComposedLockDefect::BaselineMissing { name: "nixpkgs" });
    };
    let Some(microvm) = resolve("microvm") else {
        return Err(ComposedLockDefect::BaselineMissing { name: "microvm" });
    };
    // The split check. `microvm` carrying no `nixpkgs` edge of its own is fine — there is
    // nothing to diverge — so only a resolvable edge is compared.
    if let Some(edge) = nodes
        .get(&microvm)
        .and_then(|node| node.get("inputs"))
        .and_then(|inputs| inputs.get("nixpkgs"))
    {
        let followed = match edge {
            Value::String(label) => Some(label.clone()),
            Value::Array(path) => resolve_follows(nodes, &root, path, 0),
            _ => None,
        };
        if let Some(label) = followed
            && label != nixpkgs
        {
            return Err(ComposedLockDefect::BaselineSplit {
                root: nixpkgs,
                microvm: label,
            });
        }
    }
    Ok(())
}

/// One public root input's pinned state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootPin {
    /// The reported pin: `rev`, else `narHash`, else `lastModified` — a `git` node has the
    /// first, a fetched `path:` node the middle two, and a relative base node none of them,
    /// which reports as an absent pin because its content is a layer rather than a pinned
    /// reference (ADR-0112).
    pub display: Option<String>,
    /// The input's whole resolved subtree, serialized for comparison and nothing else.
    ///
    /// Movement is decided against this rather than against `display`, because the pin a
    /// user actually moves can sit below a hashless base node: `viv update my-base` moving
    /// the base's declared `dep` changes no public node's own `locked`, and a row that read
    /// only its own node would report "nothing moved" over a lock that moved.
    pub fingerprint: String,
}

/// The public root inputs and each one's pinned state, or `None` for unparsable bytes.
pub fn pins(bytes: &[u8]) -> Option<std::collections::BTreeMap<String, RootPin>> {
    let lock = serde_json::from_slice::<Value>(bytes).ok()?;
    let root = lock["root"].as_str().unwrap_or("root").to_owned();
    let nodes = lock.get("nodes")?.as_object()?;
    let inputs = nodes.get(&root)?.get("inputs")?.as_object()?;
    let mut pinned = std::collections::BTreeMap::new();
    for (name, edge) in inputs {
        let label = match edge {
            Value::String(label) => Some(label.clone()),
            Value::Array(path) => resolve_follows(nodes, &root, path, 0),
            _ => None,
        };
        let display = label
            .as_ref()
            .and_then(|label| nodes.get(label))
            .and_then(|node| node.get("locked"))
            .and_then(pin_display);
        let fingerprint = label.map_or_else(String::new, |label| subtree(nodes, &root, &label));
        pinned.insert(
            name.clone(),
            RootPin {
                display,
                fingerprint,
            },
        );
    }
    Some(pinned)
}

/// Serializes every node reachable from `label`, sorted, for the fingerprint above.
fn subtree(nodes: &serde_json::Map<String, Value>, root: &str, label: &str) -> String {
    let mut keep = std::collections::BTreeSet::new();
    let mut queue = vec![label.to_owned()];
    while let Some(current) = queue.pop() {
        if !keep.insert(current.clone()) {
            continue;
        }
        let Some(inputs) = nodes
            .get(&current)
            .and_then(|node| node.get("inputs"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for edge in inputs.values() {
            match edge {
                Value::String(next) => queue.push(next.clone()),
                Value::Array(path) => {
                    if let Some(next) = resolve_follows(nodes, root, path, 0) {
                        queue.push(next);
                    }
                }
                _ => {}
            }
        }
    }
    let mut rendered = String::new();
    for name in keep {
        let locked = nodes
            .get(&name)
            .and_then(|node| node.get("locked"))
            .map_or_else(String::new, Value::to_string);
        rendered.push_str(&name);
        rendered.push('=');
        rendered.push_str(&locked);
        rendered.push(';');
    }
    rendered
}

fn pin_display(locked: &Value) -> Option<String> {
    if let Some(rev) = locked.get("rev").and_then(Value::as_str) {
        return Some(rev.to_owned());
    }
    if let Some(hash) = locked.get("narHash").and_then(Value::as_str) {
        return Some(hash.to_owned());
    }
    locked
        .get("lastModified")
        .and_then(Value::as_i64)
        .map(|stamp| stamp.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{ComposedLockDefect, carries_vivarium, pins, shed_vivarium, validate_composed};

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

    /// The one composed rule: a split between the root's `nixpkgs` and `microvm`'s own is
    /// the boot-failure shape nothing downstream names; the follows topology a base flake
    /// produces resolves clean; and unparsable bytes stay Nix's to report.
    #[test]
    fn the_composed_check_catches_exactly_the_split() {
        let split = serde_json::to_vec_pretty(&json!({
            "nodes": {
                "nixpkgs": { "locked": { "narHash": "a" } },
                "nixpkgs_2": { "locked": { "narHash": "b" } },
                "microvm": { "inputs": { "nixpkgs": "nixpkgs" }, "locked": { "narHash": "m" } },
                "root": { "inputs": { "nixpkgs": "nixpkgs_2", "microvm": "microvm" } }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap();
        assert_eq!(
            validate_composed(&split),
            Err(ComposedLockDefect::BaselineSplit {
                root: "nixpkgs_2".to_owned(),
                microvm: "nixpkgs".to_owned(),
            })
        );

        // The follows topology a base flake renders: both baselines are aliases into the
        // base's subtree, and `microvm`'s own edge follows the same node the root reaches.
        let followed = serde_json::to_vec_pretty(&json!({
            "nodes": {
                "my-base": {
                    "inputs": { "nixpkgs": "nixpkgs", "microvm": "microvm" },
                    "locked": { "path": "./images/my-base", "type": "path" }
                },
                "nixpkgs": { "locked": { "narHash": "a" } },
                "microvm": {
                    "inputs": { "nixpkgs": ["my-base", "nixpkgs"] },
                    "locked": { "narHash": "m" }
                },
                "root": { "inputs": {
                    "my-base": "my-base",
                    "nixpkgs": ["my-base", "nixpkgs"],
                    "microvm": ["my-base", "microvm"]
                } }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap();
        assert_eq!(validate_composed(&followed), Ok(()));

        let missing = serde_json::to_vec_pretty(&json!({
            "nodes": {
                "microvm": { "locked": { "narHash": "m" } },
                "root": { "inputs": { "microvm": "microvm" } }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap();
        assert_eq!(
            validate_composed(&missing),
            Err(ComposedLockDefect::BaselineMissing { name: "nixpkgs" })
        );

        assert_eq!(validate_composed(b"not a lock"), Ok(()));
    }

    /// The pin display walks public root inputs only, resolves follows arrays, and projects
    /// `rev`, else `narHash`, else `lastModified` — a relative base node has none and
    /// reports as an absent pin, because its content is a layer (ADR-0112).
    #[test]
    fn pins_project_public_roots_by_fidelity() {
        let lock = serde_json::to_vec_pretty(&json!({
            "nodes": {
                "my-base": {
                    "inputs": { "nixpkgs": "nixpkgs" },
                    "locked": { "path": "./images/my-base", "type": "path" }
                },
                "nixpkgs": { "locked": { "rev": "abc123", "narHash": "h" } },
                "fetched": { "locked": { "narHash": "sha256-xyz", "lastModified": 5 } },
                "hidden": { "locked": { "rev": "transitive" } },
                "root": { "inputs": {
                    "my-base": "my-base",
                    "nixpkgs": ["my-base", "nixpkgs"],
                    "fetched": "fetched"
                } }
            },
            "root": "root",
            "version": 7
        }))
        .unwrap();
        let pinned = pins(&lock).unwrap();
        assert_eq!(
            pinned.get("nixpkgs").map(|pin| pin.display.clone()),
            Some(Some("abc123".to_owned()))
        );
        assert_eq!(
            pinned.get("fetched").map(|pin| pin.display.clone()),
            Some(Some("sha256-xyz".to_owned()))
        );
        assert_eq!(
            pinned.get("my-base").map(|pin| pin.display.clone()),
            Some(None)
        );
        // A transitive node is not a public root input and gets no row.
        assert!(!pinned.contains_key("hidden"));
        assert!(pins(b"junk").is_none());

        // Movement below a hashless node is still movement: the base's own `locked` is
        // unchanged when its declared input moves, and the fingerprint is what sees it.
        let moved = String::from_utf8(lock).unwrap().replace("abc123", "def456");
        let after = pins(moved.as_bytes()).unwrap();
        assert_ne!(
            pinned.get("my-base").map(|pin| pin.fingerprint.clone()),
            after.get("my-base").map(|pin| pin.fingerprint.clone())
        );
        assert_eq!(
            pinned.get("fetched").map(|pin| pin.fingerprint.clone()),
            after.get("fetched").map(|pin| pin.fingerprint.clone())
        );
    }
}
