//! The selected image's base flake: detection, the static probe, and its collisions.
//!
//! ADR-0112: a directory-form selected image may carry a real `flake.nix` beside its
//! `default.nix`, and the generated flake takes it as an input named after the image,
//! following the baselines it declares. This module is the one place that file is read,
//! and the read is static — `nix eval --file … --apply` forces only the `inputs` attrset
//! and the `outputs` function's argument names, fetches nothing, writes no lock, and
//! needs no `--impure` (findings register, 2026-08-26). `builtins.functionArgs` is what
//! makes an implicit input — an `outputs` argument never declared under `inputs` — count,
//! because Nix counts it (`nix3-flake.1`, "Flake inputs").
//!
//! Deliberately not called from binding-only resolution: `viv config` answers with no Nix
//! process today, and a probe there would put one on a path whose contract is read-only
//! path arithmetic.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use super::error::GeneratedFlakeErrorKind;
use super::input::FlakeInput;
use super::{ArtifactForm, GeneratedFlakeError, ResolvedArtifact};
use crate::diagnostic::{Locus, Namespace};

/// The names the base input itself may not take: the flake's own handle and the two
/// baselines whose redirect the base exists to carry.
const COLLIDING_INPUT_NAMES: [&str; 3] = ["self", "nixpkgs", "microvm"];

/// The expression applied to the imported `flake.nix`; pure, forcing nothing else.
const PROBE_APPLY: &str = "f: let ok = f ? outputs && builtins.isFunction f.outputs; in { \
    explicit = builtins.attrNames (f.inputs or { }); \
    implicit = if ok then builtins.attrNames (builtins.functionArgs f.outputs) else [ ]; \
    outputsIsFunction = ok; }";

/// The selected image's base flake, when it carries one (ADR-0112).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageBase {
    /// The generated-flake input name, which is the image's own name.
    pub input_name: String,
    /// The authored `flake.nix` the probe read.
    pub flake_path: PathBuf,
    /// Whether the base declares `nixpkgs`, explicitly or as an `outputs` argument.
    pub declares_nixpkgs: bool,
    /// Whether the base declares `microvm`, the same way.
    pub declares_microvm: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeReport {
    explicit: Vec<String>,
    implicit: Vec<String>,
    outputs_is_function: bool,
}

/// Probes the selected image for a base flake; `None` when it carries none.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] when the file cannot be inspected, Nix is missing or
/// unusable, the file does not evaluate, or its `outputs` is not a function.
pub(super) fn probe_selected_image(
    image: &ResolvedArtifact,
) -> Result<Option<ImageBase>, GeneratedFlakeError> {
    if image.form != ArtifactForm::Directory {
        return Ok(None);
    }
    let Some(directory) = image.path.parent() else {
        return Ok(None);
    };
    let flake_path = directory.join("flake.nix");
    let exists = flake_path.try_exists().map_err(|source| {
        GeneratedFlakeError::io(
            Namespace::Manifest,
            "base-inspect",
            flake_path.clone(),
            format!("could not inspect `{}`", flake_path.display()),
            source,
        )
    })?;
    if !exists {
        return Ok(None);
    }
    let output = Command::new("nix")
        .args([
            "eval",
            "--extra-experimental-features",
            "nix-command flakes",
            "--json",
            "--file",
        ])
        .arg(&flake_path)
        .args(["--apply", PROBE_APPLY])
        .output()
        .map_err(|source| {
            let kind = match source.kind() {
                std::io::ErrorKind::PermissionDenied => GeneratedFlakeErrorKind::Permission,
                _ => GeneratedFlakeErrorKind::Unavailable,
            };
            GeneratedFlakeError::plain(
                kind,
                Namespace::Manifest,
                "base-probe",
                Locus::File(flake_path.clone()),
                "could not run `nix` to read the image's base flake",
                source.to_string(),
            )
        })?;
    if output.status.success() {
        return interpret_probe(&output.stdout, &image.name, flake_path);
    }
    Err(GeneratedFlakeError::plain(
        GeneratedFlakeErrorKind::Config,
        Namespace::Manifest,
        "base-syntax",
        Locus::File(flake_path),
        format!(
            "the base flake beside image `{}` does not evaluate",
            image.name
        ),
        stderr_tail(&output.stderr),
    ))
}

/// Turns the probe's JSON into the base record; separated so the reading is unit-testable
/// without a `nix` process.
fn interpret_probe(
    stdout: &[u8],
    image_name: &str,
    flake_path: PathBuf,
) -> Result<Option<ImageBase>, GeneratedFlakeError> {
    let report: ProbeReport = match serde_json::from_slice(stdout) {
        Ok(report) => report,
        Err(error) => {
            return Err(GeneratedFlakeError::plain(
                GeneratedFlakeErrorKind::Internal,
                Namespace::Manifest,
                "base-probe",
                Locus::File(flake_path),
                "the base-flake probe returned an undecodable report",
                error.to_string(),
            ));
        }
    };
    if !report.outputs_is_function {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "base-invalid",
            Locus::File(flake_path),
            format!("the base flake beside image `{image_name}` is not a flake"),
            "a flake's `outputs` must be a function",
        ));
    }
    let declares = |name: &str| {
        report.explicit.iter().any(|input| input == name)
            || report.implicit.iter().any(|input| input == name)
    };
    Ok(Some(ImageBase {
        input_name: image_name.to_owned(),
        flake_path,
        declares_nixpkgs: declares("nixpkgs"),
        declares_microvm: declares("microvm"),
    }))
}

/// Refuses a base whose input name cannot join the generated flake's namespace.
///
/// Two shapes, both authored and both decidable before rendering: an image whose own name
/// is a root input the flake already owns, and an artifact-declared input reusing the base's
/// name — refused even for a byte-identical declaration, because the base is a flake and an
/// `inputs.toml` entry is not, so no union exists for them to coalesce under.
pub(super) fn validate_base_collisions(
    base: &ImageBase,
    declared: &BTreeMap<String, FlakeInput>,
) -> Result<(), GeneratedFlakeError> {
    if COLLIDING_INPUT_NAMES.contains(&base.input_name.as_str()) {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "image-base-collision",
            Locus::File(base.flake_path.clone()),
            format!("image `{}` cannot carry a base flake", base.input_name),
            format!(
                "the base input takes the image's name, and `{}` is an input the generated \
                flake already owns; rename the image",
                base.input_name
            ),
        ));
    }
    if let Some(input) = declared.get(&base.input_name) {
        let named_by = input.declarers.first().map_or_else(String::new, |owner| {
            format!(" by {} `{}`", owner.kind, owner.name)
        });
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "image-base-collision",
            Locus::File(base.flake_path.clone()),
            format!(
                "input `{}` is declared{named_by} and is also the selected image's base input",
                base.input_name
            ),
            "the base input takes the image's name; rename the image or the declared input",
        ));
    }
    Ok(())
}

fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let mut lines: Vec<&str> = text.lines().rev().take(4).collect();
    lines.reverse();
    lines.join(" | ")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{ImageBase, interpret_probe, validate_base_collisions};
    use crate::exit::ExitKind;

    fn base(name: &str) -> ImageBase {
        ImageBase {
            input_name: name.to_owned(),
            flake_path: PathBuf::from("/config/images").join(name).join("flake.nix"),
            declares_nixpkgs: false,
            declares_microvm: false,
        }
    }

    /// The probe counts a declaration however Nix counts it: under `inputs`, or as an
    /// `outputs` argument alone — the implicit form the manual defines as a real input.
    #[test]
    fn the_probe_unions_explicit_and_implicit_declarations() {
        let report =
            br#"{"explicit":["nixpkgs"],"implicit":["self","microvm"],"outputsIsFunction":true}"#;
        let probed = interpret_probe(report, "my-base", PathBuf::from("/f"))
            .unwrap()
            .unwrap();
        assert!(probed.declares_nixpkgs);
        assert!(probed.declares_microvm);

        let neither = br#"{"explicit":[],"implicit":["self"],"outputsIsFunction":true}"#;
        let probed = interpret_probe(neither, "my-base", PathBuf::from("/f"))
            .unwrap()
            .unwrap();
        assert!(!probed.declares_nixpkgs);
        assert!(!probed.declares_microvm);
    }

    /// A file whose `outputs` is not a function is refused as authored config, not passed
    /// through for Nix to fail on later where the message would name the generated tree.
    #[test]
    fn a_non_function_outputs_is_refused_as_config() {
        let report = br#"{"explicit":[],"implicit":[],"outputsIsFunction":false}"#;
        let error = interpret_probe(report, "my-base", PathBuf::from("/f")).unwrap_err();
        assert_eq!(error.exit_code(), ExitKind::Config);
        assert!(error.diagnostic().to_string().contains("base-invalid"));
    }

    /// The five collision shapes: `self` and both baselines as a base-bearing image's name,
    /// and a declared input reusing the base's name — the last refused even byte-identical,
    /// because a flake and an `inputs.toml` entry have no union to coalesce under.
    #[test]
    fn every_collision_shape_is_refused() {
        for name in ["self", "nixpkgs", "microvm"] {
            let error = validate_base_collisions(&base(name), &BTreeMap::new()).unwrap_err();
            assert_eq!(error.exit_code(), ExitKind::Config);
            assert!(
                error
                    .diagnostic()
                    .to_string()
                    .contains("image-base-collision")
            );
        }
        let mut declared = BTreeMap::new();
        declared.insert(
            "my-base".to_owned(),
            crate::config::input::FlakeInput {
                url: "github:acme/tool".to_owned(),
                flake: true,
                declarers: vec![crate::config::input::InputDeclarer {
                    kind: crate::config::ArtifactKind::Piece,
                    name: "tooling".to_owned(),
                    path: PathBuf::from("/config/pieces/tooling/inputs.toml"),
                }],
            },
        );
        let error = validate_base_collisions(&base("my-base"), &declared).unwrap_err();
        assert_eq!(error.exit_code(), ExitKind::Config);
        assert!(error.diagnostic().to_string().contains("my-base"));

        assert!(validate_base_collisions(&base("fine"), &declared).is_ok());
    }
}
