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

    // The one stderr note spec/13 fixes: project probes skipped for want of a binding, said once,
    // with the way in.
    let notes = if inputs.project.is_none() {
        "project checks skipped: no manifest bound - run `viv init`\n".to_owned()
    } else {
        String::new()
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
    let project = config::registry::read(&context.roots.state)
        .ok()
        .and_then(|registry| {
            config::resolve_binding(None, context.environment, &registry, &context.project)
        })
        .map(|binding| gather_project(context, &binding.manifest));

    Inputs {
        environment: context.environment,
        roots: &context.roots,
        project,
        online,
        runtime_root: config::resolve_runtime_root(context.environment, config::effective_uid())
            .ok(),
    }
}

fn gather_project<E: Environment>(context: &Context<'_, E>, manifest: &str) -> ProjectInputs {
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

    // The per-target lock in force, when the project has ever evaluated: identity is read, never
    // minted — `doctor` changes nothing, and a project that never started has no lock to cover.
    let lock_path: Option<PathBuf> =
        config::resolve_identity(&context.roots.state, &context.project)
            .ok()
            .map(|project_id| {
                context
                    .roots
                    .data
                    .join("projects")
                    .join(project_id)
                    .join(super::DEFAULT_TARGET)
                    .join("flake.lock")
            })
            .filter(|path| path.exists());

    ProjectInputs {
        manifest: manifest.to_owned(),
        artifact,
        parsed,
        lock_path,
    }
}
