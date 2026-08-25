//! The project-scope probes: early warnings about the bound composition.
//!
//! Every one is soft and every one is a reader; the authoritative refusals live at evaluation
//! (`65`) and launch (`78`), where spec/13 places them. The textual lints never evaluate
//! and never expand — a fault that only appears after host-side expansion is invisible here by
//! design, so a clean lint is not a promise. The judgments are pure functions their tests
//! exercise; the file reads around them are the only impure part.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::config::{self, ArtifactForm, ArtifactKind, Environment, Manifest, ResolvedArtifact};
use crate::launch::agent_source;
use crate::protocol::CredentialId;

use super::{Finding, Inputs, Probe, ProjectInputs};

pub(super) fn run<E: Environment>(
    probe: &'static Probe,
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> Finding {
    match probe.id {
        "config-parses" => config_parses(probe, project),
        "manifest-resolves" => manifest_resolves(probe, project),
        "working-directory-declared" => working_directory_declared(probe, project),
        "shared-layer-paths-portable" => shared_layer_paths(probe, project, inputs),
        "manifest-no-inline-secret" => manifest_no_inline_secret(probe, project),
        "mount-source-not-session-dir" => mount_sources(probe, project),
        "lock-covers-declared-inputs" => lock_covers_inputs(probe, project, inputs),
        "agent-source-usable" => agent_source_usable(probe, project, inputs),
        _ => Finding::skipped(probe, "not-applicable", "not a project probe"),
    }
}

fn working_directory_declared(probe: &'static Probe, project: &ProjectInputs) -> Finding {
    project.workspace_refusal.as_ref().map_or_else(
        || {
            Finding::pass(
                probe,
                "the working directory belongs to a declared workspace",
            )
        },
        |(message, hint)| Finding::tripped(probe, message, hint),
    )
}

fn config_parses(probe: &'static Probe, project: &ProjectInputs) -> Finding {
    match &project.parsed {
        Ok(_) => Finding::pass(probe, format!("`{}` parses", project.manifest)),
        Err(why) => Finding::tripped(
            probe,
            format!("the bound manifest does not parse: {why}"),
            "fix the manifest; `viv config eval` renders the full defect",
        ),
    }
}

fn manifest_resolves(probe: &'static Probe, project: &ProjectInputs) -> Finding {
    match &project.artifact {
        Ok(artifact) => Finding::pass(
            probe,
            format!(
                "`{}` resolves to {}",
                project.manifest,
                artifact.path.display()
            ),
        ),
        Err(why) => Finding::tripped(
            probe,
            format!("the selected manifest does not resolve: {why}"),
            "define the manifest in the library and declare this directory in `[[workspaces]]`",
        ),
    }
}

/// The shared layers the resolved composition is made of: the image's artifact, then each
/// piece's. spec/13 scopes the portability and lock lints to shared layers — the bound manifest
/// is the personal composition, so a personal path in it is legitimate and it is deliberately
/// not linted. A `ResolvedArtifact`'s `path` is already the entry file in both forms (ADR-0045).
fn shared_artifacts<E: Environment>(
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> Vec<ResolvedArtifact> {
    let mut artifacts = Vec::new();
    if let Ok(manifest) = &project.parsed {
        if let Ok(artifact) =
            config::resolve_artifact(&inputs.roots.config, ArtifactKind::Image, &manifest.image)
        {
            artifacts.push(artifact);
        }
        for piece in &manifest.pieces {
            if let Ok(artifact) =
                config::resolve_artifact(&inputs.roots.config, ArtifactKind::Piece, piece)
            {
                artifacts.push(artifact);
            }
        }
    }
    artifacts
}

fn shared_layer_paths<E: Environment>(
    probe: &'static Probe,
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> Finding {
    for artifact in shared_artifacts(project, inputs) {
        let file = &artifact.path;
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        if let Some((line, found)) = personal_path_lint(&text) {
            return Finding::tripped(
                probe,
                format!(
                    "`{}` line {line} declares the literal personal path `{found}`",
                    file.display()
                ),
                "use a portable variable like `${HOME}` (N11); the authoritative check \
                runs at evaluation and returns `65`",
            );
        }
    }
    Finding::pass(probe, "no shared layer declares a literal personal path")
}

/// The first literal `/home/...` or `/Users/...` in the text, with its one-based line.
///
/// Textual and best-effort per spec/13: a path that only becomes personal after expansion is
/// invisible here, and a clean answer is not a promise.
fn personal_path_lint(text: &str) -> Option<(usize, String)> {
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        for marker in ["/home/", "/Users/"] {
            if let Some(position) = line.find(marker) {
                let literal: String = line[position..]
                    .chars()
                    .take_while(|character| !"\"' \t".contains(*character))
                    .collect();
                return Some((index + 1, literal));
            }
        }
    }
    None
}

fn manifest_no_inline_secret(probe: &'static Probe, project: &ProjectInputs) -> Finding {
    let Ok(manifest) = &project.parsed else {
        return Finding::skipped(probe, "not-applicable", "the manifest does not parse");
    };
    if let Some((key, why)) = inline_secret_lint(manifest) {
        Finding::tripped(
            probe,
            format!("`[env] {key}` looks like a credential ({why})"),
            "manifest text lands in the world-readable store (N10); carry secrets through \
            an agent channel or the guest's own environment instead",
        )
    } else {
        Finding::pass(probe, "no `[env]` value looks like a credential")
    }
}

/// The first `[env]` entry that looks like a credential, and why it tripped.
///
/// A heuristic, deliberately best-effort (spec/13): a value it does not flag is not a promise
/// the value is safe. Two families: a value carrying a known token prefix, and a secret-shaped
/// key name with a long opaque value.
fn inline_secret_lint(manifest: &Manifest) -> Option<(String, &'static str)> {
    const TOKEN_PREFIXES: &[&str] = &[
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "github_pat_",
        "glpat-",
        "sk-",
        "xoxb-",
        "xoxp-",
        "AKIA",
        "-----BEGIN",
    ];
    const SECRET_KEYS: &[&str] = &["token", "secret", "password", "passwd", "api_key", "apikey"];
    for (key, value) in &manifest.env {
        if TOKEN_PREFIXES
            .iter()
            .any(|prefix| value.starts_with(prefix))
        {
            return Some((key.clone(), "the value carries a known credential prefix"));
        }
        let lowered = key.to_lowercase();
        if SECRET_KEYS.iter().any(|name| lowered.contains(name)) && value.len() >= 16 {
            return Some((key.clone(), "a secret-shaped key with a long opaque value"));
        }
    }
    None
}

fn mount_sources(probe: &'static Probe, project: &ProjectInputs) -> Finding {
    let Ok(manifest) = &project.parsed else {
        return Finding::skipped(probe, "not-applicable", "the manifest does not parse");
    };
    for mount in &manifest.mounts {
        if let Some(marker) = session_dir_lint(&mount.source) {
            return Finding::tripped(
                probe,
                format!("mount source `{}` resolves under `{marker}`", mount.source),
                "a session directory ends with the session (N24); mount from a durable \
                path — the authoritative refusals are `65` at evaluation and `78` at launch",
            );
        }
    }
    Finding::pass(probe, "no mount source resolves under a session directory")
}

/// The session-directory marker a source textually starts with, if any (N24).
///
/// Textual: it never expands, so a source that only becomes a session path after host-side
/// expansion is invisible here — spec/13 says so and the launch refusal owns that case.
fn session_dir_lint(source: &str) -> Option<&'static str> {
    ["/tmp/", "/var/tmp/", "${XDG_RUNTIME_DIR}"]
        .into_iter()
        .find(|marker| source.starts_with(marker) || source == marker.trim_end_matches('/'))
}

fn lock_covers_inputs<E: Environment>(
    probe: &'static Probe,
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> Finding {
    let declared = declared_input_names(project, inputs);
    if declared.is_empty() {
        return Finding::pass(probe, "the composition declares no extra flake inputs");
    }
    let Some(lock_path) = &project.lock_path else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "no lock exists yet; the first evaluation creates it",
        );
    };
    let Ok(lock_text) = std::fs::read_to_string(lock_path) else {
        return Finding::skipped(
            probe,
            "not-applicable",
            format!("the lock at `{}` could not be read", lock_path.display()),
        );
    };
    let nodes = lock_node_names(&lock_text);
    let missing: Vec<&String> = declared
        .iter()
        .filter(|name| !nodes.contains(*name))
        .collect();
    if missing.is_empty() {
        Finding::pass(
            probe,
            format!("the lock covers all {} declared inputs", declared.len()),
        )
    } else {
        Finding::tripped(
            probe,
            format!(
                "the lock at `{}` carries no node for `{}`",
                lock_path.display(),
                missing[0]
            ),
            "under the tool-owned lock, `viv update` moves it; under an override, the team's \
            own act does — the build path refuses at `78` with the same fact",
        )
    }
}

/// The input names the shared layers' `inputs.toml` files declare, deduplicated.
fn declared_input_names<E: Environment>(
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    // Only the directory form may carry an `inputs.toml`, beside its entry file (ADR-0045).
    let artifact_dirs: Vec<PathBuf> = shared_artifacts(project, inputs)
        .into_iter()
        .filter(|artifact| artifact.form == ArtifactForm::Directory)
        .filter_map(|artifact| artifact.path.parent().map(Path::to_path_buf))
        .collect();
    for directory in artifact_dirs {
        let Ok(text) = std::fs::read_to_string(directory.join("inputs.toml")) else {
            continue;
        };
        names.extend(inputs_toml_names(&text));
    }
    names
}

/// The names under `[inputs.<name>]` in one `inputs.toml`, by name comparison alone.
///
/// A parse of the shape and nothing else — it resolves nothing and touches no network (spec/13);
/// a file the strict parser would refuse yields no names here and is the build path's to refuse.
fn inputs_toml_names(text: &str) -> BTreeSet<String> {
    let Ok(document) = text.parse::<toml::Table>() else {
        return BTreeSet::new();
    };
    document
        .get("inputs")
        .and_then(|inputs| inputs.as_table())
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default()
}

/// The node names the lock's `nodes` object carries.
fn lock_node_names(lock_text: &str) -> BTreeSet<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(lock_text) else {
        return BTreeSet::new();
    };
    value
        .get("nodes")
        .and_then(|nodes| nodes.as_object())
        .map(|nodes| nodes.keys().cloned().collect())
        .unwrap_or_default()
}

/// Whether every declared agent channel has a usable host source: spec/13's four faults, named
/// one at a time because "unset", "missing", "wrong type", and "wrong owner" have four repairs.
///
/// The declaration is found textually, like this file's other lints: a channel a layer computes
/// rather than writes is invisible here, and a clean answer is not a promise. Every composed
/// layer's text the probe can reach is read — the image, the pieces, and the manifest's own
/// `extends` module, which is a composed layer too (spec/04) even though the personal-layer
/// lints deliberately skip it. The source check itself is the launch path's own resolver, so the
/// warning and the `78` refusal cannot drift — but the probe stops at the socket's existence and
/// ownership. It never connects and never asks an agent to sign or decrypt, because `doctor`
/// changes nothing (spec/13).
fn agent_source_usable<E: Environment>(
    probe: &'static Probe,
    project: &ProjectInputs,
    inputs: &Inputs<'_, E>,
) -> Finding {
    let mut declared: Vec<CredentialId> = Vec::new();
    let collect = |text: &str, declared: &mut Vec<CredentialId>| {
        for id in declared_agents_lint(text) {
            if !declared.contains(&id) {
                declared.push(id);
            }
        }
    };
    for artifact in shared_artifacts(project, inputs) {
        let Ok(text) = std::fs::read_to_string(&artifact.path) else {
            continue;
        };
        collect(&text, &mut declared);
    }
    // The same lexical join `resolve_extends` performs; validation is its job, not this lint's.
    if let (Ok(manifest), Ok(artifact)) = (&project.parsed, &project.artifact)
        && let Some(extends) = &manifest.extends
        && let Some(directory) = artifact.path.parent()
        && let Ok(text) = std::fs::read_to_string(directory.join(extends))
    {
        collect(&text, &mut declared);
    }
    if declared.is_empty() {
        return Finding::skipped(
            probe,
            "not-applicable",
            "no composed layer declares an agent channel",
        );
    }
    let lookup =
        |name: &str| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    let uid = config::effective_uid();
    for id in declared {
        use agent_source::AgentSourceDefect as Defect;
        let Err(defect) = agent_source::resolve_and_check(id, &lookup, uid) else {
            continue;
        };
        let (fault, hint) = match defect {
            Defect::Unset { consulted } => (
                format!("`{consulted}` names nothing on this host"),
                "start the agent that serves this channel",
            ),
            Defect::Missing { path } => (
                format!("`{}` does not exist", path.display()),
                "restart the agent; its socket is gone",
            ),
            Defect::NotSocket { path } => (
                format!("`{}` is not a socket", path.display()),
                "point the source at the agent's socket object itself",
            ),
            Defect::NotOwned { path, owner, uid } => (
                format!(
                    "`{}` is owned by uid {owner}, not this user ({uid})",
                    path.display()
                ),
                "use your own agent's socket, never another user's",
            ),
        };
        return Finding::tripped(
            probe,
            format!("the declared `{id}` channel's host source is not usable: {fault}"),
            hint,
        );
    }
    Finding::pass(
        probe,
        "every declared agent channel names a usable host socket",
    )
}

/// The credential ids a layer's text declares under `credentials.agents`.
///
/// Textual and best-effort per spec/13: the canonical spelling every guide shows —
/// `vivarium.credentials.agents = [ "ssh" ]`, single or multi line — is read, comment lines are
/// skipped, and anything computed is invisible. The authoritative reader is the evaluation the
/// report performs.
fn declared_agents_lint(text: &str) -> Vec<CredentialId> {
    let mut declared: Vec<CredentialId> = Vec::new();
    let mut collecting = false;
    for raw in text.lines() {
        let line = raw.trim_start();
        if line.starts_with('#') {
            continue;
        }
        let mut segment = line;
        if !collecting {
            let Some(index) = segment.find("credentials.agents") else {
                continue;
            };
            segment = &segment[index..];
            collecting = true;
        }
        let (scan, closed) = segment
            .find(']')
            .map_or((segment, false), |end| (&segment[..end], true));
        for name in ["ssh", "gpg"] {
            if scan.contains(&format!("\"{name}\""))
                && let Ok(id) = name.parse::<CredentialId>()
                && !declared.contains(&id)
            {
                declared.push(id);
            }
        }
        if closed {
            collecting = false;
        }
    }
    declared
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_personal_path_lint_reads_literals_and_skips_comments() {
        assert_eq!(
            personal_path_lint("source = \"/home/alice/backend\"\n"),
            Some((1, "/home/alice/backend".to_owned()))
        );
        assert_eq!(
            personal_path_lint("# /home/alice is fine in a comment\nsource = \"${HOME}/x\"\n"),
            None
        );
        assert!(personal_path_lint("path = '/Users/bob/code'").is_some());
    }

    #[test]
    fn the_agents_lint_reads_the_canonical_spellings_and_skips_comments() {
        // Single line, multi line, and the id set deduplicated across layers of one text.
        assert_eq!(
            declared_agents_lint("{ vivarium.credentials.agents = [ \"ssh\" ]; }\n"),
            vec![CredentialId::Ssh]
        );
        assert_eq!(
            declared_agents_lint(
                "vivarium.credentials.agents = [\n  \"ssh\"\n  \"gpg\"\n  \"ssh\"\n];\n"
            ),
            vec![CredentialId::Ssh, CredentialId::Gpg]
        );
        // A comment is not a declaration, and text outside the list is not a member.
        assert_eq!(
            declared_agents_lint("# vivarium.credentials.agents = [ \"ssh\" ]\n"),
            Vec::<CredentialId>::new()
        );
        assert_eq!(
            declared_agents_lint("packages = [ \"ssh\" ];\ndescription = \"gpg\";\n"),
            Vec::<CredentialId>::new()
        );
        assert_eq!(
            declared_agents_lint("vivarium.credentials.agents = [ ];\nother = \"ssh\";\n"),
            Vec::<CredentialId>::new()
        );
    }

    #[test]
    fn the_session_dir_lint_is_textual_and_unexpanded() {
        assert_eq!(session_dir_lint("/tmp/scratch"), Some("/tmp/"));
        assert_eq!(session_dir_lint("/var/tmp/x"), Some("/var/tmp/"));
        assert_eq!(
            session_dir_lint("${XDG_RUNTIME_DIR}/socket"),
            Some("${XDG_RUNTIME_DIR}")
        );
        // Unexpanded on purpose: `${TMPDIR}` may resolve under /tmp and stays invisible here.
        assert_eq!(session_dir_lint("${TMPDIR}/scratch"), None);
        assert_eq!(session_dir_lint("/tmpfiles/x"), None);
    }

    #[test]
    fn the_secret_lint_flags_prefixes_and_secret_shaped_keys() {
        let mut manifest = Manifest {
            image: "base".to_owned(),
            pieces: Vec::new(),
            extends: None,
            resources: None,
            egress: None,
            env: std::collections::BTreeMap::new(),
            workspaces: Vec::new(),
            mounts: Vec::new(),
            volumes: Vec::new(),
            volume: None,
        };
        manifest
            .env
            .insert("GITHUB_TOKEN".into(), "ghp_abcdef0123456789".into());
        assert!(inline_secret_lint(&manifest).is_some());

        manifest.env.clear();
        manifest
            .env
            .insert("API_KEY".into(), "0123456789abcdef0123".into());
        assert!(inline_secret_lint(&manifest).is_some());

        // Deliberately not flagged: a short flag-like value under a secret-shaped key, and an
        // ordinary long value under an ordinary key. Best-effort is the contract.
        manifest.env.clear();
        manifest.env.insert("USE_TOKEN".into(), "1".into());
        manifest
            .env
            .insert("PATH_HINT".into(), "/usr/local/share/something/long".into());
        assert!(inline_secret_lint(&manifest).is_none());
    }

    #[test]
    fn declared_names_and_lock_nodes_compare_by_name() {
        let declared = inputs_toml_names("[inputs.rust-overlay]\nurl = \"github:x/y\"\n");
        assert_eq!(declared.len(), 1);
        assert!(declared.contains("rust-overlay"));
        let nodes = lock_node_names(
            r#"{"nodes": {"nixpkgs": {}, "microvm": {}, "root": {}}, "version": 7}"#,
        );
        assert!(nodes.contains("nixpkgs"));
        assert!(!nodes.contains("rust-overlay"));
    }
}
