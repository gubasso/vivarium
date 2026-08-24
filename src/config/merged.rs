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

/// The defect classes `conflicts` carries (spec/01 fixes the `kind` words).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    /// One key with two surviving definitions at the same priority.
    Tie,
    /// A literal personal path in a shared layer (N11).
    LiteralPath,
    /// A mount source that is textually a host session directory or an ancestor of one (N24's
    /// decidable half; the launch-time check owns the spellings only expansion reveals).
    SessionPath,
    /// A shared layer's mount source naming a variable outside the portable set (spec/07).
    NonPortableVariable,
}

impl ConflictKind {
    /// The word the `kind` field carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tie => "tie",
            Self::LiteralPath => "literal-path",
            Self::SessionPath => "session-path",
            Self::NonPortableVariable => "non-portable-variable",
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

    /// Adds the manifest's own `[[workspaces]]` rows to the rendered view.
    ///
    /// Injected rather than read from the report, because since `ADR-0110` there is no
    /// `vivarium.workspaces` option for the report to carry: a declared workspace compiles to a
    /// `mounts` row whose target is its source, and Nix never learns it was a workspace.
    ///
    /// The reader still needs the distinction, which is why this exists rather than letting the
    /// rows show up only as mounts. `[[workspaces]]` says a directory belongs to this sandbox and
    /// at most one manifest may claim it; `[[mounts]]` says a directory is visible in it and any
    /// number may. That difference decides which manifest a working directory resolves to, so a
    /// provenance view that hid it would answer a question the user did not ask.
    ///
    /// The declared spelling, not the expansion. It is what the user edits, and the expansion is
    /// visible in the `mounts` block beside it — which makes the two blocks explain each other
    /// rather than repeat each other.
    pub fn with_manifest_workspaces(&mut self, manifest: &super::Manifest, manifest_name: &str) {
        if manifest.workspaces.is_empty() {
            return;
        }
        let rows = Value::Array(
            manifest
                .workspaces
                .iter()
                .map(|workspace| {
                    let mut row = Map::new();
                    row.insert("source".to_owned(), Value::String(workspace.source.clone()));
                    Value::Object(row)
                })
                .collect(),
        );
        self.values.push(KeyView {
            key: "workspaces".to_owned(),
            // A list, so `config sources` prints the contribution as cooperation rather than
            // naming a winner. Truer here than anywhere: there is exactly one contributor and no
            // rival is possible, because only a manifest may declare a workspace at all.
            class: KeyClass::List,
            effective: Some(rows.clone()),
            winner: None,
            contributors: vec![Contributor {
                layer: manifest_name.to_owned(),
                kind: LayerKind::Manifest,
                priority: "normal",
                value: rows,
                winner: false,
            }],
        });
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
    conflicts.extend(session_path_conflicts(report));
    conflicts.extend(non_portable_variable_conflicts(report));
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
///
/// The `source` field only, here and in the two classifiers below. A mount's `target` is a guest
/// path, where `/home/<name>` is the ordinary shape of a home directory rather than one person's
/// machine.
fn literal_path_conflicts(report: &Report) -> Vec<Conflict> {
    source_conflicts(
        report,
        "mounts",
        ConflictKind::LiteralPath,
        true,
        is_personal_path,
    )
}

fn is_personal_path(value: &str) -> bool {
    PERSONAL_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

/// Collect one conflict over every layer whose declaration `source`s the predicate flags.
///
/// The shape `literal_path_conflicts` established, factored because N24 and the portable-variable
/// rule walk the same field with a different test and a different layer filter.
fn source_conflicts(
    report: &Report,
    key: &str,
    kind: ConflictKind,
    shared_only: bool,
    flagged: impl Fn(&str) -> bool,
) -> Vec<Conflict> {
    let mut layers = Vec::new();
    let mut evidence = Vec::new();
    for layer in &report.layers {
        if shared_only && !layer.kind.is_shared() {
            continue;
        }
        let Some(definition) = layer.defines.get(key) else {
            continue;
        };
        let offending: Vec<String> = definition
            .value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|declaration| declaration.get("source").and_then(Value::as_str))
            .filter(|source| flagged(source))
            .map(str::to_owned)
            .collect();
        if !offending.is_empty() {
            layers.push(layer.name.clone());
            evidence.extend(offending);
        }
    }
    if layers.is_empty() {
        return Vec::new();
    }
    vec![Conflict {
        kind,
        key: key.to_owned(),
        layers,
        evidence,
    }]
}

/// N24's decidable half: a mount `source` that is textually a session directory, under one, or a
/// literal ancestor of one — in EVERY layer, unlike N11, because the invariant binds the personal
/// manifest too. A source a variable hides is not decidable here; the launch-time refusal in
/// `src/launch/mounts.rs` owns it, which is the two-tier shape spec/06 names.
fn session_path_conflicts(report: &Report) -> Vec<Conflict> {
    source_conflicts(
        report,
        "mounts",
        ConflictKind::SessionPath,
        false,
        is_session_path,
    )
}

fn is_session_path(value: &str) -> bool {
    // The ancestors, exactly: `/` holds everything, and `/var` and `/run` hold two of the three
    // roots. Component-wise, so `/varnish` is not `/var`.
    if matches!(value, "/" | "/var" | "/run") {
        return true;
    }
    if value.starts_with("${XDG_RUNTIME_DIR}") {
        return true;
    }
    ["/tmp", "/var/tmp", "/run/user"].iter().any(|root| {
        value == *root
            || value
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The variables a shared layer may reference in a mount `source` (spec/07): `${HOME}` and the
/// four durable XDG directories. `${XDG_RUNTIME_DIR}` is deliberately not here — and is refused
/// harder, as a session path above.
const PORTABLE_VARIABLES: [&str; 5] = [
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
];

/// spec/07's sharing rule, decidable from text: a shared layer referencing the host through any
/// variable outside the portable set. The personal manifest is exempt — a literal path is legal
/// there, so a private variable is too.
fn non_portable_variable_conflicts(report: &Report) -> Vec<Conflict> {
    let flagged = |source: &str| {
        variable_references(source)
            .iter()
            .any(|name| !PORTABLE_VARIABLES.contains(&name.as_str()))
    };
    source_conflicts(
        report,
        "mounts",
        ConflictKind::NonPortableVariable,
        true,
        flagged,
    )
}

/// Every `${NAME}` reference in a source, by the same grammar the launch-time expander reads:
/// braced, named, nothing else. A bare `$VAR` is literal text there, so it is literal text here.
fn variable_references(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        let name = &after[..end];
        if !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            names.push(name.to_owned());
        }
        rest = &after[end + 1..];
    }
    names
}

/// Merges no priority can settle, which are defects of the result rather than of one key.
fn irreconcilable_merges(report: &Report) -> Vec<Irreconcilable> {
    let mut found = duplicate_volume_mounts(report);
    // Since ADR-0110 this reaches a declared workspace too: it compiles to a `mounts` row whose
    // target is its source, so a shared layer mounting something at a tree the manifest owns is
    // one duplicate target rather than a rule of its own.
    found.extend(duplicate_mount_targets(report));
    found
}

fn duplicate_volume_mounts(report: &Report) -> Vec<Irreconcilable> {
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

/// ADR-0020: mount declarations concatenate across layers and duplicate `target`s fail
/// evaluation. Textual duplicates only — `~/x` and its expansion are one guest path this view
/// cannot see — so the guest module's post-expansion assertion stays the authority and this is
/// the reader that refuses the ordinary collision with a merged-view diagnostic.
fn duplicate_mount_targets(report: &Report) -> Vec<Irreconcilable> {
    let Some(entry) = report.keys.get("mounts") else {
        return Vec::new();
    };
    let mut seen: Vec<&str> = Vec::new();
    let mut found = Vec::new();
    for mount in entry.value.as_array().into_iter().flatten() {
        let Some(target) = mount.get("target").and_then(Value::as_str) else {
            continue;
        };
        if seen.contains(&target) {
            found.push(Irreconcilable {
                what: format!("two declared mounts share the target `{target}`"),
                why: "declarations concatenate across layers, and one guest path cannot carry \
                    two sources (ADR-0020)"
                    .to_owned(),
            });
        }
        seen.push(target);
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
    fn two_mounts_on_one_target_are_irreconcilable() {
        let fixture = |targets: [&str; 2]| {
            let entry = |source: &str, target: &str| {
                format!(r#"{{ "source": "{source}", "target": "{target}", "readonly": false }}"#)
            };
            let value = format!(
                "[ {}, {} ]",
                entry("${HOME}/a", targets[0]),
                entry("${HOME}/b", targets[1])
            );
            report(
                &[key("mounts", "list", value.as_str())],
                &[layer("m", "manifest", &[])],
            )
        };
        let colliding = analyze(&fixture(["~/.config/x", "~/.config/x"]));
        assert!(colliding.is_defective());
        assert!(colliding.irreconcilable[0].what.contains("~/.config/x"));
        assert!(!analyze(&fixture(["~/.config/x", "~/.config/y"])).is_defective());
    }

    /// The ownership collision, which since ADR-0110 arrives as a duplicate target.
    ///
    /// A declared workspace compiles to a mount whose target is its source, so a shared layer
    /// mounting anything at a tree the manifest owns collides on that target and is refused at
    /// `65`. Three rules became one: the workspace-only overlap check, the manifest-only
    /// declaration check, and this. Pinned because the collapse is only sound if the surviving
    /// rule actually covers the cases the deleted ones did.
    #[test]
    fn a_layer_mounting_over_a_workspace_collides_on_its_target() {
        let entry = |source: &str, target: &str| {
            format!(r#"{{ "source": "{source}", "target": "{target}", "readonly": false }}"#)
        };
        let fixture = |rows: String| {
            report(
                &[key("mounts", "list", rows.as_str())],
                &[layer("m", "manifest", &[])],
            )
        };

        // Two workspaces that overlap: equal targets, because each mirrors its own source.
        let equal = analyze(&fixture(format!(
            "[ {}, {} ]",
            entry("/home/u/code", "/home/u/code"),
            entry("/home/u/code", "/home/u/code")
        )));
        assert!(equal.is_defective());
        assert!(equal.irreconcilable[0].what.contains("/home/u/code"));

        // A shared layer landing a mount on a tree the manifest owns.
        let shadowed = analyze(&fixture(format!(
            "[ {}, {} ]",
            entry("/home/u/code", "/home/u/code"),
            entry("/srv/other", "/home/u/code")
        )));
        assert!(shadowed.is_defective());

        // Disjoint trees stay legal, which is the whole point of the plural workspace.
        assert!(
            !analyze(&fixture(format!(
                "[ {}, {} ]",
                entry("/home/u/api", "/home/u/api"),
                entry("/home/u/web", "/home/u/web")
            )))
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
