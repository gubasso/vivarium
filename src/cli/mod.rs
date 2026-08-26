//! The command surface: what each verb resolves, what it prints, and what it costs when it fails.
//!
//! The manifest readers show the library as authored, `config` shows the uniquely selected
//! manifest, and the two config readers evaluate it. Resolution derives ownership from explicit
//! workspaces after the flag and environment overrides; malformed invocation grammar is decided
//! before that filesystem work.
//!
//! Everything is a function of its inputs. The roots, the project directory, the environment, and
//! the stream facts all arrive as parameters, so the whole surface is exercisable without a
//! process — the seam `resolve_xdg_roots` established and every module since has kept.

mod attach;
mod destroy;
pub mod doctor;
mod fleet;
mod generations;
pub mod grammar;
pub mod lifecycle;
mod prompt;
mod render;
// The two verbs the process boundary dispatches itself; see the module's own note on why.
pub mod session;
mod update;
mod volume;

pub use attach::start_attached;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::{
    self, ArtifactKind, Environment, EvaluationError, GeneratedFlakeError, ManifestError,
    RegistryError, ResolutionError, ResolvedArtifact, XdgRoots,
};
use crate::diagnostic::{Diagnostic, DiagnosticId, Locus, Namespace};
use crate::exit::ExitKind;
use crate::ui::Ui;
use crate::ui::style::Palette;
use grammar::{Invocation, Output, UsageError};

/// The only target a project has today (spec/15).
const DEFAULT_TARGET: &str = "default";
const MAX_SANDBOX_NAME_BYTES: usize = 48;

/// Everything a command reads about the host it was invoked on, plus the face it speaks through.
///
/// The face is itself resolved purely from the same injected inputs ([`Ui::resolve`]), so the
/// whole surface stays exercisable without a process.
pub struct Context<'a, E: Environment> {
    /// The four durable roots, each already including vivarium's subtree.
    pub roots: XdgRoots,
    /// The project directory, canonical and symlink-resolved.
    pub project: PathBuf,
    /// The injected environment.
    pub environment: &'a E,
    /// The stderr face for this invocation. Silent unless the host said otherwise.
    pub ui: &'a Ui,
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
///
/// `Debug` so a test can `unwrap` a `Result<_, Failure>`. It is not the user-facing rendering —
/// that is [`Failure::render`], which spec/14 fixes the shape of.
#[derive(Debug)]
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
    ///
    /// The palette reaches only the human arms. The JSON arm never receives it structurally,
    /// which is how "never color JSON" (spec/01) holds without discipline.
    #[must_use]
    pub fn render(&self, output: Output, palette: &Palette) -> String {
        match self {
            Self::Usage(error) => {
                let mut rendered =
                    format!("{} {}\n", palette.error.apply_to("viv:"), error.message);
                if let Some(usage) = error.usage {
                    let _ = writeln!(rendered, "{} {usage}", palette.label.apply_to("usage:"));
                }
                rendered
            }
            Self::Diagnosed { diagnostic, code } => {
                if output.is_json() {
                    format!("{}\n", diagnostic.to_json(code.code()))
                } else {
                    format!("{}\n", diagnostic.render(palette))
                }
            }
        }
    }
}

/// `viv --help`'s text, for `main` to render ahead of any context: help must answer on a host
/// where nothing else — a missing `HOME`, an unreadable manifest library — does.
#[must_use]
pub fn help_text(verb: Option<&str>, palette: &crate::ui::style::Palette) -> String {
    render::help_human(palette, verb)
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
            generation,
            attach: false,
            ..
        } => lifecycle::start(context, *rebuild, *no_rebuild, *generation),
        // The attached form is dispatched on `main`'s async arm before this synchronous surface;
        // reaching it here would silently perform the detached operation under the other flag.
        Invocation::Start { attach: true, .. } => Err(diagnosed(
            Namespace::Internal,
            "misdispatched",
            "`viv start --attach` reached the synchronous dispatch surface",
            Locus::Named("command surface"),
            "the attached form returns when the console stream ends, which only the async \
            dispatch arm can express",
            ExitKind::Software,
        )),
        Invocation::VolumeList { output } => volume::list(context, *output),
        Invocation::VolumePrune {
            dry_run,
            yes,
            output,
        } => volume::prune(context, *dry_run, *yes, *output),
        Invocation::GenerationsList { output } => generations::list(context, *output),
        Invocation::GenerationsPrune {
            keep,
            older_than_seconds,
            output,
        } => generations::prune(context, *keep, *older_than_seconds, *output),
        Invocation::GenerationsActivate { number, output } => {
            generations::activate(context, Some(*number), *output)
        }
        Invocation::GenerationsRollback { output } => generations::activate(context, None, *output),
        Invocation::Gc { output } => generations::gc(context, *output),
        Invocation::Update { inputs, output } => update::run(context, inputs, *output),
        // The caller performs all eight. The first six are the async half of this program and
        // nothing else here needs a runtime; a session additionally returns a code this signature
        // cannot express — the guest's own — and streams bytes rather than accumulating a string.
        // `status` asks a live agent for its session count, and `stop` and `destroy` walk a
        // ladder whose first rung asks the agent for a guest shutdown, so all three are
        // dispatched through [`status`], [`stop`], and [`destroy`]. Help and version are the
        // opposite edge: they must answer on a host where nothing else does, so `main` renders
        // them before this module's `Context` can fail to build. Doctor returns a code this
        // signature cannot express — a report at `69` is still a report — so `main` dispatches
        // it through [`doctor::command`].
        Invocation::StartSpec { .. }
        | Invocation::Exec(_)
        | Invocation::Shell(_)
        | Invocation::Status { .. }
        | Invocation::Stop { .. }
        | Invocation::Destroy { .. }
        | Invocation::Help { .. }
        | Invocation::Version
        | Invocation::Doctor { .. } => Ok(Success::plain(String::new())),
    }
}

/// `viv status`, both faces: the project-local report and the `-g` fleet enumeration.
///
/// Async because a report on a running VM asks the guest agent for its live session count
/// (spec/12), and dispatched from `main` beside the session verbs for the same reason they are.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), an unusable runtime root, or — under
/// `-g` — an unreadable manifest library or index (`74`).
pub async fn status<E: Environment + Sync>(
    context: &Context<'_, E>,
    global: bool,
    output: Output,
) -> Result<Success, Failure> {
    if global {
        return fleet::report(context, output).await;
    }
    let report = lifecycle::status(context).await?;
    Ok(Success::plain(if output.is_json() {
        render::status_json(&report)
    } else {
        render::status_human(&report, context.ui.palette_out())
    }))
}

/// `viv stop`, both faces: the project-local ladder and the `--all` sweep.
///
/// Async because the ladder's first rung asks the guest agent for an orderly shutdown over the
/// control socket (spec/10, spec/12), and dispatched from `main` beside `status` for the same
/// reason.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), a stop that cannot be confirmed (`69`),
/// or — under `--all` — an unreadable manifest library or index (`74`, `78`) and, after the
/// whole sweep ran, its first per-sandbox failure.
pub async fn stop<E: Environment + Sync>(
    context: &Context<'_, E>,
    all: bool,
    force: bool,
    timeout: Option<i64>,
    output: Output,
) -> Result<Success, Failure> {
    if all {
        return lifecycle::stop_all(context, force, timeout, output).await;
    }
    lifecycle::stop(context, force, timeout, output).await
}

/// `viv destroy` — async for the same reason [`stop`] is: the teardown begins with the ladder.
///
/// # Errors
///
/// Returns [`Failure`] for an unbound project (`78`), a stop that cannot be confirmed (`69`),
/// or a removal the filesystem refused (`74`, `77`).
pub async fn destroy<E: Environment + Sync>(
    context: &Context<'_, E>,
    keep_volumes: bool,
    yes: bool,
    output: Output,
) -> Result<Success, Failure> {
    destroy::destroy(context, keep_volumes, yes, output).await
}

/// The binding record: what is in force, which source said so, and where everything lives.
fn config_binding<E: Environment>(
    context: &Context<'_, E>,
    manifest_flag: Option<&str>,
    output: Output,
) -> Result<Success, Failure> {
    let resolved = resolve_manifest_and_workspace(context, manifest_flag, false)
        .map_err(ResolveFailure::into_failure)?;
    let binding = &resolved.binding;
    let selected = &resolved.selected;
    let sandbox_id = selected.name.clone();
    let paths = config::target_paths(&context.roots, &sandbox_id, DEFAULT_TARGET, selected)
        .map_err(|error| flake_failure(&error))?;
    let lock = paths.select_lock().map_err(|error| flake_failure(&error))?;

    if output.is_json() {
        return Ok(Success::plain(render::binding_json(
            binding,
            &context.roots,
            &paths.directory,
            lock.path(),
        )));
    }
    Ok(Success::plain(render::binding_human(
        binding,
        &context.roots,
        &paths.directory,
        lock.path(),
        context.ui.palette_out(),
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
    /// The lock in force for this evaluation, as bytes rather than a path (ADR-0059).
    ///
    /// Read from the staged copy the moment this process published the tree, deliberately: the
    /// durable lock can move while a build runs, and the generated tree's pathname is a shared
    /// cache another manifest-reading command may republish (ADR-0058) — a path read later can
    /// name either. Bytes captured at evaluation time are the definition of what a generation
    /// retains, whatever happens beside it afterwards.
    lock_snapshot: Vec<u8>,
}

/// The path both config readers travel, up to the point where they disagree.
///
/// Identical for the two by construction: they must not be able to reach different answers about
/// the same tree, and the only way to guarantee that is for one function to produce both.
fn evaluate_binding<E: Environment>(context: &Context<'_, E>) -> Result<Evaluated, Failure> {
    let resolved = resolve_manifest_for_launch(context)?;
    let sandbox_id = resolved.selected.name.clone();
    evaluate_resolved(context, &sandbox_id, resolved)
}

/// The evaluating half, against a binding and manifest already resolved.
fn evaluate_resolved<E: Environment>(
    context: &Context<'_, E>,
    sandbox_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<Evaluated, Failure> {
    let ResolvedForLaunch {
        binding,
        selected,
        source,
        manifest,
        workspace_paths,
        resources: _,
        ..
    } = resolved;
    let prepared = config::prepare_generated_flake(
        &context.roots,
        sandbox_id,
        DEFAULT_TARGET,
        &selected,
        &source,
        &manifest,
        &workspace_paths,
        &config::BaselineInputs::from_environment(context.environment),
    )
    .map_err(|error| flake_failure(&error))?;
    // The one sanctioned lock rewrite, announced for the same reason the created pin below is: no
    // ordinary build moves a pin (ADR-0059), so the migration ADR-0102 sanctions must never
    // happen silently. Directly on stderr rather than through `notes`, deliberately: the shed
    // happens exactly once per retained lock, this seam is shared by verbs whose surface has no
    // notes channel (a session's cold start), and the rewrite is already durable by this line —
    // so the announcement comes before evaluation or any other fallible step can suppress it.
    if prepared.shed_vivarium {
        context.ui.warn(&format!(
            "shed the dead `vivarium` node from this target's lock: {} (no other pin moved)",
            prepared.effective_lock.path().display()
        ));
    }
    // Enforcement of the composed rule over the lock in force, before Nix runs against it:
    // a split baseline is legal Nix and a guest that fails at boot, so nothing downstream
    // would name it (ADR-0112). A first evaluation has no lock yet; its created pin meets
    // the same check inside `persist_created_lock` before anything durable exists.
    if !prepared.effective_lock.may_persist_created() {
        let staged = read_staged_lock(&prepared.directory)?;
        config::composed_lock_failure(&staged).map_err(|error| flake_failure(&error))?;
    }
    let report = config::evaluate::report(&prepared, context.ui)
        .map_err(|error| evaluation_failure(&error))?;
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
    let mut analysis = config::merged::analyze(&report);
    // The one key the report cannot carry: since ADR-0110 a workspace compiles to a mount and Nix
    // never learns it was one, so the reader's view of ownership is put back here.
    analysis.with_manifest_workspaces(&manifest, &selected.name);
    Ok(Evaluated {
        binding,
        manifest,
        notes,
        analysis,
        lock_snapshot: read_staged_lock(&prepared.directory)?,
        flake_directory: prepared.directory,
    })
}

/// The staged lock's bytes, read the moment this process published the tree it evaluated.
///
/// Present in every case by this point: an existing or override lock was staged into the tree,
/// and a first evaluation created one there before `persist_first_pin` copied it out.
fn read_staged_lock(flake_directory: &Path) -> Result<Vec<u8>, Failure> {
    let path = flake_directory.join("flake.lock");
    std::fs::read(&path).map_err(|source| {
        diagnosed(
            Namespace::State,
            "generation-lock-snapshot",
            "could not read the lock this evaluation was pinned by",
            Locus::File(path),
            source.to_string(),
            ExitKind::IoErr,
        )
    })
}

/// The first content defect, rendered as the `65` spec/14 fixes for it.
///
/// First rather than all, matching how `doctor` reports the first failing hard check: the four-slot
/// skeleton names one condition, and a list of them would be a report rather than a diagnostic.
/// `config sources` is where the complete set is read, and the hint says so.
fn defect_failure(analysis: &config::merged::Analysis) -> Option<Failure> {
    use config::merged::ConflictKind;

    let diagnostic =
        if let Some(conflict) = analysis.conflicts.first() {
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
            ConflictKind::SessionPath => Diagnostic::new(
                DiagnosticId::new(Namespace::Merge, "session-path"),
                format!("a mount in `{}` takes a host session directory", conflict.key),
                Locus::Named("merged configuration"),
                format!("{layers} declared {}", conflict.evidence.join(", ")),
            )
            .with_hint(concat!(
                "`/tmp`, `/var/tmp`, and `${XDG_RUNTIME_DIR}` hold live session state no share ",
                "may carry (N24), in every layer; a source only expansion reveals is refused at ",
                "launch with `78`",
            )),
            ConflictKind::NonPortableVariable => Diagnostic::new(
                DiagnosticId::new(Namespace::Merge, "non-portable-variable"),
                format!(
                    "a shared layer's mount in `{}` names a variable outside the portable set",
                    conflict.key
                ),
                Locus::Named("merged configuration"),
                format!("{layers} declared {}", conflict.evidence.join(", ")),
            )
            .with_hint(concat!(
                "a shared layer references the host only through `${HOME}` and the four durable ",
                "XDG directories (spec/07); a private variable belongs in your own manifest",
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
        render::manifest_list_human(&rows, context.ui.palette_out())
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

/// The fail-closed answer when no override or uniquely declaring manifest applies.
pub(super) fn unbound<E: Environment>(context: &Context<'_, E>) -> Failure {
    Failure::Diagnosed {
        diagnostic: Box::new(
            Diagnostic::new(
                DiagnosticId::new(Namespace::State, "no-manifest"),
                "no manifest owns the working directory",
                Locus::File(context.roots.config.join("manifests")),
                concat!(
                    "nothing was given by `--manifest` or `VIVARIUM_MANIFEST`, and no manifest ",
                    "declares this directory in `[[workspaces]]`"
                ),
            )
            .with_hint(config::unbound_hint(&context.project)),
        ),
        code: ExitKind::Config,
    }
}

/// A stable snapshot of every uniquely resolvable manifest-library member.
fn manifest_stamps<E: Environment>(
    context: &Context<'_, E>,
) -> Result<Vec<config::ManifestStamp>, Failure> {
    let library = context.roots.config.join("manifests");
    let entries = match std::fs::read_dir(&library) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(diagnosed(
                Namespace::Manifest,
                "library-unreadable",
                "could not enumerate the manifest library",
                Locus::File(library),
                source.to_string(),
                ExitKind::IoErr,
            ));
        }
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| library_member(&entry))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    let mut stamps = Vec::new();
    for name in names {
        // An ambiguous or concurrently removed unrelated member cannot own this invocation. An
        // explicitly selected name still fails through `resolve_manifest`; the derived scan skips
        // it just as it skips unrelated broken text.
        let Ok(selected) =
            config::resolve_artifact(&context.roots.config, ArtifactKind::Manifest, &name)
        else {
            continue;
        };
        // `None` is the same concurrently-removed member the comment above describes, reaching
        // this call instead of the one before it because the window spans both.
        if let Some(stamp) = config::ManifestStamp::read(name, selected.path)
            .map_err(|error| registry_failure(&error))?
        {
            stamps.push(stamp);
        }
    }
    Ok(stamps)
}

/// Reads or rebuilds the cache-root owner index from explicit workspace declarations.
fn workspace_index<E: Environment>(
    context: &Context<'_, E>,
    publish: bool,
) -> Result<config::WorkspaceIndex, Failure> {
    let stamps = manifest_stamps(context)?;
    let derive = |stamp: &config::ManifestStamp| {
        let selected = resolve_manifest(context, &stamp.name).ok()?;
        let (_, manifest) = read_manifest(context, &selected).ok()?;
        Some(
            manifest
                .workspaces
                .into_iter()
                .map(|workspace| workspace.source)
                .collect(),
        )
    };
    if publish {
        config::registry::load_or_rebuild(&context.roots.cache, stamps, derive)
            .map_err(|error| registry_failure(&error))
    } else {
        Ok(config::WorkspaceIndex::derive(stamps, derive))
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
    /// The declared workspace containing the exact invoking directory.
    pub(super) matched_workspace: Option<PathBuf>,
    /// The manifest's declared workspace set, expanded and canonical, in declaration order.
    ///
    /// One list, derived once, reaches the reuse comparison and the generated flake. Empty only
    /// when a refusal is being carried rather than raised.
    pub(super) workspace_paths: Vec<PathBuf>,
    /// Present only for doctor's non-refusing view: every verb but `doctor` has already refused.
    pub(super) workspace_refusal: Option<WorkspaceFinding>,
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
    /// Every volume the merge declared, with the layer that declared it.
    ///
    /// Carried out of the one command that evaluates so the read-only volume verbs never have to.
    /// See `config::volumes` for why that split exists rather than each verb asking Nix.
    pub(super) volumes: Vec<config::volumes::DeclaredVolume>,
    /// The lock in force for this evaluation, which the appended generation retains (ADR-0059).
    pub(super) lock_snapshot: Vec<u8>,
}

/// Why the one resolution routine could not answer.
///
/// Two variants rather than a bare `Failure` so the second consumer can tell the ADR-0109
/// ambiguity apart from every ordinary reason resolution fails. Refusing consumers collapse it
/// immediately; only doctor looks inside.
pub(super) enum ResolveFailure {
    Ambiguous(AmbiguousWorkspaceOwner),
    Other(Box<Failure>),
}

impl ResolveFailure {
    fn into_failure(self) -> Failure {
        match self {
            Self::Ambiguous(finding) => finding.failure(),
            Self::Other(failure) => *failure,
        }
    }
}

impl From<Failure> for ResolveFailure {
    fn from(failure: Failure) -> Self {
        Self::Other(Box::new(failure))
    }
}

/// spec/10 step 1: resolve the binding and read the manifest it names.
pub(super) fn resolve_manifest_for_launch<E: Environment>(
    context: &Context<'_, E>,
) -> Result<ResolvedForLaunch, Failure> {
    resolve_manifest_and_workspace(context, None, false).map_err(ResolveFailure::into_failure)
}

/// Doctor's view: never refuses, and keeps the ADR-0109 findings as values it can report.
pub(super) fn resolve_manifest_for_doctor<E: Environment>(
    context: &Context<'_, E>,
) -> Result<ResolvedForLaunch, ResolveFailure> {
    resolve_manifest_and_workspace(context, None, true)
}

/// The one ADR-0109 resolution path: overrides, the derived index, and workspace ownership.
fn resolve_manifest_and_workspace<E: Environment>(
    context: &Context<'_, E>,
    manifest_flag: Option<&str>,
    allow_undeclared: bool,
) -> Result<ResolvedForLaunch, ResolveFailure> {
    let override_binding = config::resolve_binding(manifest_flag, context.environment, None);

    let (binding, selected, source, manifest) = if let Some(binding) = override_binding {
        let selected = resolve_manifest(context, &binding.manifest)?;
        let (source, manifest) = read_manifest(context, &selected)?;
        (binding, selected, source, manifest)
    } else {
        // `doctor` uses this same judgment but remains a pure checker, so it derives in memory
        // rather than publishing the rebuildable cache.
        let index = workspace_index(context, !allow_undeclared)?;
        let mut matches = Vec::new();
        for indexed in index.manifests() {
            if containing_workspace(
                &context.project,
                expanded_workspace_sources(context, &indexed.workspace_sources).iter(),
            )
            .is_none()
            {
                continue;
            }
            let Ok(selected) = resolve_manifest(context, &indexed.name) else {
                continue;
            };
            let Ok((source, manifest)) = read_manifest(context, &selected) else {
                continue;
            };
            // The cache narrows candidates; current manifest text remains the authority. Lenient
            // on purpose, and it is the strictness asymmetry this routine turns on: an unset
            // variable or a missing tree in someone else's manifest must not decide whether this
            // invocation resolves, or one bad file in the library would take the whole tool
            // hostage. The selected manifest is held to the full rule below.
            let sources: Vec<String> = manifest
                .workspaces
                .iter()
                .map(|workspace| workspace.source.clone())
                .collect();
            let owns = containing_workspace(
                &context.project,
                expanded_workspace_sources(context, &sources).iter(),
            )
            .is_some();
            if owns {
                matches.push((selected, source, manifest));
            }
        }
        if matches.len() > 1 {
            return Err(ResolveFailure::Ambiguous(ambiguous_workspace_owner(
                &context.project,
                matches.iter().map(|(selected, _, _)| selected),
            )));
        }
        let Some((selected, source, manifest)) = matches.pop() else {
            return Err(unbound(context).into());
        };
        let binding =
            config::resolve_binding(None, context.environment, Some(selected.name.as_str()))
                .ok_or_else(|| unbound(context))?;
        (binding, selected, source, manifest)
    };

    validate_sandbox_name(&selected)?;

    // The selected manifest, held to the whole rule. Since ADR-0110 this is where a workspace
    // defect is decided: the paths it produces are written into the generated flake as both the
    // share source and its guest target, so a defect that survived to evaluation would reach the
    // build rather than the user.
    let lookup = |name: &str| {
        context
            .environment
            .dynamic_variable(name)
            .map(|value| value.to_string_lossy().into_owned())
    };
    let sources: Vec<String> = manifest
        .workspaces
        .iter()
        .map(|workspace| workspace.source.clone())
        .collect();
    let (workspace_paths, defect) = match crate::launch::workspace::resolve(&sources, &lookup) {
        Ok(paths) => (paths, None),
        Err(defect) => (Vec::new(), Some(WorkspaceFinding::Defect(defect))),
    };
    let matched_workspace = containing_workspace(&context.project, workspace_paths.iter());
    let workspace_refusal = defect.or_else(|| {
        matched_workspace.is_none().then(|| {
            WorkspaceFinding::Undeclared(undeclared_workspace(&selected, &context.project))
        })
    });
    if !allow_undeclared && let Some(refusal) = &workspace_refusal {
        return Err(refusal.failure().into());
    }
    Ok(ResolvedForLaunch {
        binding,
        selected,
        source,
        matched_workspace,
        workspace_paths,
        workspace_refusal,
        resources: manifest.resources,
        manifest,
    })
}

fn validate_sandbox_name(selected: &ResolvedArtifact) -> Result<(), Failure> {
    if selected.name.len() <= MAX_SANDBOX_NAME_BYTES {
        return Ok(());
    }
    Err(diagnosed(
        Namespace::Manifest,
        "name-too-long",
        "the selected manifest name is too long to key a sandbox",
        Locus::File(selected.path.clone()),
        format!(
            concat!(
                "manifest name `{}` is {} bytes; sandbox names may be at most ",
                "{} bytes"
            ),
            selected.name,
            selected.name.len(),
            MAX_SANDBOX_NAME_BYTES
        ),
        ExitKind::Config,
    )
    .with_hint("rename the manifest to 48 bytes or fewer"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod sandbox_name_tests {
    use super::*;
    use crate::config::ArtifactForm;

    fn manifest(name: &str) -> ResolvedArtifact {
        ResolvedArtifact {
            kind: ArtifactKind::Manifest,
            name: name.to_owned(),
            form: ArtifactForm::Flat,
            path: PathBuf::from(format!("/config/manifests/{name}.toml")),
        }
    }

    #[test]
    fn manifest_sandbox_names_accept_48_bytes_and_refuse_49() {
        assert!(validate_sandbox_name(&manifest(&"a".repeat(48))).is_ok());
        let failure = validate_sandbox_name(&manifest(&"a".repeat(49))).unwrap_err();
        assert_eq!(failure.code(), ExitKind::Config);
        assert!(format!("{failure:?}").contains("name-too-long"));
    }
}

/// ADR-0109's second-claimant condition, as a value rather than only as a refusal.
///
/// A sibling of [`UndeclaredWorkspace`] and for the same reason: ADR-0109 gives the second
/// claimant the same code and the same message shape as the undeclared directory, and both have
/// two consumers. Every manifest-resolving verb turns one into a `78`; `viv doctor` reports it.
/// Keeping the finding a value is what lets the second consumer exist at all — a `Failure` is
/// something you return, and doctor needs something it can hold.
pub(super) struct AmbiguousWorkspaceOwner {
    cwd: PathBuf,
    owners: Vec<(String, PathBuf)>,
}

impl AmbiguousWorkspaceOwner {
    fn owner_list(&self) -> String {
        self.owners
            .iter()
            .map(|(name, path)| format!("`{name}` ({})", path.display()))
            .collect::<Vec<_>>()
            .join(" and ")
    }

    pub(super) fn message(&self) -> String {
        format!(
            "{} both claim the working directory `{}`",
            self.owner_list(),
            self.cwd.display()
        )
    }

    // Paired with `message` above, which needs the receiver. Splitting the pair so one is an
    // associated function would make the two halves of one diagnostic read differently at every
    // call site.
    #[allow(clippy::unused_self)]
    pub(super) fn hint(&self) -> String {
        "remove one ownership claim so exactly one manifest declares this tree".to_owned()
    }

    fn failure(&self) -> Failure {
        diagnosed(
            Namespace::State,
            "workspace-owner-ambiguous",
            "the working directory matches more than one manifest binding",
            Locus::File(self.cwd.clone()),
            self.message(),
            ExitKind::Config,
        )
        .with_hint(self.hint())
    }
}

fn ambiguous_workspace_owner<'a>(
    cwd: &Path,
    owners: impl Iterator<Item = &'a ResolvedArtifact>,
) -> AmbiguousWorkspaceOwner {
    AmbiguousWorkspaceOwner {
        cwd: cwd.to_path_buf(),
        owners: owners
            .map(|selected| (selected.name.clone(), selected.path.clone()))
            .collect(),
    }
}

fn expanded_workspace_sources<E: Environment>(
    context: &Context<'_, E>,
    sources: &[String],
) -> Vec<PathBuf> {
    let lookup = |name: &str| {
        context
            .environment
            .dynamic_variable(name)
            .map(|value| value.to_string_lossy().into_owned())
    };
    sources
        .iter()
        .filter_map(|source| {
            let expanded = crate::launch::mounts::expand_source(source, &lookup).ok()?;
            let path = PathBuf::from(expanded);
            Some(path.canonicalize().unwrap_or(path))
        })
        .collect()
}

fn containing_workspace<'a>(
    cwd: &Path,
    workspaces: impl Iterator<Item = &'a PathBuf>,
) -> Option<PathBuf> {
    workspaces
        .filter(|workspace| cwd.starts_with(workspace))
        .max_by_key(|workspace| workspace.components().count())
        .cloned()
}

/// A workspace finding a resolving verb refuses on and `doctor` reports.
///
/// Two shapes, one channel. A directory declared by no workspace is `ADR-0109`'s refusal; a
/// declared workspace that cannot be resolved is `ADR-0100`'s and `ADR-0108`'s, moved here from
/// launch by `ADR-0110`. They share a channel because every consumer treats them the same way:
/// refuse, except `doctor`, which reports and keeps checking. Kept as a value rather than raised
/// on the spot for exactly that reason — the previous shape discarded one of them through an
/// `.ok()` and rendered the wrong finding.
#[derive(Clone, Debug)]
pub(super) enum WorkspaceFinding {
    Undeclared(UndeclaredWorkspace),
    Defect(crate::launch::workspace::WorkspaceDefect),
}

impl WorkspaceFinding {
    pub(super) fn message(&self) -> String {
        match self {
            Self::Undeclared(finding) => finding.message(),
            Self::Defect(defect) => defect_message(defect),
        }
    }

    pub(super) fn hint(&self) -> String {
        match self {
            Self::Undeclared(finding) => finding.hint(),
            Self::Defect(defect) => defect_hint(defect),
        }
    }

    fn failure(&self) -> Failure {
        match self {
            Self::Undeclared(finding) => finding.failure(),
            Self::Defect(defect) => workspace_defect_failure(defect),
        }
    }
}

/// The published id for one defect. Ids are a published surface (ADR-0075), so each is a literal:
/// a grep for one of these strings has to find the site that emits it. Every one of them predates
/// ADR-0110 and is carried across unchanged — only where it fires moved.
const fn defect_id(kind: &crate::launch::workspace::WorkspaceDefectKind) -> &'static str {
    use crate::launch::workspace::WorkspaceDefectKind as Kind;
    match kind {
        Kind::UnsetVariable { .. } => "workspace-source-unset-variable",
        Kind::NotAbsolute { .. } => "workspace-source-not-absolute",
        Kind::Missing { .. } => "workspace-source-missing",
        Kind::Unreadable { .. } => "workspace-source-unreadable",
        Kind::NotDirectory { .. } => "workspace-source-not-directory",
        Kind::GuestOwned { .. } => "workspace-path-unmirrorable",
        Kind::Nested { .. } => "workspace-paths-overlap",
    }
}

fn defect_message(defect: &crate::launch::workspace::WorkspaceDefect) -> String {
    use crate::launch::workspace::WorkspaceDefectKind as Kind;
    match &defect.kind {
        Kind::UnsetVariable { variable } => {
            format!("a declared workspace names `${{{variable}}}`, which this host does not set")
        }
        Kind::NotAbsolute { .. } => {
            "a declared workspace does not expand to an absolute path".to_owned()
        }
        Kind::Missing { .. } => "a declared workspace does not exist on this host".to_owned(),
        Kind::Unreadable { .. } => "a declared workspace cannot be read on this host".to_owned(),
        Kind::NotDirectory { .. } => "a declared workspace is not a directory".to_owned(),
        Kind::GuestOwned { .. } => {
            "a declared workspace cannot be mounted inside the guest at its host path".to_owned()
        }
        Kind::Nested { .. } => "two declared workspaces overlap".to_owned(),
    }
}

fn defect_hint(defect: &crate::launch::workspace::WorkspaceDefect) -> String {
    use crate::launch::workspace::WorkspaceDefectKind as Kind;
    match &defect.kind {
        Kind::UnsetVariable { variable } => {
            format!("set `{variable}`, or write the path out in full in `[[workspaces]]`")
        }
        Kind::NotAbsolute { .. } | Kind::Missing { .. } | Kind::Unreadable { .. } => {
            "`[[workspaces]] source` names one existing tree by its absolute path".to_owned()
        }
        Kind::NotDirectory { .. } => {
            "declare a directory in `[[workspaces]] source`; a single file is a `[[mounts]]` row"
                .to_owned()
        }
        Kind::GuestOwned { .. } => {
            "move the workspace under a path the guest does not own, such as your home".to_owned()
        }
        Kind::Nested { .. } => "declare disjoint workspace trees".to_owned(),
    }
}

fn workspace_defect_failure(defect: &crate::launch::workspace::WorkspaceDefect) -> Failure {
    use crate::launch::workspace::WorkspaceDefectKind as Kind;
    let declared = &defect.declared;
    let locus = match &defect.kind {
        Kind::UnsetVariable { .. } => Locus::Named("declared workspaces"),
        Kind::NotAbsolute { expanded }
        | Kind::Missing { expanded }
        | Kind::Unreadable { expanded, .. }
        | Kind::NotDirectory { expanded }
        | Kind::GuestOwned { expanded, .. } => Locus::File(expanded.clone()),
        Kind::Nested { left, .. } => Locus::File(left.clone()),
    };
    // The declared spelling in every `why`, because the refusal reads against what the user wrote
    // rather than against an expansion they never typed.
    let why = match &defect.kind {
        Kind::UnsetVariable { variable } => {
            format!("`{declared}` cannot resolve while `{variable}` is unset (ADR-0020)")
        }
        Kind::NotAbsolute { expanded } => {
            format!("`{declared}` expanded to `{}`", expanded.display())
        }
        Kind::Missing { expanded } => format!(
            "`{declared}` expanded to `{}`, which is missing",
            expanded.display()
        ),
        Kind::Unreadable { expanded, error } => format!(
            "`{declared}` expanded to `{}`, which could not be read: {error}",
            expanded.display()
        ),
        Kind::NotDirectory { expanded } => format!(
            "`{declared}` expanded to `{}`, which is not a directory",
            expanded.display()
        ),
        Kind::GuestOwned { reason, .. } => (*reason).to_owned(),
        Kind::Nested { left, right } => format!(
            "`{}` and `{}` are equal or nested",
            left.display(),
            right.display()
        ),
    };
    diagnosed(
        Namespace::Host,
        defect_id(&defect.kind),
        defect_message(defect),
        locus,
        why,
        ExitKind::Config,
    )
    .with_hint(defect_hint(defect))
}

#[derive(Clone, Debug)]
pub(super) struct UndeclaredWorkspace {
    manifest: String,
    manifest_path: PathBuf,
    cwd: PathBuf,
}

fn undeclared_workspace(selected: &ResolvedArtifact, cwd: &Path) -> UndeclaredWorkspace {
    UndeclaredWorkspace {
        manifest: selected.name.clone(),
        manifest_path: selected.path.clone(),
        cwd: cwd.to_path_buf(),
    }
}

impl UndeclaredWorkspace {
    pub(super) fn block(&self) -> String {
        let source = toml::Value::String(self.cwd.to_string_lossy().into_owned()).to_string();
        format!("[[workspaces]]\nsource = {source}")
    }

    pub(super) fn message(&self) -> String {
        format!(
            concat!(
                "manifest `{}` at `{}` does not declare the working directory `{}` in any ",
                "`[[workspaces]]` block"
            ),
            self.manifest,
            self.manifest_path.display(),
            self.cwd.display()
        )
    }

    pub(super) fn hint(&self) -> String {
        format!("add this exact block to the manifest:\n\n{}", self.block())
    }

    fn failure(&self) -> Failure {
        diagnosed(
            Namespace::Manifest,
            "workspace-undeclared-directory",
            "the working directory is not declared as a workspace",
            Locus::File(self.manifest_path.clone()),
            self.message(),
            ExitKind::Config,
        )
        .with_hint(self.hint())
    }
}

/// The evaluate-refuse-defects half `start` shares with the config readers.
///
/// It reaches the same generated tree by the same route, deliberately: a `start` that built from a
/// tree the readers never saw would make `viv config eval` a report about something else.
pub(super) fn evaluate_resolved_for_launch<E: Environment>(
    context: &Context<'_, E>,
    sandbox_id: &str,
    resolved: ResolvedForLaunch,
) -> Result<LaunchInputs, Failure> {
    let evaluated = evaluate_resolved(context, sandbox_id, resolved)?;
    // A content defect means the merge produced no answer, so there is nothing to build.
    if let Some(failure) = defect_failure(&evaluated.analysis) {
        return Err(failure);
    }
    // A declared credential channel refuses before the build, not merely before the boot: a cold
    // start costs minutes, and the acceptance forbids spending them on a launch whose host agent
    // is already known unusable. The resolution is discarded — `execute_runner` resolves again
    // from the built contract, which is the only source under `--no-rebuild`.
    lifecycle::resolve_declared_agent_sockets(&merged_credentials(&evaluated.analysis))?;
    Ok(LaunchInputs {
        flake_directory: evaluated.flake_directory,
        resources: merged_resources(&evaluated.analysis),
        volumes: merged_volumes(&evaluated.analysis),
        lock_snapshot: evaluated.lock_snapshot,
    })
}

/// The volumes the merge declared, each attributed to the layer that declared it.
///
/// Read from the contributors rather than from the effective list, because the effective list is
/// the concatenation and carries no provenance — and provenance is the whole reason this is
/// collected here, where an evaluation has already happened, instead of at `viv volume list`.
///
/// Two layers may declare one name, which is legal exactly while they agree about the mountpoint
/// (`merged.rs` refuses the rest as a `65`). The first contributor in merge order is the one named,
/// and the ceiling is the largest anyone asked for — which is what `nix/guest.nix` provisions, so
/// this file describes the volume that exists rather than one declaration of it.
fn merged_volumes(analysis: &config::merged::Analysis) -> Vec<config::volumes::DeclaredVolume> {
    let Some(view) = analysis.value_of("volumes") else {
        return Vec::new();
    };
    let mut declared: Vec<config::volumes::DeclaredVolume> = Vec::new();
    for contributor in &view.contributors {
        let Some(items) = contributor.value.as_array() else {
            continue;
        };
        for item in items {
            let Some(name) = item.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let size_gib = item
                .get("size_gib")
                .and_then(serde_json::Value::as_u64)
                .and_then(|size| u32::try_from(size).ok());
            if let Some(existing) = declared.iter_mut().find(|volume| volume.name == name) {
                existing.size_gib = existing.size_gib.max(size_gib);
                continue;
            }
            declared.push(config::volumes::DeclaredVolume {
                name: name.to_owned(),
                mount: item
                    .get("mount")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                declared_by: contributor.layer.clone(),
                kind: contributor.kind.as_str().to_owned(),
                size_gib,
            });
        }
    }
    declared
}

/// The credential channels the merge declared, in merge order.
///
/// Read from the effective list rather than the contributors because no provenance is needed
/// here — the refusal names the channel id, and `viv config sources` is the provenance surface.
/// An unparsable member cannot occur: the option is a closed enum the evaluation already
/// refused, so the filter is shape tolerance, not policy.
fn merged_credentials(analysis: &config::merged::Analysis) -> Vec<crate::protocol::CredentialId> {
    analysis
        .effective("credentials.agents")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(|id| id.parse().ok())
                .collect()
        })
        .unwrap_or_default()
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
