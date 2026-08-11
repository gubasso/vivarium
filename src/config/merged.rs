//! What one evaluation report means: the effective configuration, and the defects in it.
//!
//! Held apart from `evaluate.rs`, which only runs Nix and decodes what it printed. This is the
//! judgment half, and both config readers share it — `viv config eval` refuses when a defect is
//! present and `viv config sources` renders the same defect at exit `0` (ADR-0042). Two classifiers
//! would let one command call something a conflict that the other did not.

use serde_json::{Map, Value};

use super::evaluate::{Definition, KeyClass, KeyReport, LayerKind, Report};

/// The absolute prefixes that make a path personal rather than portable.
///
/// N11's decidable half, and no more than that: a shared artifact must not carry a literal personal
/// path, while `${HOME}` and the XDG names stay unexpanded through evaluation and are explicitly
/// not personal. Guest paths are not scanned at all — a guest home is legitimately `/home/vivarium`
/// — so only host-facing values are checked against this.
const PERSONAL_PREFIXES: [&str; 3] = ["/home/", "/Users/", "/root/"];

/// One layer's appearance in a key's provenance.
#[derive(Clone, Debug)]
pub struct Contributor {
    /// The artifact name.
    pub layer: String,
    /// Its role in the merge.
    pub kind: LayerKind,
    /// The tier word spec/01 prints: `mkDefault`, `normal`, or `mkForce`.
    pub priority: &'static str,
    /// What this layer set.
    pub value: Value,
    /// Whether this definition decided the effective value.
    pub winner: bool,
}

/// One tracked key, with everything both readers need to say about it.
#[derive(Clone, Debug)]
pub struct KeyView {
    /// The manifest's own name for the key, such as `sandbox.egress.mode`.
    pub key: String,
    /// Whether the key resolves by priority or concatenates across layers.
    pub class: KeyClass,
    /// The merged value, or `None` when the merge produced none.
    pub effective: Option<Value>,
    /// The single layer that decided it, when one did.
    pub winner: Option<String>,
    /// Every layer that defined it, in merge order.
    pub contributors: Vec<Contributor>,
}

/// The two defect classes `conflicts` carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    /// One key with two surviving definitions at the same priority.
    Tie,
    /// A literal personal path in a shared layer (N11).
    LiteralPath,
}

impl ConflictKind {
    /// The word the `kind` field carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tie => "tie",
            Self::LiteralPath => "literal-path",
        }
    }
}

/// One content defect, named by class, key, and the layers that caused it.
#[derive(Clone, Debug)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub key: String,
    pub layers: Vec<String>,
    /// The offending values, for the `why:` slot. Not part of the `conflicts` JSON entry, whose
    /// three fields spec/01 fixes; a reader who wants the values reads them from `values`.
    pub evidence: Vec<String>,
}

/// An irreconcilable merge that is not a conflict class: `65`, but nothing `conflicts` reports.
///
/// spec/01 fixes `conflicts` as ties and literal paths only, so a duplicate volume name — spec/14's
/// own cited instance of an irreconcilable merge — is carried separately rather than smuggled into
/// an array whose entries a script branches on.
#[derive(Clone, Debug)]
pub struct Irreconcilable {
    pub what: String,
    pub why: String,
}

/// Everything the report means, computed once.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// Every tracked key, in a stable order.
    pub values: Vec<KeyView>,
    /// The content defects, normally empty.
    pub conflicts: Vec<Conflict>,
    /// The irreconcilable merges, normally empty.
    pub irreconcilable: Vec<Irreconcilable>,
}

impl Analysis {
    /// Whether `viv config eval` must refuse.
    #[must_use]
    pub const fn is_defective(&self) -> bool {
        !self.conflicts.is_empty() || !self.irreconcilable.is_empty()
    }

    /// The merged configuration in the manifest's own shape, for the `config` envelope key.
    ///
    /// Only meaningful when the analysis carries no defect; a defective key has no effective value
    /// and this renders it as `null` rather than inventing one.
    #[must_use]
    pub fn configuration(&self) -> Value {
        let mut root = Map::new();
        for view in &self.values {
            insert_at(
                &mut root,
                &view.key,
                view.effective.clone().unwrap_or(Value::Null),
            );
        }
        Value::Object(root)
    }

    /// The view of one key, when it is tracked.
    #[must_use]
    pub fn value_of(&self, key: &str) -> Option<&KeyView> {
        self.values.iter().find(|view| view.key == key)
    }

    /// The effective value of one key, when it has one.
    #[must_use]
    pub fn effective(&self, key: &str) -> Option<&Value> {
        self.value_of(key).and_then(|view| view.effective.as_ref())
    }
}

/// Reads a report into effective values, provenance, and defects.
#[must_use]
pub fn analyze(report: &Report) -> Analysis {
    let mut values = Vec::new();
    let mut conflicts = Vec::new();

    for (key, entry) in &report.keys {
        let contributors = contributors_of(report, key);
        let tie = tie_layers(entry, &contributors);
        if !tie.is_empty() {
            conflicts.push(Conflict {
                kind: ConflictKind::Tie,
                key: key.clone(),
                layers: tie.clone(),
                evidence: Vec::new(),
            });
        }
        values.push(view_of(key, entry, contributors, !tie.is_empty()));
    }

    conflicts.extend(literal_path_conflicts(report));
    Analysis {
        values,
        conflicts,
        irreconcilable: irreconcilable_merges(report),
    }
}

/// Every layer that defined one key, in merge order.
fn contributors_of(report: &Report, key: &str) -> Vec<(String, LayerKind, Definition)> {
    report
        .layers
        .iter()
        .filter_map(|layer| {
            layer
                .defines
                .get(key)
                .map(|definition| (layer.name.clone(), layer.kind, definition.clone()))
        })
        .collect()
}

/// The layers locked in an equal-priority tie, or empty when there is none.
///
/// Only a scalar can tie. A list key concatenates, so two layers contributing to it are cooperating
/// rather than colliding, and calling that a defect would make every adopted piece a conflict.
fn tie_layers(entry: &KeyReport, contributors: &[(String, LayerKind, Definition)]) -> Vec<String> {
    if entry.class != KeyClass::Scalar || contributors.len() < 2 {
        return Vec::new();
    }
    let Some(highest) = contributors
        .iter()
        .map(|(_, _, definition)| definition.prio)
        .min()
    else {
        return Vec::new();
    };
    let surviving: Vec<&(String, LayerKind, Definition)> = contributors
        .iter()
        .filter(|(_, _, definition)| definition.prio == highest)
        .collect();
    // Two layers that agree are not a tie: the module system merges identical definitions without
    // complaint, and reporting one would make a piece adopted twice look broken.
    let distinct = surviving
        .iter()
        .map(|(_, _, definition)| &definition.value)
        .collect::<Vec<_>>();
    if surviving.len() < 2 || distinct.windows(2).all(|pair| pair[0] == pair[1]) {
        return Vec::new();
    }
    surviving.iter().map(|(name, _, _)| name.clone()).collect()
}

fn view_of(
    key: &str,
    entry: &KeyReport,
    contributors: Vec<(String, LayerKind, Definition)>,
    tied: bool,
) -> KeyView {
    let defective = tied || !entry.ok;
    // A list has no winner even when it evaluates: every contributor is in the result, so naming
    // one would say that the others were shadowed when nothing was.
    let winner = if defective || entry.class == KeyClass::List {
        None
    } else {
        contributors
            .iter()
            .min_by_key(|(_, _, definition)| definition.prio)
            .map(|(name, _, _)| name.clone())
    };
    KeyView {
        key: key.to_owned(),
        class: entry.class,
        effective: if defective {
            None
        } else {
            Some(entry.value.clone())
        },
        winner: winner.clone(),
        contributors: contributors
            .into_iter()
            .map(|(layer, kind, definition)| Contributor {
                winner: winner.as_ref().is_some_and(|name| *name == layer),
                priority: definition.priority_label(),
                value: definition.value,
                layer,
                kind,
            })
            .collect(),
    }
}

/// Every shared layer that mounts a literal personal host path.
fn literal_path_conflicts(report: &Report) -> Vec<Conflict> {
    let mut layers = Vec::new();
    let mut evidence = Vec::new();
    for layer in &report.layers {
        if !layer.kind.is_shared() {
            continue;
        }
        let Some(definition) = layer.defines.get("mounts") else {
            continue;
        };
        // The `source` field only. A mount's `target` is a guest path, where `/home/<name>` is the
        // ordinary shape of a home directory rather than one person's machine.
        let personal: Vec<String> = definition
            .value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|mount| mount.get("source").and_then(Value::as_str))
            .filter(|source| is_personal_path(source))
            .map(str::to_owned)
            .collect();
        if !personal.is_empty() {
            layers.push(layer.name.clone());
            evidence.extend(personal);
        }
    }
    if layers.is_empty() {
        return Vec::new();
    }
    vec![Conflict {
        kind: ConflictKind::LiteralPath,
        key: "mounts".to_owned(),
        layers,
        evidence,
    }]
}

fn is_personal_path(value: &str) -> bool {
    PERSONAL_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

/// Merges no priority can settle, which are defects of the result rather than of one key.
fn irreconcilable_merges(report: &Report) -> Vec<Irreconcilable> {
    let Some(entry) = report.keys.get("volumes") else {
        return Vec::new();
    };
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut found = Vec::new();
    for volume in entry.value.as_array().into_iter().flatten() {
        let (Some(name), Some(mount)) = (
            volume.get("name").and_then(Value::as_str),
            volume.get("mount").and_then(Value::as_str),
        ) else {
            continue;
        };
        if let Some((_, other)) = seen.iter().find(|(seen, _)| *seen == name)
            && *other != mount
        {
            found.push(Irreconcilable {
                what: format!("volume `{name}` is bound to two mountpoints"),
                why: format!("`{other}` and `{mount}` cannot both be where it is mounted"),
            });
        }
        seen.push((name, mount));
    }
    found
}

/// Places one dotted key into the nested envelope, creating the objects it passes through.
fn insert_at(root: &mut Map<String, Value>, key: &str, value: Value) {
    let mut segments = key.split('.').peekable();
    let mut cursor = root;
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            cursor.insert(segment.to_owned(), value);
            return;
        }
        cursor = cursor
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap_or_else(|| unreachable!("a tracked key never nests inside a leaf"));
    }
}

#[cfg(test)]
mod tests {
    use super::{ConflictKind, analyze};
    use crate::config::evaluate::Report;

    /// Assembles a fixture from its parts.
    ///
    /// Composed rather than written out, because a report is verbose and every case here turns on
    /// two or three fields of it. Spelling the whole envelope each time would bury the one
    /// difference that makes the case.
    fn report(keys: &[String], layers: &[String]) -> Report {
        let source = format!(
            r#"{{ "keys": {{ {} }}, "layers": [ {} ] }}"#,
            keys.join(", "),
            layers.join(", ")
        );
        serde_json::from_str(&source).unwrap_or_else(|error| unreachable!("fixture: {error}"))
    }

    fn key(name: &str, class: &str, value: &str) -> String {
        let ok = value != "null";
        format!(r#""{name}": {{ "class": "{class}", "ok": {ok}, "value": {value} }}"#)
    }

    fn layer(name: &str, kind: &str, defines: &[String]) -> String {
        let identity = format!(r#""name": "{name}", "kind": "{kind}", "readable": true"#);
        let defines = defines.join(", ");
        format!(r#"{{ {identity}, "defines": {{ {defines} }} }}"#)
    }

    fn defines(key: &str, prio: i64, value: &str) -> String {
        format!(r#""{key}": {{ "prio": {prio}, "value": {value} }}"#)
    }

    #[test]
    fn a_shadowed_proposal_keeps_its_place_and_names_one_winner() {
        let analysis = analyze(&report(
            &[key("sandbox.egress.mode", "scalar", r#""allowlist""#)],
            &[
                layer(
                    "base",
                    "image",
                    &[defines("sandbox.egress.mode", 1000, r#""open""#)],
                ),
                layer(
                    "net",
                    "piece",
                    &[defines("sandbox.egress.mode", 50, r#""allowlist""#)],
                ),
            ],
        ));
        assert!(!analysis.is_defective());
        let view = analysis
            .value_of("sandbox.egress.mode")
            .unwrap_or_else(|| unreachable!("tracked key"));
        assert_eq!(view.winner.as_deref(), Some("net"));
        assert_eq!(view.contributors.len(), 2);
        assert_eq!(view.contributors[0].priority, "mkDefault");
        assert!(!view.contributors[0].winner);
        assert!(view.contributors[1].winner);
    }

    #[test]
    fn equal_priority_survivors_tie_but_agreeing_ones_do_not() {
        let pieces = |first: &str, second: &str| {
            [
                layer("a", "piece", &[defines("resources.mem_mib", 100, first)]),
                layer("b", "piece", &[defines("resources.mem_mib", 100, second)]),
            ]
        };
        let tied = analyze(&report(
            &[key("resources.mem_mib", "scalar", "null")],
            &pieces("4096", "8192"),
        ));
        assert!(tied.is_defective());
        assert_eq!(tied.conflicts[0].kind, ConflictKind::Tie);
        assert_eq!(tied.conflicts[0].layers, vec!["a", "b"]);
        let view = tied
            .value_of("resources.mem_mib")
            .unwrap_or_else(|| unreachable!("tracked key"));
        // A defective key keeps its contributors and reports no winner (spec/01, ADR-0042).
        assert_eq!(view.contributors.len(), 2);
        assert!(view.winner.is_none());
        assert!(view.effective.is_none());

        let agreeing = analyze(&report(
            &[key("resources.mem_mib", "scalar", "4096")],
            &pieces("4096", "4096"),
        ));
        assert!(!agreeing.is_defective());
    }

    #[test]
    fn a_list_concatenates_without_a_winner_or_a_tie() {
        let analysis = analyze(&report(
            &[key(
                "sandbox.egress.allow",
                "list",
                r#"["a.example", "b.example"]"#,
            )],
            &[
                layer(
                    "one",
                    "piece",
                    &[defines("sandbox.egress.allow", 100, r#"["a.example"]"#)],
                ),
                layer(
                    "two",
                    "manifest",
                    &[defines("sandbox.egress.allow", 100, r#"["b.example"]"#)],
                ),
            ],
        ));
        assert!(!analysis.is_defective());
        let view = analysis
            .value_of("sandbox.egress.allow")
            .unwrap_or_else(|| unreachable!("tracked key"));
        assert!(view.winner.is_none());
        assert_eq!(view.contributors.len(), 2);
    }

    #[test]
    fn a_literal_path_is_a_defect_only_in_a_shared_layer() {
        let mounted = |source: &str| {
            format!(
                r#"[ {{ "source": "{source}", "target": "~/.config/team", "readonly": true }} ]"#
            )
        };
        let personal = mounted("/home/ana/.config/team");
        let fixture = |kind: &str| {
            report(
                &[key("mounts", "list", "[]")],
                &[layer(
                    "team",
                    kind,
                    &[defines("mounts", 100, personal.as_str())],
                )],
            )
        };
        let shared = analyze(&fixture("piece"));
        assert_eq!(shared.conflicts[0].kind, ConflictKind::LiteralPath);
        assert_eq!(shared.conflicts[0].layers, vec!["team"]);
        assert_eq!(shared.conflicts[0].evidence, vec!["/home/ana/.config/team"]);
        // The manifest is the personal layer, so the same path there is ordinary authorship.
        assert!(!analyze(&fixture("manifest")).is_defective());

        // An unexpanded portable variable is explicitly not a personal path (N11).
        let portable = mounted("${HOME}/.config/team");
        assert!(
            !analyze(&report(
                &[key("mounts", "list", "[]")],
                &[layer(
                    "team",
                    "piece",
                    &[defines("mounts", 100, portable.as_str())]
                )],
            ))
            .is_defective()
        );
    }

    #[test]
    fn one_volume_name_on_two_mountpoints_is_irreconcilable() {
        let analysis = analyze(&report(
            &[key(
                "volumes",
                "list",
                r#"[ { "name": "cache", "mount": "/one", "size_gib": null },
                { "name": "cache", "mount": "/two", "size_gib": null } ]"#,
            )],
            &[],
        ));
        assert!(analysis.is_defective());
        // Not a `conflicts` entry: spec/01 fixes that array as ties and literal paths only.
        assert!(analysis.conflicts.is_empty());
        assert!(analysis.irreconcilable[0].what.contains("cache"));
    }

    #[test]
    fn the_envelope_nests_dotted_keys() {
        let analysis = analyze(&report(
            &[
                key("sandbox.egress.mode", "scalar", r#""open""#),
                key("env.EDITOR", "scalar", r#""hx""#),
            ],
            &[],
        ));
        let configuration = analysis.configuration();
        assert_eq!(configuration["sandbox"]["egress"]["mode"], "open");
        assert_eq!(configuration["env"]["EDITOR"], "hx");
    }
}
