//! `viv doctor`: the catalog run, its gathering, and its exit mapping (spec/13).
//!
//! The verb is a pure checker — it reads, judges, and changes nothing (`ADR-0022`). What lives
//! here is only what the verb adds over the catalog in [`crate::doctor`]: gathering the injected
//! inputs leniently (a fault in gathering is a finding for a probe to report, never this verb's
//! own failure), the skip note, and the exit mapping. Dispatched by `main` rather than through
//! `run` because a report at `69` is still a report: the result prints and the code speaks.

use std::path::PathBuf;

use crate::config::{self, Environment};
use crate::doctor::{self, Finding, Inputs, ProjectInputs, Severity, Status};
use crate::exit::ExitKind;

use super::{Context, Failure, Success, render};
use crate::cli::grammar::Output;

/// Runs the verb and returns the report beside the code the outcome maps to.
///
/// # Errors
///
/// Returns [`Failure`] only for an internal fault in `doctor` itself (`70`); every host, project,
/// and network condition is a finding in the report, not a failure of the verb.
pub fn command<E: Environment>(
    context: &Context<'_, E>,
    strict: bool,
    list: bool,
    online: bool,
    output: Output,
) -> Result<(Success, ExitKind), Failure> {
    if list {
        // The catalog enumerated, nothing probed: id, category, scope, severity, title.
        let stdout = if output.is_json() {
            render::doctor_list_json()
        } else {
            render::doctor_list_human(context.ui.palette_out())
        };
        return Ok((Success::plain(stdout), ExitKind::Success));
    }

    let inputs = gather(context, online);
    let step = context.ui.step("probing the host");
    let findings = doctor::run(&inputs);
    drop(step);

    // The one stderr note spec/13 fixes: project probes skipped for want of a unique owner, said
    // once with the way in. Ambiguity is the case where `project` is `None` and the reason is the
    // opposite of the usual one — two manifests declare this directory, not none — so it gets its
    // own sentence rather than the note that would contradict the findings above it.
    let notes = match (&inputs.ownership_ambiguity, inputs.project.is_none()) {
        (Some(_), _) => {
            "project checks skipped: more than one manifest declares this workspace\n".to_owned()
        }
        (None, true) => "project checks skipped: no manifest declares this workspace\n".to_owned(),
        (None, false) => String::new(),
    };

    let code = exit_code(&findings, strict);
    let stdout = if output.is_json() {
        render::doctor_json(&findings)
    } else {
        render::doctor_human(&findings, code, context.ui.palette_out())
    };
    Ok((Success { stdout, notes }, code))
}

/// The outcome mapping spec/13 fixes: the first hard failure in catalog order decides the code;
/// `--strict` promotes a warn to `1` — the one sanctioned non-sysexits code (ADR-0023); skips
/// never affect the exit.
fn exit_code(findings: &[Finding], strict: bool) -> ExitKind {
    if let Some(failed) = findings
        .iter()
        .find(|finding| finding.status == Status::Fail && finding.probe.severity == Severity::Hard)
    {
        return failed.probe.code.unwrap_or(ExitKind::Software);
    }
    if strict
        && findings
            .iter()
            .any(|finding| finding.status == Status::Warn)
    {
        return ExitKind::DoctorStrict;
    }
    ExitKind::Success
}

/// Gathers the run's injected inputs, leniently: whatever cannot be read arrives as the absence
/// or the error a probe exists to report.
fn gather<'a, E: Environment>(context: &'a Context<'_, E>, online: bool) -> Inputs<'a, E> {
    let resolution = super::resolve_manifest_for_doctor(context);
    // The one condition where there is no single project to gather and that fact is itself the
    // finding. Held rather than discarded, so the catalog reports it instead of skipping.
    let ownership_ambiguity = match &resolution {
        Err(super::ResolveFailure::Ambiguous(finding)) => Some((finding.message(), finding.hint())),
        _ => None,
    };
    let project = resolution
        .ok()
        .map(|resolved| {
            let refusal = resolved
                .workspace_refusal
                .as_ref()
                .map(|finding| (finding.message(), finding.hint()));
            gather_project(context, &resolved.binding.manifest, refusal)
        })
        .or_else(|| {
            // Keep doctor's older leniency for a broken selected manifest: the catalog owns the
            // parse/resolution finding, so gathering failure must not turn it into no project.
            config::resolve_binding(None, context.environment, None)
                .map(|binding| gather_project(context, &binding.manifest, None))
        });

    Inputs {
        environment: context.environment,
        roots: &context.roots,
        project,
        ownership_ambiguity,
        online,
        runtime_root: config::resolve_runtime_root(context.environment, config::effective_uid())
            .ok(),
    }
}

fn gather_project<E: Environment>(
    context: &Context<'_, E>,
    manifest: &str,
    workspace_refusal: Option<(String, String)>,
) -> ProjectInputs {
    let artifact = config::resolve_artifact(
        &context.roots.config,
        config::ArtifactKind::Manifest,
        manifest,
    )
    .map_err(|error| error.to_string());

    let parsed = artifact
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|selected| {
            let source =
                std::fs::read_to_string(&selected.path).map_err(|error| error.to_string())?;
            let origin = config::ManifestOrigin {
                name: selected.name.clone(),
                path: selected.path.clone(),
                form: selected.form,
            };
            config::parse_manifest(&source, &origin).map_err(|error| error.to_string())
        });

    // The per-target lock in force, when this manifest has ever evaluated. The manifest name is
    // the sandbox key, and `doctor` remains read-only.
    let lock_path: Option<PathBuf> = artifact
        .as_ref()
        .ok()
        .map(|selected| selected.name.as_str())
        .map(|sandbox_id| {
            context
                .roots
                .data
                .join("projects")
                .join(sandbox_id)
                .join(super::DEFAULT_TARGET)
                .join("flake.lock")
        })
        .filter(|path| path.exists());

    ProjectInputs {
        manifest: manifest.to_owned(),
        artifact,
        parsed,
        workspace_refusal,
        lock_path,
    }
}
