//! Running Nix against a published generated flake, and reading back what it said.
//!
//! One process invocation per command, against one attribute. The generated flake carries the
//! whole report expression (ADR-0058 makes its internals the tool's own), so nothing here composes
//! Nix source at the call site: a host-side expression string would be a second place where the
//! option surface is spelled, and the two would drift.
//!
//! The lock is the other thing this module owns. A build never re-resolves inputs — that is the
//! unannounced input jump N3 forbids — so a preparation that already has a pin runs with
//! `--no-update-lock-file` and fails when a declared input has no node. Only the first evaluation
//! of a target may create one, and the pin it created is then installed under the data root by
//! `persist_created_lock`, which is where ADR-0059 puts it.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::ui::{Ui, watch};

use super::error::EvaluationError;
use super::{EffectiveLock, PreparedFlake, flake, materialize};

/// The systems the generated flake publishes, which are the systems a microVM boots (N1).
const SUPPORTED_SYSTEMS: [(&str, &str); 2] =
    [("x86_64", "x86_64-linux"), ("aarch64", "aarch64-linux")];

/// Enabled explicitly rather than assumed: the flake interface is still gated in a default Nix
/// installation, and a user who has not opted in would otherwise get an error about experimental
/// features for a command that never mentioned them.
pub(crate) const FEATURE_FLAGS: [&str; 2] = ["--extra-experimental-features", "nix-command flakes"];

/// Everything one evaluation of the report attribute produced.
#[derive(Clone, Debug, Deserialize)]
pub struct Report {
    /// Each tracked key's effective value, by the manifest's own key names.
    pub keys: std::collections::BTreeMap<String, KeyReport>,
    /// Each composed layer and what it contributed, in merge order.
    pub layers: Vec<LayerReport>,
}

/// One tracked key as the merge left it.
#[derive(Clone, Debug, Deserialize)]
pub struct KeyReport {
    /// Whether two definitions surviving at one priority is a defect for this key.
    pub class: KeyClass,
    /// Whether the merged value could be produced at all.
    pub ok: bool,
    /// The merged value, `null` when `ok` is false.
    pub value: Value,
}

/// Whether a key merges by replacement or by concatenation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KeyClass {
    /// Resolves by priority, so two survivors at one priority is an equal-priority tie.
    Scalar,
    /// Concatenates across layers, so two contributors are cooperation rather than collision.
    List,
}

/// One composed layer's own contribution.
#[derive(Clone, Debug, Deserialize)]
pub struct LayerReport {
    /// The artifact name a user would recognize it by.
    pub name: String,
    /// Which role it plays in the merge.
    pub kind: LayerKind,
    /// Whether the layer could be evaluated on its own at all.
    pub readable: bool,
    /// The keys it defines, and at what priority.
    pub defines: std::collections::BTreeMap<String, Definition>,
}

/// The four roles a module can hold in one composition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    Image,
    Piece,
    Extends,
    Manifest,
}

impl LayerKind {
    /// The word the provenance view prints in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Piece => "piece",
            Self::Extends => "extends",
            Self::Manifest => "manifest",
        }
    }

    /// Whether a layer of this kind is shared with other people.
    ///
    /// The manifest is the personal layer (spec/07, ADR-0040), and everything else is adoptable by
    /// someone else. That is exactly the distinction N11 draws: a literal personal path is a defect
    /// in a shared layer and ordinary authorship in a manifest.
    #[must_use]
    pub const fn is_shared(self) -> bool {
        matches!(self, Self::Image | Self::Piece | Self::Extends)
    }
}

/// One definition of one key by one layer.
#[derive(Clone, Debug, Deserialize)]
pub struct Definition {
    /// The module-system override priority: lower wins. 50 is `mkForce`, 100 normal, 1000
    /// `mkDefault` — the three tiers spec/04's convention assigns roles to.
    pub prio: i64,
    /// The value this layer set, already projected to the manifest's own shape.
    pub value: Value,
}

impl Definition {
    /// The word spec/01's provenance block prints in the priority column.
    #[must_use]
    pub const fn priority_label(&self) -> &'static str {
        // Named by tier rather than by number, and by comparison rather than equality: a layer may
        // reach a tier through `mkOverride` with its own number, and it still belongs to whichever
        // role it outranks.
        if self.prio < 100 {
            "mkForce"
        } else if self.prio <= 100 {
            "normal"
        } else {
            "mkDefault"
        }
    }
}

/// Evaluates the report attribute for this host's system.
///
/// # Errors
///
/// Returns [`EvaluationError`] when Nix is absent or unusable, when evaluation fails, when the lock
/// in force has no node for a declared input, or when the report cannot be decoded.
pub fn report(prepared: &PreparedFlake, ui: &Ui) -> Result<Report, EvaluationError> {
    let attribute = format!("{}.{}", flake::REPORT_ATTR, host_system());
    let output = run(
        prepared,
        "eval",
        &[
            &format!("{}#{attribute}", display(&prepared.directory)),
            "--json",
        ],
        ui,
    )?;
    serde_json::from_slice(&output.stdout).map_err(|error| EvaluationError::Undecodable {
        detail: error.to_string(),
    })
}

/// Installs the pin a first evaluation created, when this preparation was eligible for one.
///
/// Separate from [`report`] because creating a pin is a write and evaluation is not: a caller that
/// only reads must be able to do so without this, and a caller that does persist should be the one
/// deciding to. Returns the lock in force afterwards.
///
/// # Errors
///
/// Returns [`super::GeneratedFlakeError`] when the created lock cannot be read or installed, or
/// when a different concurrent first pin won.
pub fn persist_first_pin(
    prepared: &PreparedFlake,
) -> Result<EffectiveLock, super::GeneratedFlakeError> {
    if !prepared.effective_lock.may_persist_created() {
        return Ok(prepared.effective_lock.clone());
    }
    materialize::persist_created_lock(prepared)
}

fn run(
    prepared: &PreparedFlake,
    verb: &'static str,
    arguments: &[&str],
    ui: &Ui,
) -> Result<watch::Captured, EvaluationError> {
    let mut command = Command::new("nix");
    command.arg(verb);
    command.args(FEATURE_FLAGS);
    command.args(arguments);
    // A preparation that already has a pin must not move it. Passing this unconditionally would
    // make the very first evaluation of a target fail for want of the file it is allowed to create.
    if !prepared.effective_lock.may_persist_created() {
        command.arg("--no-update-lock-file");
    }
    // Three answers, not two: absent is `69`, forbidden is `77`, and anything else stays the
    // unclassified `69`. Spec/14's `config` rows admit both codes for the Nix preflight, and
    // collapsing the middle case into "unavailable" would tell a user to install what they have.
    let step = ui.step("evaluating the merged configuration");
    let output = watch::output(&mut command, &step).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => EvaluationError::NixMissing { source },
        std::io::ErrorKind::PermissionDenied => EvaluationError::NixNotPermitted { source },
        _ => EvaluationError::NixUnusable { source },
    })?;
    if output.status.success() {
        step.done("evaluated the merged configuration");
        return Ok(output);
    }
    drop(step);
    let stderr = output.stderr;
    if names_a_missing_lock_node(&stderr) {
        return Err(EvaluationError::LockMissingNode { stderr });
    }
    Err(EvaluationError::NixFailed { verb, stderr })
}

/// Whether Nix refused because the lock in force does not cover a declared input.
///
/// Matched on the message because Nix reports it as an ordinary evaluation failure and offers no
/// machine-readable class for it. The alternative is worse than a fragile match: without this the
/// case spec/04 fixes at `78` would reach a user as `70`, a Nix fault they did not cause and cannot
/// act on, instead of a lock they can move with one command. A miss here degrades to `70`, which is
/// the honest fallback rather than a wrong `78`.
fn names_a_missing_lock_node(stderr: &str) -> bool {
    let lowered = stderr.to_lowercase();
    lowered.contains("lock file")
        && (lowered.contains("does not have") || lowered.contains("must be updated"))
}

/// The flake system this host evaluates for.
fn host_system() -> &'static str {
    let architecture = std::env::consts::ARCH;
    SUPPORTED_SYSTEMS
        .iter()
        .find(|(arch, _)| *arch == architecture)
        // Not a failure path: N1 makes a microVM host Linux on one of these two architectures, and
        // a third would fail at the flake attribute with Nix naming what it does not publish.
        .map_or("x86_64-linux", |(_, system)| *system)
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::{Definition, KeyClass, LayerKind, Report, names_a_missing_lock_node};

    #[test]
    fn decodes_a_report_and_labels_its_priorities() -> Result<(), Box<dyn std::error::Error>> {
        let report: Report = serde_json::from_str(
            r#"{
                "keys": {
                    "resources.mem_mib": { "class": "scalar", "ok": false, "value": null },
                    "mounts": { "class": "list", "ok": true, "value": [] }
                },
                "layers": [
                    {
                        "name": "base", "kind": "image", "readable": true,
                        "defines": { "sandbox.egress.mode": { "prio": 1000, "value": "open" } }
                    },
                    { "name": "ana", "kind": "manifest", "readable": true, "defines": {} }
                ]
            }"#,
        )?;
        assert_eq!(report.keys["resources.mem_mib"].class, KeyClass::Scalar);
        assert!(!report.keys["resources.mem_mib"].ok);
        assert_eq!(report.keys["mounts"].class, KeyClass::List);
        assert_eq!(report.layers[0].kind, LayerKind::Image);
        assert!(report.layers[0].kind.is_shared());
        assert!(!report.layers[1].kind.is_shared());
        Ok(())
    }

    #[test]
    fn priority_labels_follow_the_tier_and_not_the_number() {
        let force = Definition {
            prio: 50,
            value: serde_json::Value::Null,
        };
        let normal = Definition {
            prio: 100,
            value: serde_json::Value::Null,
        };
        let proposal = Definition {
            prio: 1000,
            value: serde_json::Value::Null,
        };
        assert_eq!(force.priority_label(), "mkForce");
        assert_eq!(normal.priority_label(), "normal");
        assert_eq!(proposal.priority_label(), "mkDefault");
        // A layer reaching the floor through its own `mkOverride` number still holds the floor.
        assert_eq!(
            Definition {
                prio: 10,
                value: serde_json::Value::Null,
            }
            .priority_label(),
            "mkForce"
        );
    }

    #[test]
    fn a_refused_input_jump_is_told_apart_from_an_ordinary_fault() {
        assert!(names_a_missing_lock_node(
            "error: lock file '/x/flake.lock' does not have an entry for input 'zed'"
        ));
        assert!(!names_a_missing_lock_node(
            "error: attribute 'nixosConfigurations' missing"
        ));
    }
}
