//! The command surface: what each verb resolves, what it prints, and what it costs when it fails.
//!
//! Three verbs do real work here — `init`, `config`, and the `manifest` readers — because slice
//! 011 is about turning a bound manifest into something evaluable, and those are the commands that
//! bind it and show what was bound. The rest parse and fail closed, which is not a placeholder: an
//! unbound project answering `78` and a malformed invocation answering `64` are contracts spec/14
//! already fixes, and they are true before the work behind the verb exists.
//!
//! Everything is a function of its inputs. The roots, the project directory, the environment, and
//! the stream facts all arrive as parameters, so the whole surface is exercisable without a
//! process — the seam `resolve_xdg_roots` established and every module since has kept.

pub mod grammar;
mod render;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::{
    self, ArtifactKind, Environment, GeneratedFlakeError, ManifestError, Registry, RegistryError,
    ResolutionError, ResolvedArtifact, XdgRoots,
};
use crate::diagnostic::{Diagnostic, DiagnosticId, Locus, Namespace};
use crate::exit::ExitKind;
use grammar::{Deferred, Invocation, Output, UsageError};

/// The only target a project has today (spec/15).
const DEFAULT_TARGET: &str = "default";

/// Everything a command reads about the host it was invoked on.
pub struct Context<'a, E: Environment> {
    /// The four durable roots, each already including vivarium's subtree.
    pub roots: XdgRoots,
    /// The project directory, canonical and symlink-resolved.
    pub project: PathBuf,
    /// The injected environment.
    pub environment: &'a E,
}

/// What a command produced.
pub struct Success {
    /// The result, for stdout. Empty when the command has nothing to say.
    pub stdout: String,
}

/// What a command cost when it did not succeed.
pub enum Failure {
    /// A malformed invocation. Carries no diagnostic id: spec/14 reserves no `cli.` namespace, and
    /// minting one would publish a second stable surface for something the usage line already says.
    Usage(UsageError),
    /// A vivarium-origin failure, in the skeleton spec/14 fixes.
    Diagnosed {
        diagnostic: Box<Diagnostic>,
        code: ExitKind,
    },
}

impl Failure {
    /// The code a caller reads from `$?`.
    #[must_use]
    pub const fn code(&self) -> ExitKind {
        match self {
            Self::Usage(_) => ExitKind::Usage,
            Self::Diagnosed { code, .. } => *code,
        }
    }

    /// Everything this failure writes to stderr, in the form the requested output selects.
    #[must_use]
    pub fn render(&self, output: Output) -> String {
        match self {
            Self::Usage(error) => {
                let mut rendered = format!("viv: {}\n", error.message);
                if let Some(usage) = error.usage {
                    let _ = writeln!(rendered, "usage: {usage}");
                }
                rendered
            }
            Self::Diagnosed { diagnostic, code } => {
                if output.is_json() {
                    format!("{}\n", diagnostic.to_json(code.code()))
                } else {
                    format!("{diagnostic}\n")
                }
            }
        }
    }
}

/// Runs one already-parsed invocation.
///
/// # Errors
///
/// Returns [`Failure`] carrying the code and the rendering for every path that does not succeed.
pub fn run<E: Environment>(
    invocation: &Invocation,
    context: &Context<'_, E>,
) -> Result<Success, Failure> {
    match invocation {
        Invocation::Init {
            manifest,
            write,
            yes,
            output,
            ..
        } => init(context, manifest.as_deref(), *write, *yes, *output),
        Invocation::Config { manifest, output } => {
            config_binding(context, manifest.as_deref(), *output)
        }
        Invocation::ManifestList { output } => manifest_list(context, *output),
        Invocation::ManifestShow { name, output } => manifest_show(context, name, *output),
        Invocation::Deferred { verb, .. } => deferred(context, *verb),
        // The caller performs the handoff, because it is the async half of this program and
        // nothing else here needs a runtime.
        Invocation::StartSpec { .. } => Ok(Success {
            stdout: String::new(),
        }),
    }
}

/// The binding assistant. Read-only unless `--write`, which is the whole of ADR-0011's P2.
fn init<E: Environment>(
    context: &Context<'_, E>,
    manifest_flag: Option<&str>,
    write: bool,
    yes: bool,
    output: Output,
) -> Result<Success, Failure> {
    let registry = read_registry(context)?;
    let resolved = config::resolve_binding(
        manifest_flag,
        context.environment,
        &registry,
        &context.project,
    );
    let Some(name) = manifest_flag
        .map(ToOwned::to_owned)
        .or_else(|| resolved.as_ref().map(|binding| binding.manifest.clone()))
    else {
        return Err(unbound(context));
    };

    // Resolved before anything is recorded, so `--write` cannot persist a binding to a manifest
    // that does not exist. spec/14 gives an unknown `--manifest` the same `78` a missing binding
    // gets, and writing first would turn that into a registry entry the next command rejects.
    let selected = resolve_manifest(context, &name)?;

    if !write {
        let snippet = Registry::snippet(&context.project, &name);
        if output.is_json() {
            return Ok(Success {
                stdout: render::init_preview_json(&name, &context.project, &selected.path),
            });
        }
        let registry_file = config::registry_path(&context.roots.state);
        let mut rendered = format!("manifest: {name}\npath: {}\n\n", selected.path.display());
        let _ = write!(
            rendered,
            "not yet bound. add this to `{}`:\n\n",
            registry_file.display()
        );
        rendered.push_str(&snippet);
        let _ = write!(rendered, "\nor run `viv init --manifest {name} --write`\n");
        return Ok(Success { stdout: rendered });
    }

    // The confirmation spec/01 requires is `--yes`'s job to skip. Off a terminal there is nobody to
    // prompt, and the grammar has already refused an unconfirmed destroy for that reason; here the
    // write is reversible by `viv unbind`, so the missing consent is a diagnostic rather than a
    // malformed invocation.
    if !yes {
        return Err(Failure::Diagnosed {
            diagnostic: Box::new(
                Diagnostic::new(
                    DiagnosticId::new(Namespace::State, "unconfirmed-write"),
                    "`viv init --write` needs confirmation",
                    Locus::Named("state registry"),
                    "recording a binding is an explicit act, never a side effect",
                )
                .with_hint(format!(
                    "re-run with `--yes`, or paste the snippet into `{}`",
                    config::registry_path(&context.roots.state).display()
                )),
            ),
            code: ExitKind::Usage,
        });
    }

    let project = context.project.clone();
    let bound = name.clone();
    config::registry::update(&context.roots.state, move |registry| {
        registry.bind(project, bound);
    })
    .map_err(|error| registry_failure(&error))?;

    if output.is_json() {
        return Ok(Success {
            stdout: render::init_written_json(&name, &context.project, &selected.path),
        });
    }
    Ok(Success {
        stdout: format!(
            "bound `{}` to manifest `{name}`\nrecorded in `{}`\n",
            context.project.display(),
            config::registry_path(&context.roots.state).display(),
        ),
    })
}

/// The binding record: what is in force, which source said so, and where everything lives.
fn config_binding<E: Environment>(
    context: &Context<'_, E>,
    manifest_flag: Option<&str>,
    output: Output,
) -> Result<Success, Failure> {
    let registry = read_registry(context)?;
    let Some(binding) = config::resolve_binding(
        manifest_flag,
        context.environment,
        &registry,
        &context.project,
    ) else {
        // Never an empty record. spec/01 is explicit: a `config` that rendered nulls would answer
        // "there is no binding" in a shape a consumer would have to inspect to distinguish from
        // "here is one", so it fails closed instead.
        return Err(unbound(context));
    };

    let selected = resolve_manifest(context, &binding.manifest)?;
    let project_id = config::sanitize_project_name(project_name(&context.project));
    let paths = config::target_paths(&context.roots, &project_id, DEFAULT_TARGET, &selected)
        .map_err(|error| flake_failure(&error))?;
    let lock = paths.select_lock().map_err(|error| flake_failure(&error))?;

    if output.is_json() {
        return Ok(Success {
            stdout: render::binding_json(&binding, &context.roots, &paths.directory, lock.path()),
        });
    }
    Ok(Success {
        stdout: render::binding_human(&binding, &context.roots, &paths.directory, lock.path()),
    })
}

/// Enumerates the manifest library, which is the config root's `manifests/` and nothing else.
fn manifest_list<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let library = context.roots.config.join("manifests");
    let mut names = Vec::new();
    // An absent library is zero rows and `0`, not a failure: spec/01 says so, and a user who has
    // not created one yet is in a normal state rather than a broken one.
    if let Ok(entries) = std::fs::read_dir(&library) {
        for entry in entries.flatten() {
            if let Some(name) = library_member(&entry) {
                names.push(name);
            }
        }
    }
    names.sort();
    names.dedup();

    let mut rows = Vec::new();
    for name in names {
        // A member that fails to resolve or parse is reported rather than skipped. Silently
        // dropping it would make a malformed manifest look like one the user never wrote.
        let selected = resolve_manifest(context, &name)?;
        let (_, manifest) = read_manifest(context, &selected)?;
        rows.push((name, selected, manifest));
    }

    Ok(Success {
        stdout: if output.is_json() {
            render::manifest_list_json(&rows)
        } else {
            render::manifest_list_human(&rows)
        },
    })
}

/// Shows one manifest as authored — its own declarations, not a merged result.
fn manifest_show<E: Environment>(
    context: &Context<'_, E>,
    name: &str,
    output: Output,
) -> Result<Success, Failure> {
    let selected = resolve_manifest(context, name)?;
    let (_, manifest) = read_manifest(context, &selected)?;
    Ok(Success {
        stdout: if output.is_json() {
            render::manifest_show_json(name, &selected, &manifest)
        } else {
            render::manifest_show_human(name, &selected, &manifest)
        },
    })
}

/// The verbs this slice parses but does not perform.
///
/// The fail-closed check still runs, because it is the part that is already true: a project with
/// no binding cannot start, exec, or list volumes whatever the implementation behind those verbs
/// turns out to be, and spec/14 commits to `78` for it today.
fn deferred<E: Environment>(context: &Context<'_, E>, verb: Deferred) -> Result<Success, Failure> {
    if verb.needs_binding() {
        let registry = read_registry(context)?;
        if config::resolve_binding(None, context.environment, &registry, &context.project).is_none()
        {
            return Err(unbound(context));
        }
    }
    Err(Failure::Diagnosed {
        diagnostic: Box::new(Diagnostic::new(
            DiagnosticId::new(Namespace::Internal, "not-implemented"),
            format!("`viv {}` is not implemented yet", verb.as_str()),
            Locus::Named("command surface"),
            "this verb belongs to a later slice; its grammar and fail-closed paths are settled",
        )),
        code: ExitKind::Software,
    })
}

fn read_registry<E: Environment>(context: &Context<'_, E>) -> Result<Registry, Failure> {
    config::registry::read(&context.roots.state).map_err(|error| registry_failure(&error))
}

/// The fail-closed answer, carrying the snippet that fixes it.
fn unbound<E: Environment>(context: &Context<'_, E>) -> Failure {
    let registry_file = config::registry_path(&context.roots.state);
    Failure::Diagnosed {
        diagnostic: Box::new(
            Diagnostic::new(
                DiagnosticId::new(Namespace::State, "no-manifest"),
                "no manifest is bound to this project",
                Locus::Named("state registry"),
                "nothing was given by `--manifest`, `VIVARIUM_MANIFEST`, or the project registry",
            )
            .with_hint(config::unbound_hint(&context.project, &registry_file)),
        ),
        code: ExitKind::Config,
    }
}

fn resolve_manifest<E: Environment>(
    context: &Context<'_, E>,
    name: &str,
) -> Result<ResolvedArtifact, Failure> {
    config::resolve_artifact(&context.roots.config, ArtifactKind::Manifest, name)
        .map_err(|error| resolution_failure(&error))
}

fn read_manifest<E: Environment>(
    context: &Context<'_, E>,
    selected: &ResolvedArtifact,
) -> Result<(String, config::Manifest), Failure> {
    let _ = context;
    let source = std::fs::read_to_string(&selected.path).map_err(|source| Failure::Diagnosed {
        diagnostic: Box::new(Diagnostic::new(
            DiagnosticId::new(Namespace::Manifest, "unreadable"),
            format!("could not read manifest `{}`", selected.name),
            Locus::File(selected.path.clone()),
            source.to_string(),
        )),
        code: if source.kind() == std::io::ErrorKind::PermissionDenied {
            ExitKind::NoPerm
        } else {
            ExitKind::IoErr
        },
    })?;
    let origin = config::ManifestOrigin {
        name: selected.name.clone(),
        path: selected.path.clone(),
        form: selected.form,
    };
    let manifest =
        config::parse_manifest(&source, &origin).map_err(|error| manifest_failure(&error))?;
    Ok((source, manifest))
}

/// Whether a directory entry is a member of a `.toml` library, by ADR-0045's two forms.
fn library_member(entry: &std::fs::DirEntry) -> Option<String> {
    let name = entry.file_name();
    let name = name.to_str()?;
    let file_type = entry.file_type().ok()?;
    if file_type.is_dir() {
        // The directory form is a member only when it actually holds the file, which is what keeps
        // a helper directory beside a manifest invisible to the listing.
        return entry
            .path()
            .join("default.toml")
            .is_file()
            .then(|| name.to_owned());
    }
    let stem = name.strip_suffix(".toml")?;
    Some(stem.to_owned())
}

/// The last component of the project path, which spec/15 sanitizes into the project id.
fn project_name(project: &Path) -> &str {
    project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
}

fn registry_failure(error: &RegistryError) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(error.diagnostic()),
        code: error.exit_code(),
    }
}

fn manifest_failure(error: &ManifestError) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(error.diagnostic()),
        code: error.exit_code(),
    }
}

fn flake_failure(error: &GeneratedFlakeError) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(error.diagnostic()),
        code: error.exit_code(),
    }
}

/// Renders a resolution failure, which has no diagnostic of its own yet.
///
/// [`ResolutionError`] predates the skeleton and carries only a message and a code. Rendering it
/// here rather than teaching it `diagnostic()` keeps that decision in one place until the resolver
/// needs the other slots; what it must not do is reach a user without an id, which is why the
/// condition is chosen from the variant rather than defaulted.
fn resolution_failure(error: &ResolutionError) -> Failure {
    let (condition, locus) = match error {
        ResolutionError::InvalidArtifactName { .. } => {
            ("invalid-name", Locus::Named("config root"))
        }
        ResolutionError::ArtifactNotFound { flat, .. } => ("not-found", Locus::File(flat.clone())),
        ResolutionError::ArtifactAmbiguous { flat, .. } => ("ambiguous", Locus::File(flat.clone())),
        ResolutionError::InspectArtifact { path, .. } => ("unreadable", Locus::File(path.clone())),
        _ => ("unresolvable", Locus::Named("config root")),
    };
    let code = error.exit_code();
    Failure::Diagnosed {
        diagnostic: Box::new(Diagnostic::new(
            DiagnosticId::new(Namespace::Manifest, condition),
            error.to_string(),
            locus,
            "resolution searches the config root and nothing else (N13, ADR-0061)",
        )),
        code,
    }
}
