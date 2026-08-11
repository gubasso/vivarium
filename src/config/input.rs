//! Closed parsing and deterministic union of artifact-owned flake inputs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::artifact::valid_artifact_name;
use super::{ArtifactKind, InputError};
use crate::diagnostic::Locus;

const ROOT_KEYS: &[&str] = &["inputs"];
const DECLARATION_KEYS: &[&str] = &["url", "flake"];
const RESERVED_NAMES: &[&str] = &["nixpkgs", "microvm"];

/// One generated-flake input, including every artifact that made the same declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlakeInput {
    /// The flake reference supplied to Nix.
    pub url: String,
    /// Whether Nix interprets the source as a flake.
    pub flake: bool,
    pub(super) declarers: Vec<InputDeclarer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct InputDeclarer {
    pub kind: ArtifactKind,
    pub name: String,
    pub path: PathBuf,
}

#[allow(clippy::too_many_lines)]
pub(super) fn parse_inputs(
    source: &str,
    path: &Path,
    declarer: &InputDeclarer,
) -> Result<BTreeMap<String, FlakeInput>, InputError> {
    let document = DeTable::parse(source).map_err(|error| {
        input_error(
            path,
            source,
            error.span().map(|span| span.start),
            "syntax",
            format!("`{}` is not valid TOML", path.display()),
            error.message(),
            &[],
        )
    })?;
    let root = document.get_ref();
    for key in root.keys() {
        if !ROOT_KEYS.contains(&key.get_ref().as_ref()) {
            return Err(input_error(
                path,
                source,
                Some(key.span().start),
                "unknown-key",
                format!("unknown key `{}` in inputs declaration", key.get_ref()),
                format!(
                    "not part of the inputs grammar viv {} understands",
                    env!("CARGO_PKG_VERSION")
                ),
                ROOT_KEYS,
            ));
        }
    }
    let Some(inputs_value) = entry(root, "inputs") else {
        return Ok(BTreeMap::new());
    };
    let inputs = table(inputs_value, path, source, "inputs")?;
    let mut declarations = BTreeMap::new();
    for (name_key, value) in inputs {
        let name = name_key.get_ref().as_ref();
        if !valid_artifact_name(name) {
            return Err(input_error(
                path,
                source,
                Some(name_key.span().start),
                "invalid-value",
                format!("invalid value for input name `{name}`"),
                "expected a kebab-case name",
                &[],
            ));
        }
        if RESERVED_NAMES.contains(&name) {
            return Err(input_error(
                path,
                source,
                Some(name_key.span().start),
                "reserved-input",
                format!("input name `{name}` is reserved"),
                "the generated flake owns its baseline inputs",
                &[],
            ));
        }
        let declaration = table(value, path, source, &format!("inputs.{name}"))?;
        for key in declaration.keys() {
            if !DECLARATION_KEYS.contains(&key.get_ref().as_ref()) {
                return Err(input_error(
                    path,
                    source,
                    Some(key.span().start),
                    "unknown-key",
                    format!("unknown key `{}` in input `{name}`", key.get_ref()),
                    format!(
                        "not part of the inputs grammar viv {} understands",
                        env!("CARGO_PKG_VERSION")
                    ),
                    DECLARATION_KEYS,
                ));
            }
        }
        let Some(url_value) = entry(declaration, "url") else {
            return Err(input_error(
                path,
                source,
                Some(value.span().start),
                "missing-key",
                format!("missing required key `url` in input `{name}`"),
                "the inputs grammar requires it",
                &[],
            ));
        };
        let url = string(url_value, path, source, &format!("inputs.{name}.url"))?;
        if url.is_empty() {
            return Err(input_error(
                path,
                source,
                Some(url_value.span().start),
                "invalid-value",
                format!("invalid value for key `inputs.{name}.url`"),
                "expected a non-empty flake reference",
                &[],
            ));
        }
        let flake = entry(declaration, "flake").map_or(Ok(true), |flag| {
            boolean(flag, path, source, &format!("inputs.{name}.flake"))
        })?;
        declarations.insert(
            name.to_owned(),
            FlakeInput {
                url,
                flake,
                declarers: vec![declarer.clone()],
            },
        );
    }
    Ok(declarations)
}

pub(super) fn merge_inputs(
    union: &mut BTreeMap<String, FlakeInput>,
    declarations: BTreeMap<String, FlakeInput>,
) -> Result<(), InputError> {
    for (name, mut declaration) in declarations {
        if let Some(existing) = union.get_mut(&name) {
            if existing.url == declaration.url && existing.flake == declaration.flake {
                existing.declarers.append(&mut declaration.declarers);
                continue;
            }
            let first = &existing.declarers[0];
            let second = &declaration.declarers[0];
            return Err(InputError::new(
                format!("conflicting declarations for input `{name}`"),
                "input-conflict",
                Locus::File(second.path.clone()),
                format!(
                    "{} `{}` and {} `{}` declare different url or flake values",
                    first.kind, first.name, second.kind, second.name
                ),
                std::iter::empty::<String>(),
            ));
        }
        union.insert(name, declaration);
    }
    Ok(())
}

fn table<'a>(
    value: &'a Spanned<DeValue<'_>>,
    path: &Path,
    source: &str,
    key: &str,
) -> Result<&'a DeTable<'a>, InputError> {
    match value.get_ref() {
        DeValue::Table(table) => Ok(table),
        found => Err(wrong_type(value, path, source, key, "a table", found)),
    }
}

fn string(
    value: &Spanned<DeValue<'_>>,
    path: &Path,
    source: &str,
    key: &str,
) -> Result<String, InputError> {
    match value.get_ref() {
        DeValue::String(text) => Ok(text.as_ref().to_owned()),
        found => Err(wrong_type(value, path, source, key, "a string", found)),
    }
}

fn boolean(
    value: &Spanned<DeValue<'_>>,
    path: &Path,
    source: &str,
    key: &str,
) -> Result<bool, InputError> {
    match value.get_ref() {
        DeValue::Boolean(flag) => Ok(*flag),
        found => Err(wrong_type(value, path, source, key, "a boolean", found)),
    }
}

fn wrong_type(
    value: &Spanned<DeValue<'_>>,
    path: &Path,
    source: &str,
    key: &str,
    expected: &'static str,
    found: &DeValue<'_>,
) -> InputError {
    input_error(
        path,
        source,
        Some(value.span().start),
        "wrong-type",
        format!("wrong type for key `{key}` in inputs declaration"),
        format!("expected {expected}, found {}", type_name(found)),
        &[],
    )
}

fn input_error(
    path: &Path,
    source: &str,
    offset: Option<usize>,
    condition: &'static str,
    message: impl Into<String>,
    why: impl Into<String>,
    accepted: &[&str],
) -> InputError {
    InputError::new(
        message,
        condition,
        offset.map_or_else(
            || Locus::File(path.to_path_buf()),
            |offset| Locus::in_source(path, source, offset),
        ),
        why,
        accepted.iter().copied(),
    )
}

fn entry<'a>(table: &'a DeTable<'_>, key: &str) -> Option<&'a Spanned<DeValue<'a>>> {
    table
        .iter()
        .find_map(|(candidate, value)| (candidate.get_ref().as_ref() == key).then_some(value))
}

const fn type_name(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "a string",
        DeValue::Integer(_) => "an integer",
        DeValue::Float(_) => "a float",
        DeValue::Boolean(_) => "a boolean",
        DeValue::Datetime(_) => "a date or time",
        DeValue::Array(_) => "an array",
        DeValue::Table(_) => "a table",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{InputDeclarer, merge_inputs, parse_inputs};
    use crate::config::ArtifactKind;
    use crate::exit::ExitKind;

    fn declarer(name: &str) -> InputDeclarer {
        InputDeclarer {
            kind: ArtifactKind::Piece,
            name: name.to_owned(),
            path: Path::new("pieces/demo/inputs.toml").to_path_buf(),
        }
    }

    #[test]
    fn parses_defaults_and_non_flake_inputs() -> Result<(), Box<dyn std::error::Error>> {
        let inputs = parse_inputs(
            "[inputs.alpha]\nurl = 'github:one/a'\n[inputs.zed]\nurl = 'path:x'\nflake = false\n",
            Path::new("inputs.toml"),
            &declarer("demo"),
        )?;
        assert!(inputs["alpha"].flake);
        assert!(!inputs["zed"].flake);
        assert_eq!(inputs.keys().collect::<Vec<_>>(), vec!["alpha", "zed"]);
        Ok(())
    }

    #[test]
    fn rejects_closed_grammar_name_type_and_reserved_defects()
    -> Result<(), Box<dyn std::error::Error>> {
        for source in [
            "other = 1",
            "inputs = []",
            "[inputs.Bad]\nurl = 'x'",
            "[inputs.nixpkgs]\nurl = 'x'",
            "[inputs.ok]\nurl = 1",
            "[inputs.ok]\nurl = 'x'\nrevision = 'y'",
        ] {
            let error = parse_inputs(source, Path::new("inputs.toml"), &declarer("demo"))
                .err()
                .ok_or("fixture unexpectedly succeeded")?;
            assert_eq!(error.exit_code(), ExitKind::Config);
            assert!(error.diagnostic().to_string().contains("inputs.toml"));
        }
        Ok(())
    }

    #[test]
    fn identical_declarations_coalesce_and_conflicts_name_both_artifacts()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut union = parse_inputs(
            "[inputs.shared]\nurl = 'github:one/a'",
            Path::new("one/inputs.toml"),
            &declarer("one"),
        )?;
        merge_inputs(
            &mut union,
            parse_inputs(
                "[inputs.shared]\nurl = 'github:one/a'",
                Path::new("two/inputs.toml"),
                &declarer("two"),
            )?,
        )?;
        assert_eq!(union["shared"].declarers.len(), 2);

        let conflict = merge_inputs(
            &mut union,
            parse_inputs(
                "[inputs.shared]\nurl = 'github:other/b'",
                Path::new("three/inputs.toml"),
                &declarer("three"),
            )?,
        )
        .err()
        .ok_or("different declaration unexpectedly coalesced")?;
        assert_eq!(conflict.exit_code(), ExitKind::Config);
        assert!(conflict.to_string().contains("conflicting declarations"));
        Ok(())
    }
}
