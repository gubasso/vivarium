//! The command surface: what each verb resolves, what it prints, and what it costs when it fails.
//!
//! The verbs that do real work here are the ones slice 011 owns: `init` binds a project, the
//! `manifest` readers show the library as authored, `config` shows what is bound, and the two
//! `config` readers evaluate it. The rest parse and fail closed, which is not a placeholder: an
//! unbound project answering `78` and a malformed invocation answering `64` are contracts spec/14
//! already fixes, and they are true before the work behind the verb exists.
//!
//! Everything is a function of its inputs. The roots, the project directory, the environment, and
//! the stream facts all arrive as parameters, so the whole surface is exercisable without a
//! process — the seam `resolve_xdg_roots` established and every module since has kept.

pub mod grammar;
pub mod lifecycle;
mod render;
// The two verbs the process boundary dispatches itself; see the module's own note on why.
pub mod session;

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::config::{
    self, ArtifactKind, Environment, EvaluationError, GeneratedFlakeError, ManifestError, Registry,
    RegistryError, ResolutionError, ResolvedArtifact, XdgRoots,
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
    /// A note that accompanies a successful result, for stderr. Empty for most commands.
    ///
    /// Two things need it. `config sources` marks an equal-priority tie with `[tie]` and still
    /// exits `0`, and spec/01 puts that marker on stderr so `… --json 2>/dev/null | jq` stays
    /// clean. Both config readers announce the pin a first evaluation created, which spec/04
    /// requires be reported and neither envelope has a field for. A note is not a failure, so it
    /// cannot travel through [`Failure`], and it is not the result, so it must not travel through
    /// stdout.
    pub notes: String,
}

impl Success {
    /// A result with nothing to add on stderr, which is every command but one.
    const fn plain(stdout: String) -> Self {
        Self {
            stdout,
            notes: String::new(),
        }
    }
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
        Invocation::ConfigEval { output } => config_eval(context, *output),
        Invocation::ConfigSources { output } => config_sources(context, *output),
        Invocation::ManifestList { output } => manifest_list(context, *output),
        Invocation::ManifestShow { name, output } => manifest_show(context, name, *output),
        Invocation::Start {
            rebuild,
            no_rebuild,
            attach,
            ..
        } => lifecycle::start(context, *rebuild, *no_rebuild, *attach),
        Invocation::Status { global, output } => {
            let report = lifecycle::status(context, *global)?;
            Ok(Success::plain(if output.is_json() {
                render::status_json(&report)
            } else {
                render::status_human(&report)
            }))
        }
        Invocation::Stop {
            all,
            force,
            timeout,
            ..
        } => lifecycle::stop(context, *all, *force, *timeout),
        Invocation::Deferred { verb, .. } => deferred(context, *verb),
        // The caller performs all three, because they are the async half of this program and
        // nothing else here needs a runtime. A session additionally returns a code this signature
        // cannot express — the guest's own — and streams bytes rather than accumulating a string.
        Invocation::StartSpec { .. } | Invocation::Exec(_) | Invocation::Shell(_) => {
            Ok(Success::plain(String::new()))
        }
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
            return Ok(Success::plain(render::init_preview_json(
                &name,
                &context.project,
                &selected.path,
            )));
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
        return Ok(Success::plain(rendered));
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
        return Ok(Success::plain(render::init_written_json(
            &name,
            &context.project,
            &selected.path,
        )));
    }
    Ok(Success::plain(format!(
        "bound `{}` to manifest `{name}`\nrecorded in `{}`\n",
        context.project.display(),
        config::registry_path(&context.roots.state).display(),
    )))
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
    // The same resolved id the evaluating readers use: `viv config` prints the generated tree and
    // the lock in force, and both are keyed by `<project-id>` (spec/02). Sanitizing the basename
    // here would report the first `api`'s paths to a second project that holds `api-2`.
    let project_id = config::resolve_identity(&context.roots.state, &context.project)
        .map_err(|error| registry_failure(&error))?;
    let paths = config::target_paths(&context.roots, &project_id, DEFAULT_TARGET, &selected)
        .map_err(|error| flake_failure(&error))?;
    let lock = paths.select_lock().map_err(|error| flake_failure(&error))?;

    if output.is_json() {
        return Ok(Success::plain(render::binding_json(
            &binding,
            &context.roots,
            &paths.directory,
            lock.path(),
        )));
    }
    Ok(Success::plain(render::binding_human(
        &binding,
        &context.roots,
        &paths.directory,
        lock.path(),
    )))
}

/// The merged, evaluated configuration — what the layers produced.
fn config_eval<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let evaluated = evaluate_binding(context)?;
    // Never a partial render. A defect means the merge produced no answer, and printing the keys
    // that happened to survive would let a reader act on a configuration that does not exist.
    if let Some(failure) = defect_failure(&evaluated.analysis) {
        return Err(failure);
    }
    Ok(Success {
        stdout: if output.is_json() {
            render::config_eval_json(&evaluated.binding, &evaluated.manifest, &evaluated.analysis)
        } else {
            render::config_eval_human(&evaluated.analysis)
        },
        notes: evaluated.notes,
    })
}

/// Provenance and defects — which layer each value came from, and what collided.
///
/// Exits `0` whatever it finds. A content defect is data here rather than this command's own
/// failure, which is what keeps it usable at the one moment it is most needed: right after
/// `config eval` refused (ADR-0042).
fn config_sources<E: Environment>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let evaluated = evaluate_binding(context)?;
    Ok(Success {
        stdout: if output.is_json() {
            render::config_sources_json(
                &evaluated.binding,
                &evaluated.manifest,
                &evaluated.analysis,
            )
        } else {
            render::config_sources_human(&evaluated.analysis)
        },
        notes: evaluated.notes + &render::tie_notes(&evaluated.analysis),
    })
}

/// One bound manifest, prepared, evaluated, and read.
struct Evaluated {
    binding: config::ResolvedBinding,
    manifest: config::Manifest,
    /// What the evaluation has to say on stderr before the result reaches stdout.
    notes: String,
    analysis: config::merged::Analysis,
    /// The generated tree this evaluation prepared, which is also what a launch builds from.
    flake_directory: PathBuf,
}

/// The path both config readers travel, up to the point where they disagree.
///
/// Identical for the two by construction: they must not be able to reach different answers about
/// the same tree, and the only way to guarantee that is for one function to produce both.
fn evaluate_binding<E: Environment>(context: &Context<'_, E>) -> Result<Evaluated, Failure> {
    let resolved = resolve_manifest_for_launch(context)?;
    // Resolved, never minted: these are the read-only readers, and spec/14's read-only guarantee
    // is what stops them writing a marker. Resolved rather than re-sanitized because the generated
    // tree and its lock are keyed by `<project-id>` (spec/02), and a second project named `api`
    // holds `api-2` — sanitizing its basename would point it at the first project's tree.
    let project_id = config::resolve_identity(&context.roots.state, &context.project)
        .map_err(|error| registry_failure(&error))?;
    evaluate_resolved(context, &project_id, resolved)
}

/// The evaluating half, against a binding, manifest, and identity already resolved.
fn evaluate_resolved<E: Environment>(
    context: &Context<'_, E>,
    project_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<Evaluated, Failure> {
    let ResolvedForLaunch {
        binding,
        selected,
        source,
        manifest,
        resources: _,
    } = resolved;
    let prepared = config::prepare_generated_flake(
        &context.roots,
        project_id,
        DEFAULT_TARGET,
        &selected,
        &source,
        &manifest,
        &config::BaselineInputs::from_environment(context.environment),
    )
    .map_err(|error| flake_failure(&error))?;
    let report = config::evaluate::report(&prepared).map_err(|error| evaluation_failure(&error))?;
    // Only now, and only when this target had no pin: the lock is created by the first successful
    // evaluation and thereafter moves only under `viv update` (spec/04, ADR-0059).
    let created = prepared.effective_lock.may_persist_created();
    let lock =
        config::evaluate::persist_first_pin(&prepared).map_err(|error| flake_failure(&error))?;
    // Announced, because spec/04 requires the created pin to be reported and a read-only command
    // that quietly wrote one would be the surprise ADR-0011 exists to prevent. On stderr, since
    // the pin is not the result the command was asked for. A first evaluation that then refuses a
    // defect loses the note rather than the fact: `viv config` reports the lock in force at any
    // time, so nothing here is the only chance to learn it.
    let notes = if created {
        format!(
            "created the pin for this target: {}\n",
            lock.path().display()
        )
    } else {
        String::new()
    };
    Ok(Evaluated {
        binding,
        manifest,
        notes,
        analysis: config::merged::analyze(&report),
        flake_directory: prepared.directory,
    })
}

/// The first content defect, rendered as the `65` spec/14 fixes for it.
///
/// First rather than all, matching how `doctor` reports the first failing hard check: the four-slot
/// skeleton names one condition, and a list of them would be a report rather than a diagnostic.
/// `config sources` is where the complete set is read, and the hint says so.
fn defect_failure(analysis: &config::merged::Analysis) -> Option<Failure> {
    use config::merged::ConflictKind;

    let diagnostic = if let Some(conflict) = analysis.conflicts.first() {
        let layers = conflict.layers.join(", ");
        match conflict.kind {
            ConflictKind::Tie => Diagnostic::new(
                DiagnosticId::new(Namespace::Merge, "equal-priority-tie"),
                format!(
                    "`{}` has two definitions at the same priority",
                    conflict.key
                ),
                Locus::Named("merged configuration"),
                format!("{layers} each set it, and priority never breaks a tie"),
            )
            .with_hint(concat!(
                "a shared piece should propose with mkDefault so your manifest can decide; ",
                "failing that, drop one piece or override through extends",
            )),
            ConflictKind::LiteralPath => Diagnostic::new(
                DiagnosticId::new(Namespace::Merge, "literal-path"),
                format!(
                    "a shared layer carries a literal personal path in `{}`",
                    conflict.key
                ),
                Locus::Named("merged configuration"),
                format!("{layers} declared {}", conflict.evidence.join(", ")),
            )
            .with_hint(concat!(
                "shared config is personal-data-free (N11): use `${HOME}` or an XDG name, ",
                "which stay unexpanded until launch, or move the mount to your own manifest",
            )),
        }
    } else {
        let irreconcilable = analysis.irreconcilable.first()?;
        Diagnostic::new(
            DiagnosticId::new(Namespace::Merge, "irreconcilable"),
            irreconcilable.what.clone(),
            Locus::Named("merged configuration"),
            irreconcilable.why.clone(),
        )
        .with_hint("run `viv config sources` to see every layer that contributed")
    };
    Some(Failure::Diagnosed {
        diagnostic: Box::new(diagnostic),
        code: ExitKind::DataErr,
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

    Ok(Success::plain(if output.is_json() {
        render::manifest_list_json(&rows)
    } else {
        render::manifest_list_human(&rows)
    }))
}

/// Shows one manifest as authored — its own declarations, not a merged result.
fn manifest_show<E: Environment>(
    context: &Context<'_, E>,
    name: &str,
    output: Output,
) -> Result<Success, Failure> {
    let selected = resolve_manifest(context, name)?;
    let (_, manifest) = read_manifest(context, &selected)?;
    Ok(Success::plain(if output.is_json() {
        render::manifest_show_json(name, &selected, &manifest)
    } else {
        render::manifest_show_human(name, &selected, &manifest)
    }))
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
pub(super) fn unbound<E: Environment>(context: &Context<'_, E>) -> Failure {
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

/// One diagnosed failure in the four-slot skeleton spec/14 fixes.
///
/// The lifecycle verbs mint several of these, and every id has to come from a namespace spec/14
/// reserves — minting one outside the reserved set would publish a stable surface the spec does
/// not admit. Taking the namespace as an argument is what keeps that visible at each call site.
pub(super) fn diagnosed(
    namespace: Namespace,
    id: &'static str,
    what: impl Into<String>,
    where_: Locus,
    why: impl Into<String>,
    code: ExitKind,
) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(Diagnostic::new(
            DiagnosticId::new(namespace, id),
            what,
            where_,
            why,
        )),
        code,
    }
}

impl Failure {
    /// Adds the remedy line to an already-diagnosed failure.
    pub(super) fn with_hint(self, hint: impl Into<String>) -> Self {
        match self {
            Self::Diagnosed { diagnostic, code } => Self::Diagnosed {
                diagnostic: Box::new(diagnostic.with_hint(hint)),
                code,
            },
            usage @ Self::Usage(_) => usage,
        }
    }
}

/// The bound manifest, resolved and read — spec/10 step 1, and nothing beyond it.
///
/// Separate from the evaluation below because `start` needs it strictly earlier than the build:
/// the step order makes an unbound project answer `78` before the host is preflighted, and the
/// `[resources]` table is a launch-channel input `--no-rebuild` still needs on a path that
/// evaluates nothing (spec/17, N19).
pub(super) struct ResolvedForLaunch {
    binding: config::ResolvedBinding,
    selected: ResolvedArtifact,
    source: String,
    manifest: config::Manifest,
    pub(super) resources: Option<config::Resources>,
}

/// What a launch needs from the resolution front half: a prepared, evaluated generated tree, and
/// the launch-channel values the merge produced.
pub(super) struct LaunchInputs {
    pub(super) flake_directory: PathBuf,
    /// The merged `resources`, which spec/04 makes authoritative over the manifest's own table: a
    /// piece proposing `vivarium.resources` compiles into the same option, so reading the leaf
    /// would bypass the module system's answer.
    pub(super) resources: Option<config::Resources>,
}

/// spec/10 step 1: resolve the binding and read the manifest it names.
pub(super) fn resolve_manifest_for_launch<E: Environment>(
    context: &Context<'_, E>,
) -> Result<ResolvedForLaunch, Failure> {
    let registry = read_registry(context)?;
    let Some(binding) =
        config::resolve_binding(None, context.environment, &registry, &context.project)
    else {
        return Err(unbound(context));
    };
    let selected = resolve_manifest(context, &binding.manifest)?;
    let (source, manifest) = read_manifest(context, &selected)?;
    Ok(ResolvedForLaunch {
        binding,
        selected,
        source,
        resources: manifest.resources,
        manifest,
    })
}

/// The evaluate-refuse-defects half `start` shares with the config readers.
///
/// It reaches the same generated tree by the same route, deliberately: a `start` that built from a
/// tree the readers never saw would make `viv config eval` a report about something else.
pub(super) fn evaluate_resolved_for_launch<E: Environment>(
    context: &Context<'_, E>,
    project_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<LaunchInputs, Failure> {
    let evaluated = evaluate_resolved(context, project_id, resolved)?;
    // A content defect means the merge produced no answer, so there is nothing to build.
    if let Some(failure) = defect_failure(&evaluated.analysis) {
        return Err(failure);
    }
    Ok(LaunchInputs {
        flake_directory: evaluated.flake_directory,
        resources: merged_resources(&evaluated.analysis),
    })
}

/// The launch-channel `resources` the merge produced, in the shape the manifest declares them.
///
/// Read from the analysis rather than from the manifest because that is what spec/04 specifies and
/// what `viv config eval` reports: a launch that used the leaf would boot a VM the tool's own
/// reader says it did not build. Absent keys stay absent, so per-knob host auto-sizing still
/// applies to whichever one no layer declared.
fn merged_resources(analysis: &config::merged::Analysis) -> Option<config::Resources> {
    let read = |key: &str| {
        analysis
            .effective(key)
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    let resources = config::Resources {
        mem_mib: read("resources.mem_mib"),
        vcpu: read("resources.vcpu"),
    };
    (resources.mem_mib.is_some() || resources.vcpu.is_some()).then_some(resources)
}

pub(super) fn registry_failure(error: &RegistryError) -> Failure {
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

pub(super) fn evaluation_failure(error: &EvaluationError) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(error.diagnostic()),
        code: error.exit_code(),
    }
}

pub(super) fn flake_failure(error: &GeneratedFlakeError) -> Failure {
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
pub(super) fn resolution_failure(error: &ResolutionError) -> Failure {
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
