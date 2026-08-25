//! `argv` in, one parsed invocation out, and nothing else.
//!
//! Hand-written rather than reached for through a parser library because spec/01 fixes the surface
//! and spec/14 fixes what each rejection costs — `64` for a bad invocation, and nothing else from
//! this stage. A library would decide both for us, and its message and code would become the
//! contract by accident.
//!
//! The rule that shapes every verb below: grammar is decided before anything is resolved. A wrong
//! flag on a command in an unbound project is `64`, not `78`, because the invocation never became
//! well-formed enough to need a manifest. That ordering is what spec/14's command matrix means by
//! listing both codes against one verb.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::PathBuf;

// Verbosity lives in `ui` because it is what the stderr face reads, and `ui` is a leaf the
// grammar may import while the reverse would invert the layering. Re-exported here because it is
// this grammar's output: `Parsed` carries it, and a caller should not need to know where it lives.
pub use crate::ui::Verbosity;

/// Why an invocation is not well-formed. Every one of these is `64`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageError {
    /// What was wrong, in the wording rules spec/14 fixes for the `what` slot.
    pub message: String,
    /// The one-line form of the verb that was attempted, when one was recognized.
    pub usage: Option<&'static str>,
}

impl UsageError {
    fn new(message: impl Into<String>, usage: Option<&'static str>) -> Self {
        Self {
            message: message.into(),
            usage,
        }
    }
}

/// How much a command was asked to say.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Output {
    /// The human-facing rendering.
    Human,
    /// The single keyed JSON object spec/01 fixes for this command.
    Json,
}

impl Output {
    /// Whether `--json` was given.
    #[must_use]
    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// One well-formed invocation.
///
/// The variants split by what the command needs rather than by verb name, because that is what
/// decides how far execution gets before a missing binding stops it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Invocation {
    /// The binding record.
    Config {
        manifest: Option<String>,
        output: Output,
    },
    /// The merged, evaluated configuration.
    ConfigEval { output: Output },
    /// The provenance view behind that merge.
    ConfigSources { output: Output },
    /// Enumerate the manifest library.
    ManifestList { output: Output },
    /// Show one manifest.
    ManifestShow { name: String, output: Output },
    /// Bring the project's VM up, detached, and return once it is running (spec/10).
    Start {
        rebuild: bool,
        no_rebuild: bool,
        /// Boot this retained generation instead of the current one, evaluating nothing (spec/11).
        generation: Option<u64>,
        /// The console-streaming form, whose post-condition differs from the detached one.
        attach: bool,
        output: Output,
    },
    /// What the project's VM is doing, read-only (spec/01, ADR-0030).
    Status { global: bool, output: Output },
    /// Bring the project's VM down, stopping at the teardown boundary (ADR-0018).
    Stop {
        all: bool,
        force: bool,
        /// Seconds of grace before a hard poweroff. `-1` waits indefinitely (spec/10).
        timeout: Option<i64>,
        output: Output,
    },
    /// Run one command inside the project's sandbox and return its status (spec/12).
    Exec(Session),
    /// Open an interactive login shell inside the project's sandbox (spec/12).
    Shell(Session),
    /// The slice-002 supervisor handoff, which the tool re-invokes itself with after the built
    /// runner renders the specification (ADR-0102).
    ///
    /// Kept as a sub-form of `start` rather than promoted to its own verb because it is not part of
    /// the published surface: spec/01's `start` is the manifest-driven one, and this is the private
    /// spelling that hands a resolved specification to the async launch half.
    StartSpec { spec: PathBuf },
    /// What volumes this project has, read-only (spec/01, ADR-0019).
    VolumeList { output: Output },
    /// Remove the volume images no current layer declares (ADR-0067).
    VolumePrune {
        /// Print the rows a run would remove and remove nothing.
        dry_run: bool,
        /// Consent, given ahead of the prompt.
        yes: bool,
        output: Output,
    },
    /// Tear the project down, at the boundary spec/10 fixes (ADR-0043, ADR-0080).
    Destroy {
        /// Spare the volume images; everything else still goes.
        keep_volumes: bool,
        yes: bool,
        output: Output,
    },
    /// The retained generations, read-only (spec/11).
    GenerationsList { output: Output },
    /// Unlink retained generations under exactly one retention argument (spec/11).
    GenerationsPrune {
        /// Keep this many of the newest generations.
        keep: Option<u64>,
        /// Keep every generation younger than this many seconds.
        older_than_seconds: Option<u64>,
        output: Output,
    },
    /// Move the `current` pointer to a named retained generation (spec/11).
    GenerationsActivate { number: u64, output: Output },
    /// Move the `current` pointer to the previous retained generation (spec/11).
    GenerationsRollback { output: Output },
    /// Run the whole-store garbage collector — global, so it needs no binding and never
    /// answers `78` (spec/11).
    Gc { output: Output },
    /// Diagnose the host and project setup from the shared probe catalog (spec/13).
    Doctor {
        /// Any warn fails the run at exit `1` — the one sanctioned use of `1` (ADR-0023).
        strict: bool,
        /// Enumerate the catalog without running any probe.
        list: bool,
        /// Admit the network-scope probes; offline is the default and there is no `--offline`.
        online: bool,
        output: Output,
    },
    /// `--help` anywhere before the first `--`: the summary, or one verb's usage.
    ///
    /// Parsed globally like the verbosity flags, and it wins over whatever else the invocation
    /// says — `viv start --bogus --help` is a user asking what `start` takes, not a usage error.
    Help {
        /// The verb the help was asked beside, unresolved: an unknown name falls back to the
        /// summary rather than failing, because help never fails.
        verb: Option<String>,
    },
    /// `--version` anywhere before the first `--`: the version string and nothing else.
    Version,
}

/// One parsed command line: the invocation, plus the globals that ride beside any verb.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parsed {
    pub invocation: Invocation,
    pub verbosity: Verbosity,
}

/// Everything one guest session is asked for, in the form the wire's `Start` needs it.
///
/// `OsString` rather than `String` throughout because spec/12 makes the guest argv byte-for-byte
/// and the wire carries argv and environment values as raw bytes. A grammar that decoded them
/// would refuse an invocation the contract admits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    /// Guest argv, everything after the first `--`. Empty for `shell`, which resolves the user's
    /// own login shell in the guest and ignores argv entirely.
    pub argv: Vec<OsString>,
    /// Whether the guest allocates a PTY. Resolved here because the two flags, the verb, and the
    /// host's own stdin are all facts this stage already holds.
    pub pty: bool,
    /// Each `--env` in the order given. `None` is the `KEY` form: copy from the host if it exists.
    pub env: Vec<(OsString, Option<OsString>)>,
}

/// What the host can tell a command about its own streams.
///
/// Passed in rather than probed here so the whole grammar stays decidable without a terminal —
/// `-t` off a TTY is a usage error, and a test that could not say so would have to run under a pty
/// to exercise it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Streams {
    pub stdin_is_tty: bool,
    /// Whether the result stream may carry color; the chain in `ui::chain` reads it.
    pub stdout_is_tty: bool,
    /// Whether the human face exists at all: progress animates only here (spec/01).
    pub stderr_is_tty: bool,
}

/// Parses one invocation.
///
/// `argv` includes the program name, as `std::env::args_os` yields it.
///
/// The global flags — `-v`/`-q`, `--help`, `--version` — are lifted out first, so placement is
/// free per ADR-0026: `viv -v start` and `viv start -v` are one invocation. The lift stops at the
/// first `--` because everything after it is the guest's (spec/12), and it is verb-keyed for
/// value-taking flags because `-t` is a boolean on `exec` and takes seconds on `stop` — a lift
/// that read past one would silently change that parse.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown verb, an unknown flag, wrong arity, or two flags that
/// contradict each other. Every one of them is `64`.
pub fn parse<I>(argv: I, streams: Streams) -> Result<Parsed, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let globals = lift_globals(argv.into_iter().skip(1));
    let verbosity = globals.verbosity;
    // Help wins over everything, version over everything but help: both are the user asking about
    // the tool rather than asking the tool for anything, so neither needs the rest to be
    // well-formed.
    if globals.help {
        return Ok(Parsed {
            invocation: Invocation::Help { verb: globals.verb },
            verbosity,
        });
    }
    if globals.version {
        return Ok(Parsed {
            invocation: Invocation::Version,
            verbosity,
        });
    }

    let tokens = globals.tokens;
    let Some(verb) = tokens.first() else {
        return Err(UsageError::new("no command given", Some(TOP_USAGE)));
    };
    let rest = &tokens[1..];

    let invocation = match verb.to_str() {
        Some("config") => config(rest),
        Some("manifest") => manifest(rest),
        Some("start") => start(rest),
        Some("status") => status(rest),
        Some("shell") => shell(rest),
        Some("exec") => exec(rest, streams),
        Some("stop") => stop(rest),
        Some("volume") => volume(rest, streams),
        Some("generations") => generations(rest),
        Some("destroy") => destroy(rest, streams),
        Some("gc") => gc(rest),
        Some("doctor") => doctor(rest),
        _ => Err(UsageError::new(
            format!("unknown command `{}`", verb.to_string_lossy()),
            Some(TOP_USAGE),
        )),
    }?;
    Ok(Parsed {
        invocation,
        verbosity,
    })
}

/// What the global pre-pass produced: the verb-local tokens, and the globals lifted from them.
struct Globals {
    tokens: Vec<OsString>,
    verbosity: Verbosity,
    help: bool,
    version: bool,
    /// The first verb-local token, for `--help`'s "which verb" answer. Never validated here.
    verb: Option<String>,
}

/// Which flags take a value, per verb: the tokens the lift must never read past.
fn takes_value(verb: Option<&str>, flag: &str) -> bool {
    match verb {
        Some("config") => flag == "--manifest",
        Some("start") => matches!(flag, "--spec" | "--generation"),
        Some("exec") => flag == "--env",
        Some("stop") => matches!(flag, "-t" | "--timeout"),
        // Keyed on the family verb, because the lift never sees the subcommand.
        Some("generations") => matches!(flag, "--keep" | "--older-than"),
        _ => false,
    }
}

fn lift_globals(argv: impl Iterator<Item = OsString>) -> Globals {
    let mut globals = Globals {
        tokens: Vec::new(),
        verbosity: Verbosity::Normal,
        help: false,
        version: false,
        verb: None,
    };
    // `-q` and `-v` are mutually exclusive with the last one winning (ADR-0026), and `-v` stacks;
    // an integer carries both rules: `-q` sets it below zero, each `v` adds one from zero up.
    let mut level: i8 = 0;
    let bump = |level: i8, added: i8| {
        if level < 0 {
            added
        } else {
            (level + added).min(3)
        }
    };
    let mut argv = argv.peekable();
    while let Some(token) = argv.next() {
        match token.to_str() {
            // Everything after the first `--` is the guest's, verbatim (spec/12).
            Some("--") => {
                globals.tokens.push(token);
                globals.tokens.extend(argv);
                break;
            }
            Some("-q" | "--quiet") => level = -1,
            Some("-v" | "--verbose") => level = bump(level, 1),
            Some("-vv") => level = bump(level, 2),
            Some("-vvv") => level = bump(level, 3),
            Some("-h" | "--help") => globals.help = true,
            Some("--version") => globals.version = true,
            other => {
                if globals.verb.is_none() {
                    // The verb candidate is simply the first token the lift keeps, dashes and
                    // all: a `viv --json` stays an unknown command downstream, exactly as before.
                    globals.verb = other.map(str::to_owned);
                }
                let value_next =
                    other.is_some_and(|flag| takes_value(globals.verb.as_deref(), flag));
                globals.tokens.push(token);
                if value_next && let Some(value) = argv.next() {
                    globals.tokens.push(value);
                }
            }
        }
    }
    globals.verbosity = match level {
        i8::MIN..=-1 => Verbosity::Quiet,
        0 => Verbosity::Normal,
        1 => Verbosity::Verbose,
        2 => Verbosity::Debug,
        _ => Verbosity::Trace,
    };
    globals
}

const TOP_USAGE: &str = "viv <config|manifest|start|status|shell|exec|stop|generations|volume|\
destroy|gc|doctor> [options]";
const CONFIG_USAGE: &str = "viv config [--manifest <name>] [--json]";
const CONFIG_EVAL_USAGE: &str = "viv config eval [--json]";
const CONFIG_SOURCES_USAGE: &str = "viv config sources [--json]";
const MANIFEST_USAGE: &str = "viv manifest <list|show <name>> [--json]";
const START_USAGE: &str = "viv start [--rebuild|--no-rebuild] [--generation <n>] [--json]";
const STATUS_USAGE: &str = "viv status [--json] [-g|--global]";
const SHELL_USAGE: &str = "viv shell";
const EXEC_USAGE: &str =
    "viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <command> [args...]";
const STOP_USAGE: &str = "viv stop [--all] [--force] [-t|--timeout <secs>] [--json]";
const VOLUME_USAGE: &str = "viv volume <list|prune> [options]";
const VOLUME_LIST_USAGE: &str = "viv volume list [--json]";
const VOLUME_PRUNE_USAGE: &str = "viv volume prune [-n|--dry-run] [-f|--yes] [--json]";
const DESTROY_USAGE: &str = "viv destroy [-f|--yes] [--keep-volumes] [--json]";
const GENERATIONS_USAGE: &str = "viv generations <list|activate|rollback|prune> [options]";
const GENERATIONS_LIST_USAGE: &str = "viv generations list [--json]";
const GENERATIONS_ACTIVATE_USAGE: &str = "viv generations activate <n> [--json]";
const GENERATIONS_ROLLBACK_USAGE: &str = "viv generations rollback [--json]";
const GENERATIONS_PRUNE_USAGE: &str =
    "viv generations prune (--keep <n> | --older-than <dur>) [--json]";
const GC_USAGE: &str = "viv gc [--json]";
const DOCTOR_USAGE: &str = "viv doctor [--json] [--strict] [--list] [--online]";

fn doctor(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let (mut strict, mut list, mut online) = (false, false, false);
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("--strict") => strict = true,
            Some("--list") => list = true,
            Some("--online") => online = true,
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, DOCTOR_USAGE)),
        }
    }
    Ok(Invocation::Doctor {
        strict,
        list,
        online,
        output,
    })
}

/// The one-line usage of the whole surface, for the errors that predate a verb.
#[must_use]
pub const fn top_usage() -> &'static str {
    TOP_USAGE
}

/// The published verbs, one row each: name, usage, and what it does.
///
/// `--help` renders from this table, beside the dispatch in [`parse`] rather than inside it, so
/// the summary and the dispatch can only drift through a diff that touches this file.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "config",
        CONFIG_USAGE,
        "show the binding, its paths, and the merged configuration",
    ),
    (
        "manifest",
        MANIFEST_USAGE,
        "list and show the manifest library",
    ),
    (
        "start",
        START_USAGE,
        "evaluate, build, and boot this project's VM",
    ),
    ("status", STATUS_USAGE, "report what the VM is doing"),
    ("shell", SHELL_USAGE, "open a shell inside the guest"),
    ("exec", EXEC_USAGE, "run one command inside the guest"),
    ("stop", STOP_USAGE, "bring the VM down"),
    (
        "generations",
        GENERATIONS_USAGE,
        "list, switch, and prune retained builds",
    ),
    (
        "volume",
        VOLUME_USAGE,
        "list and prune this project's volumes",
    ),
    ("destroy", DESTROY_USAGE, "remove the VM and its state"),
    ("gc", GC_USAGE, "collect unreferenced build outputs"),
    (
        "doctor",
        DOCTOR_USAGE,
        "diagnose the host and project setup",
    ),
];

fn config(rest: &[OsString]) -> Result<Invocation, UsageError> {
    // spec/01's other two `config` forms. Neither takes `--manifest`: they read the binding in
    // force, and a flag that selected a different manifest would report a merge for a project that
    // is not bound to it.
    if let Some(first) = rest.first() {
        match first.to_str() {
            Some("eval") => {
                return Ok(Invocation::ConfigEval {
                    output: flags_only(&rest[1..], CONFIG_EVAL_USAGE)?,
                });
            }
            Some("sources") => {
                return Ok(Invocation::ConfigSources {
                    output: flags_only(&rest[1..], CONFIG_SOURCES_USAGE)?,
                });
            }
            _ => {}
        }
    }

    let mut manifest = None;
    let mut output = Output::Human;
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        match token.to_str() {
            Some("--manifest") => manifest = Some(value(&mut tokens, "--manifest", CONFIG_USAGE)?),
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, CONFIG_USAGE)),
        }
    }
    Ok(Invocation::Config { manifest, output })
}

fn manifest(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let Some(sub) = rest.first() else {
        return Err(UsageError::new(
            "`viv manifest` needs a subcommand",
            Some(MANIFEST_USAGE),
        ));
    };
    let rest = &rest[1..];
    match sub.to_str() {
        Some("list") => {
            let output = flags_only(rest, MANIFEST_USAGE)?;
            Ok(Invocation::ManifestList { output })
        }
        Some("show") => {
            let mut name: Option<String> = None;
            let mut output = Output::Human;
            for token in rest {
                match token.to_str() {
                    Some("--json") => output = Output::Json,
                    Some(text) if !text.starts_with('-') => {
                        if name.is_some() {
                            // Arity is checked rather than tolerated: `show` takes exactly one
                            // name, and silently ignoring the second would make a typo look like
                            // a successful query of the first.
                            return Err(UsageError::new(
                                "`viv manifest show` takes exactly one manifest name",
                                Some(MANIFEST_USAGE),
                            ));
                        }
                        name = Some(text.to_owned());
                    }
                    _ => return Err(unknown(token, MANIFEST_USAGE)),
                }
            }
            name.map_or_else(
                || {
                    Err(UsageError::new(
                        "`viv manifest show` needs a manifest name",
                        Some(MANIFEST_USAGE),
                    ))
                },
                |name| Ok(Invocation::ManifestShow { name, output }),
            )
        }
        _ => Err(UsageError::new(
            format!(
                "unknown `viv manifest` subcommand `{}`",
                sub.to_string_lossy()
            ),
            Some(MANIFEST_USAGE),
        )),
    }
}

fn start(rest: &[OsString]) -> Result<Invocation, UsageError> {
    if rest.first().is_some_and(|token| token == "--spec") {
        let spec = rest
            .get(1)
            .ok_or_else(|| UsageError::new("`start --spec` needs a path", Some(START_USAGE)))?;
        let spec = PathBuf::from(spec);
        if rest.len() != 2 || !spec.is_absolute() {
            return Err(UsageError::new(
                "`start --spec` takes exactly one absolute path",
                Some(START_USAGE),
            ));
        }
        return Ok(Invocation::StartSpec { spec });
    }

    let (mut rebuild, mut no_rebuild, mut attach) = (false, false, false);
    let mut generation = None;
    let mut output = Output::Human;
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        match token.to_str() {
            Some("--rebuild") => rebuild = true,
            Some("--no-rebuild") => no_rebuild = true,
            Some("--generation") => {
                let raw = value(&mut tokens, "--generation", START_USAGE)?;
                generation = Some(raw.parse().map_err(|_| {
                    UsageError::new(
                        format!("`--generation` expects a generation number, got `{raw}`"),
                        Some(START_USAGE),
                    )
                })?);
            }
            // Parsed, never dropped: spec/10 makes `--attach` a console-streaming form whose
            // post-condition differs from the detached one, so the verb refuses it by name rather
            // than performing a different operation under it. The refusal is in `lifecycle::start`
            // with the other unimplemented forms, because it is a missing capability and not a
            // malformed invocation.
            Some("--attach") => attach = true,
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, START_USAGE)),
        }
    }
    if rebuild && no_rebuild {
        return Err(UsageError::new(
            "`--rebuild` and `--no-rebuild` contradict each other",
            Some(START_USAGE),
        ));
    }
    // Both rebuild flags contradict a named generation, each its own way: `--rebuild` asks for a
    // fresh build and `--no-rebuild` names the current generation, while `--generation` names a
    // different one. Precedence would perform one of the two requests silently.
    if generation.is_some() && (rebuild || no_rebuild) {
        return Err(UsageError::new(
            "`--generation` conflicts with `--rebuild` and `--no-rebuild`",
            Some(START_USAGE),
        ));
    }
    Ok(Invocation::Start {
        rebuild,
        no_rebuild,
        generation,
        attach,
        output,
    })
}

fn status(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let mut global = false;
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("-g" | "--global") => global = true,
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, STATUS_USAGE)),
        }
    }
    Ok(Invocation::Status { global, output })
}

/// `viv shell`, whose whole grammar is its own name.
///
/// No `--json`: spec/12's grammar line carries none, and a verb that hands the terminal to a guest
/// shell has no keyed object to emit. It always allocates a guest PTY, so the flags `exec` needs to
/// choose one have nothing to decide here.
///
/// Off a terminal it is still well-formed. What the host cannot supply — raw mode, a size —
/// is a capability the session degrades on, not a malformed invocation, and the trials that run
/// `viv shell` unbound through a pipe expect `78` from the binding it lacks rather than `64`.
fn shell(rest: &[OsString]) -> Result<Invocation, UsageError> {
    if let Some(token) = rest.first() {
        return Err(unknown(token, SHELL_USAGE));
    }
    Ok(Invocation::Shell(Session {
        argv: Vec::new(),
        pty: true,
        env: Vec::new(),
    }))
}

fn exec(rest: &[OsString], streams: Streams) -> Result<Invocation, UsageError> {
    let (mut tty, mut no_tty) = (false, false);
    let mut env = Vec::new();
    let mut separator = None;
    let mut index = 0;
    while let Some(token) = rest.get(index) {
        // The first `--` ends vivarium's parsing entirely, which is what makes everything past it
        // guest argv byte for byte — including a second `--`, which is an ordinary guest argument.
        if token == "--" {
            separator = Some(index);
            break;
        }
        match token.to_str() {
            Some("-t" | "--tty") => tty = true,
            Some("-T" | "--no-tty") => no_tty = true,
            Some("--env") => {
                let assignment = rest
                    .get(index + 1)
                    .ok_or_else(|| UsageError::new("`--env` needs a value", Some(EXEC_USAGE)))?;
                env.push(env_assignment(assignment)?);
                index += 1;
            }
            _ => return Err(unknown(token, EXEC_USAGE)),
        }
        index += 1;
    }
    if tty && no_tty {
        return Err(UsageError::new(
            "`-t` and `-T` contradict each other",
            Some(EXEC_USAGE),
        ));
    }
    // `-t` asks for a pty allocated from this process's own stdin. Off a terminal there is nothing
    // to allocate one from, so the request cannot be honored and saying so now is better than
    // starting a guest process that would then behave unlike the one that was asked for.
    if tty && !streams.stdin_is_tty {
        return Err(UsageError::new(
            "`-t` was given but stdin is not a terminal",
            Some(EXEC_USAGE),
        ));
    }
    let Some(separator) = separator else {
        return Err(UsageError::new(
            "`viv exec` needs `--` followed by a command",
            Some(EXEC_USAGE),
        ));
    };
    let argv = &rest[separator + 1..];
    if argv.is_empty() {
        return Err(UsageError::new(
            "`viv exec --` needs a command to run",
            Some(EXEC_USAGE),
        ));
    }
    Ok(Invocation::Exec(Session {
        argv: argv.to_vec(),
        // spec/12: `exec` defaults to no guest PTY, `-t` allocates one, and `-T` forces none. With
        // the contradiction already refused, the flag that was given is the whole answer.
        pty: tty,
        env,
    }))
}

/// Splits one `--env` operand into `KEY` or `KEY=VAL`.
///
/// Deliberately not routed through [`value`]: that helper refuses anything but UTF-8, and an
/// environment value on this wire is raw bytes. Only the split point is decided here, so a name
/// vivarium cannot decode still reaches the guest exactly as the host spelled it.
fn env_assignment(token: &OsStr) -> Result<(OsString, Option<OsString>), UsageError> {
    let bytes = token.as_bytes();
    match bytes.iter().position(|byte| *byte == b'=') {
        Some(0) => Err(UsageError::new(
            "`--env` needs a variable name before `=`",
            Some(EXEC_USAGE),
        )),
        Some(split) => Ok((
            OsString::from_vec(bytes[..split].to_vec()),
            Some(OsString::from_vec(bytes[split + 1..].to_vec())),
        )),
        None if bytes.is_empty() => Err(UsageError::new(
            "`--env` needs a variable name",
            Some(EXEC_USAGE),
        )),
        None => Ok((token.to_os_string(), None)),
    }
}

fn stop(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let (mut force, mut all) = (false, false);
    let mut timeout: Option<i64> = None;
    let mut output = Output::Human;
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        match token.to_str() {
            Some("--force") => force = true,
            Some("--all") => all = true,
            Some("-t" | "--timeout") => {
                let raw = value(&mut tokens, "--timeout", STOP_USAGE)?;
                let seconds: i64 = raw.parse().map_err(|_| {
                    UsageError::new(
                        format!("`--timeout` expects a whole number of seconds, got `{raw}`"),
                        Some(STOP_USAGE),
                    )
                })?;
                // spec/10 reserves exactly one negative value: `-1` waits indefinitely. Anything
                // below it is outside the option's domain, and accepting it would silently turn a
                // mistyped bounded stop into an unbounded one — the reading a user is least able
                // to notice, since the command simply never returns.
                if seconds < -1 {
                    return Err(UsageError::new(
                        format!(
                            "`--timeout` accepts seconds from `0` up, or `-1` to wait \
                            indefinitely, got `{raw}`"
                        ),
                        Some(STOP_USAGE),
                    ));
                }
                timeout = Some(seconds);
            }
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, STOP_USAGE)),
        }
    }
    // `--force` powers off immediately, so a nonzero grace period is not a preference it overrides
    // but a request it contradicts. `--force --timeout 0` says the same thing twice and is fine.
    if force && timeout.is_some_and(|seconds| seconds != 0) {
        return Err(UsageError::new(
            "`--force` conflicts with a nonzero `--timeout`",
            Some(STOP_USAGE),
        ));
    }
    Ok(Invocation::Stop {
        all,
        force,
        timeout,
        output,
    })
}

fn volume(rest: &[OsString], streams: Streams) -> Result<Invocation, UsageError> {
    let Some(sub) = rest.first() else {
        return Err(UsageError::new(
            "`viv volume` needs a subcommand",
            Some(VOLUME_USAGE),
        ));
    };
    match sub.to_str() {
        Some("list") => Ok(Invocation::VolumeList {
            output: flags_only(&rest[1..], VOLUME_LIST_USAGE)?,
        }),
        Some("prune") => {
            let (mut dry_run, mut yes) = (false, false);
            let mut output = Output::Human;
            for token in &rest[1..] {
                match token.to_str() {
                    Some("--dry-run" | "-n") => dry_run = true,
                    Some("--yes" | "-f") => yes = true,
                    Some("--json") => output = Output::Json,
                    _ => return Err(unknown(token, VOLUME_PRUNE_USAGE)),
                }
            }
            // The same consent rule `destroy` follows, and for the same reason — except that a
            // preview removes nothing, so demanding consent for one would be a gate on a read.
            // `--dry-run --yes` would then also have to read as "yes, remove", which is the
            // opposite of what it says.
            if !dry_run && !yes && !streams.stdin_is_tty {
                return Err(UsageError::new(
                    "`viv volume prune` needs `--yes` when stdin is not a terminal",
                    Some(VOLUME_PRUNE_USAGE),
                ));
            }
            Ok(Invocation::VolumePrune {
                dry_run,
                yes,
                output,
            })
        }
        _ => Err(UsageError::new(
            format!(
                "`viv volume {}` is not implemented yet",
                sub.to_string_lossy()
            ),
            Some(VOLUME_USAGE),
        )),
    }
}

fn destroy(rest: &[OsString], streams: Streams) -> Result<Invocation, UsageError> {
    let (mut yes, mut keep_volumes) = (false, false);
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("--yes" | "-f") => yes = true,
            Some("--keep-volumes") => keep_volumes = true,
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, DESTROY_USAGE)),
        }
    }
    // Destroying is irreversible, so consent is required rather than assumed. On a terminal it can
    // be asked for; off one there is nobody to ask, which makes the missing `--yes` a malformed
    // invocation rather than a silent yes.
    if !yes && !streams.stdin_is_tty {
        return Err(UsageError::new(
            "`viv destroy` needs `--yes` when stdin is not a terminal",
            Some(DESTROY_USAGE),
        ));
    }
    Ok(Invocation::Destroy {
        keep_volumes,
        yes,
        output,
    })
}

fn gc(rest: &[OsString]) -> Result<Invocation, UsageError> {
    Ok(Invocation::Gc {
        output: flags_only(rest, GC_USAGE)?,
    })
}

fn generations(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let Some(sub) = rest.first() else {
        return Err(UsageError::new(
            "`viv generations` needs a subcommand",
            Some(GENERATIONS_USAGE),
        ));
    };
    match sub.to_str() {
        Some("list") => Ok(Invocation::GenerationsList {
            output: flags_only(&rest[1..], GENERATIONS_LIST_USAGE)?,
        }),
        Some("activate") => {
            let mut number = None;
            let mut output = Output::Human;
            for token in &rest[1..] {
                match token.to_str() {
                    Some("--json") => output = Output::Json,
                    Some(raw) if number.is_none() && !raw.starts_with('-') => {
                        number = Some(raw.parse().map_err(|_| {
                            UsageError::new(
                                format!("`activate` expects a generation number, got `{raw}`"),
                                Some(GENERATIONS_ACTIVATE_USAGE),
                            )
                        })?);
                    }
                    _ => return Err(unknown(token, GENERATIONS_ACTIVATE_USAGE)),
                }
            }
            let Some(number) = number else {
                return Err(UsageError::new(
                    "`activate` needs the generation number to switch to",
                    Some(GENERATIONS_ACTIVATE_USAGE),
                ));
            };
            Ok(Invocation::GenerationsActivate { number, output })
        }
        Some("rollback") => Ok(Invocation::GenerationsRollback {
            output: flags_only(&rest[1..], GENERATIONS_ROLLBACK_USAGE)?,
        }),
        Some("prune") => {
            let mut keep = None;
            let mut older_than_seconds = None;
            let mut output = Output::Human;
            let mut tokens = rest[1..].iter();
            while let Some(token) = tokens.next() {
                match token.to_str() {
                    Some("--keep") => {
                        let raw = value(&mut tokens, "--keep", GENERATIONS_PRUNE_USAGE)?;
                        keep = Some(raw.parse().map_err(|_| {
                            UsageError::new(
                                format!("`--keep` expects a whole number, got `{raw}`"),
                                Some(GENERATIONS_PRUNE_USAGE),
                            )
                        })?);
                    }
                    Some("--older-than") => {
                        let raw = value(&mut tokens, "--older-than", GENERATIONS_PRUNE_USAGE)?;
                        older_than_seconds = Some(duration_seconds(&raw)?);
                    }
                    Some("--json") => output = Output::Json,
                    _ => return Err(unknown(token, GENERATIONS_PRUNE_USAGE)),
                }
            }
            // Exactly one retention argument, by the spec's own grammar: neither would mean
            // "remove everything", and both would leave which one decides to precedence.
            match (keep, older_than_seconds) {
                (Some(_), Some(_)) => Err(UsageError::new(
                    "`--keep` and `--older-than` contradict each other",
                    Some(GENERATIONS_PRUNE_USAGE),
                )),
                (None, None) => Err(UsageError::new(
                    "`prune` needs `--keep <n>` or `--older-than <dur>`",
                    Some(GENERATIONS_PRUNE_USAGE),
                )),
                _ => Ok(Invocation::GenerationsPrune {
                    keep,
                    older_than_seconds,
                    output,
                }),
            }
        }
        _ => Err(UsageError::new(
            format!(
                "`viv generations {}` is not a subcommand",
                sub.to_string_lossy()
            ),
            Some(GENERATIONS_USAGE),
        )),
    }
}

/// `<count><d|h|m|s>`, the suffix required: a bare number would make the unit a guess, and the
/// guessed reading of a retention window deletes in the unrecoverable direction.
fn duration_seconds(raw: &str) -> Result<u64, UsageError> {
    let malformed = || {
        UsageError::new(
            format!("`--older-than` expects `<number><d|h|m|s>`, got `{raw}`"),
            Some(GENERATIONS_PRUNE_USAGE),
        )
    };
    if !raw.is_ascii() || raw.len() < 2 {
        return Err(malformed());
    }
    let (digits, unit) = raw.split_at(raw.len() - 1);
    let scale: u64 = match unit {
        "d" => 86_400,
        "h" => 3_600,
        "m" => 60,
        "s" => 1,
        _ => return Err(malformed()),
    };
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed());
    }
    let count: u64 = digits.parse().map_err(|_| malformed())?;
    count.checked_mul(scale).ok_or_else(malformed)
}

/// Accepts `--json` and nothing else, which is the whole grammar of several read-only verbs.
fn flags_only(rest: &[OsString], usage: &'static str) -> Result<Output, UsageError> {
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, usage)),
        }
    }
    Ok(output)
}

fn value<'a>(
    tokens: &mut impl Iterator<Item = &'a OsString>,
    flag: &str,
    usage: &'static str,
) -> Result<String, UsageError> {
    let next = tokens
        .next()
        .ok_or_else(|| UsageError::new(format!("`{flag}` needs a value"), Some(usage)))?;
    next.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        UsageError::new(
            format!("`{flag}` needs a value that is valid UTF-8"),
            Some(usage),
        )
    })
}

fn unknown(token: &OsStr, usage: &'static str) -> UsageError {
    UsageError::new(
        format!("unknown option `{}`", token.to_string_lossy()),
        Some(usage),
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{Invocation, Output, Streams, parse};

    fn argv(rest: &[&str]) -> Vec<OsString> {
        std::iter::once("viv")
            .chain(rest.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn tty() -> Streams {
        Streams {
            stdin_is_tty: true,
            stdout_is_tty: true,
            stderr_is_tty: true,
        }
    }

    fn piped() -> Streams {
        Streams {
            stdin_is_tty: false,
            stdout_is_tty: false,
            stderr_is_tty: false,
        }
    }

    fn parsed(rest: &[&str]) -> Result<Invocation, String> {
        parse(argv(rest), piped())
            .map(|parsed| parsed.invocation)
            .map_err(|error| error.message)
    }

    /// The `--json` slot of whichever invocation carries one.
    const fn output_of(invocation: &Invocation) -> Option<Output> {
        match invocation {
            Invocation::Config { output, .. }
            | Invocation::ConfigEval { output }
            | Invocation::ConfigSources { output }
            | Invocation::ManifestList { output }
            | Invocation::ManifestShow { output, .. }
            | Invocation::Start { output, .. }
            | Invocation::Status { output, .. }
            | Invocation::Stop { output, .. }
            | Invocation::VolumeList { output }
            | Invocation::VolumePrune { output, .. }
            | Invocation::Destroy { output, .. }
            | Invocation::GenerationsList { output }
            | Invocation::GenerationsPrune { output, .. }
            | Invocation::GenerationsActivate { output, .. }
            | Invocation::GenerationsRollback { output }
            | Invocation::Gc { output }
            | Invocation::Doctor { output, .. } => Some(*output),
            // None carries a `--json` slot: the private handoff predates the published surface,
            // the two session verbs hand their streams to a guest process, and help and version
            // are human by definition.
            Invocation::StartSpec { .. }
            | Invocation::Exec(_)
            | Invocation::Shell(_)
            | Invocation::Help { .. }
            | Invocation::Version => None,
        }
    }

    /// Pins the supervisor handoff `nix/runner.sh` executes. It is not part of the published
    /// surface, so nothing else asserts it and a rewrite would otherwise break the runner silently.
    #[test]
    fn the_supervisor_handoff_survives_the_published_surface() -> Result<(), String> {
        assert_eq!(
            parsed(&["start", "--spec", "/run/a/launch.json"])?,
            Invocation::StartSpec {
                spec: PathBuf::from("/run/a/launch.json")
            }
        );
        for rejected in [
            vec!["start", "--spec"],                            // no path
            vec!["start", "--spec", "launch.json"],             // relative path
            vec!["start", "--spec", "/run/a/launch.json", "x"], // trailing argument
        ] {
            assert!(
                parse(argv(&rejected), piped()).is_err(),
                "accepted a malformed handoff: {rejected:?}"
            );
        }
        Ok(())
    }

    /// Pins the four contradictions spec/01 makes usage errors rather than precedence questions.
    #[test]
    fn contradicting_flags_are_usage_errors() {
        for rejected in [
            vec!["start", "--rebuild", "--no-rebuild"],
            vec!["exec", "-t", "--no-tty", "--", "true"],
            vec!["stop", "--force", "--timeout", "5"],
        ] {
            assert!(
                parse(argv(&rejected), tty()).is_err(),
                "accepted contradicting flags: {rejected:?}"
            );
        }
        // The same pair saying one thing twice is not a contradiction.
        assert!(parse(argv(&["stop", "--force", "--timeout", "0"]), tty()).is_ok());
    }

    /// spec/10's `--timeout` domain: seconds from zero up, and `-1` alone for an indefinite wait.
    ///
    /// The lower bound is load-bearing rather than tidy. `grace_seconds` reads any negative value
    /// as "no deadline", so a value the grammar let through would turn a mistyped bounded stop
    /// into one that never returns.
    #[test]
    fn the_timeout_domain_admits_only_zero_up_and_minus_one() {
        for accepted in ["0", "1", "600", "-1"] {
            assert!(
                parse(argv(&["stop", "--timeout", accepted]), tty()).is_ok(),
                "rejected `{accepted}`"
            );
        }
        for rejected in ["-2", "-10", "1.5", "abc", ""] {
            assert!(
                parse(argv(&["stop", "--timeout", rejected]), tty()).is_err(),
                "accepted `{rejected}`"
            );
        }
    }

    /// Pins the two decisions that read the host's own streams rather than a flag.
    #[test]
    fn stream_dependent_grammar_reads_the_host_not_a_flag() {
        // `-t` needs a terminal to allocate a pty from.
        assert!(parse(argv(&["exec", "-t", "--", "true"]), piped()).is_err());
        assert!(parse(argv(&["exec", "-t", "--", "true"]), tty()).is_ok());

        // `destroy` needs consent, which can be asked for on a terminal and not otherwise.
        assert!(parse(argv(&["destroy"]), piped()).is_err());
        assert!(parse(argv(&["destroy"]), tty()).is_ok());
        assert!(parse(argv(&["destroy", "--yes"]), piped()).is_ok());
    }

    /// Pins `exec`'s arity: the separator is required and so is something after it.
    #[test]
    fn exec_requires_a_separator_and_a_command() {
        assert!(parse(argv(&["exec"]), tty()).is_err());
        assert!(parse(argv(&["exec", "--"]), tty()).is_err());
        assert!(parse(argv(&["exec", "--", "true"]), tty()).is_ok());
    }

    /// Pins that everything past the first `--` survives parsing unexamined.
    ///
    /// The predecessor of this stage found the separator, used it for the arity check above, and
    /// discarded what followed. That is the defect this asserts against: a flag spelling, a second
    /// separator, and a lone `-` all have meanings to vivarium that they must not have here.
    #[test]
    fn the_argv_boundary_is_a_boundary() -> Result<(), String> {
        let session = match parsed(&["exec", "--", "sh", "-lc", "x", "--json", "--", "-"])? {
            Invocation::Exec(session) => session,
            other => return Err(format!("`exec` parsed as {other:?}")),
        };
        assert_eq!(
            session.argv,
            ["sh", "-lc", "x", "--json", "--", "-"].map(OsString::from)
        );
        Ok(())
    }

    /// Pins the terminal-allocation tri-state onto the one boolean the wire carries.
    #[test]
    fn terminal_allocation_resolves_to_what_the_wire_asks_for() -> Result<(), String> {
        let pty_of = |rest: &[&str]| match parse(argv(rest), tty())
            .map(|parsed| parsed.invocation)
            .map_err(|error| error.message)
        {
            Ok(Invocation::Exec(session)) => Ok(session.pty),
            Ok(other) => Err(format!("parsed as {other:?}")),
            Err(message) => Err(message),
        };
        assert!(!pty_of(&["exec", "--", "true"])?, "exec defaults to a pty");
        assert!(pty_of(&["exec", "-t", "--", "true"])?);
        assert!(pty_of(&["exec", "--tty", "--", "true"])?);
        assert!(!pty_of(&["exec", "-T", "--", "true"])?);
        assert!(!pty_of(&["exec", "--no-tty", "--", "true"])?);
        // `shell` has no flag to read: spec/12 gives it a pty unconditionally.
        match parsed(&["shell"])? {
            Invocation::Shell(session) => {
                assert!(session.pty);
                assert!(session.argv.is_empty());
            }
            other => return Err(format!("`shell` parsed as {other:?}")),
        }
        Ok(())
    }

    /// Pins both `--env` forms, their order, and the split that separates them.
    ///
    /// `KEY` and `KEY=VAL` mean different things — copy from the host if present, versus supply
    /// this literal — so collapsing them would silently turn a missing host variable into an empty
    /// one. Order is kept because the same name may be given twice and the last one wins.
    #[test]
    fn env_carries_both_forms_in_the_order_given() -> Result<(), String> {
        let session = match parsed(&[
            "exec", "--env", "TERM", "--env", "A=1", "--env", "B=", "--env", "A=2", "--", "true",
        ])? {
            Invocation::Exec(session) => session,
            other => return Err(format!("`exec` parsed as {other:?}")),
        };
        let named =
            |name: &str, value: Option<&str>| (OsString::from(name), value.map(OsString::from));
        assert_eq!(
            session.env,
            vec![
                named("TERM", None),
                named("A", Some("1")),
                // An empty value is a value, not an absent one.
                named("B", Some("")),
                named("A", Some("2")),
            ]
        );

        for rejected in [
            vec!["exec", "--env", "--", "true"], // the separator was eaten as the value
            vec!["exec", "--env", "=1", "--", "true"],
            vec!["exec", "--env", "", "--", "true"],
            vec!["exec", "--env"],
        ] {
            assert!(
                parse(argv(&rejected), tty()).is_err(),
                "accepted a malformed `--env`: {rejected:?}"
            );
        }
        Ok(())
    }

    /// Pins `manifest show`'s exact-one-name arity in both directions.
    #[test]
    fn manifest_show_takes_exactly_one_name() -> Result<(), String> {
        assert!(parse(argv(&["manifest", "show"]), piped()).is_err());
        assert!(parse(argv(&["manifest", "show", "one", "two"]), piped()).is_err());
        assert_eq!(
            parsed(&["manifest", "show", "one", "--json"])?,
            Invocation::ManifestShow {
                name: "one".to_owned(),
                output: Output::Json,
            }
        );
        Ok(())
    }

    /// Pins that an unknown flag is decided before anything is resolved, which is what keeps a
    /// typo a `64` instead of whatever the resolution behind it would have answered.
    #[test]
    fn an_unknown_option_is_rejected_before_resolution() {
        for rejected in [
            vec!["shell", "--unknown"],
            vec!["config", "--unknown"],
            vec!["manifest", "list", "--unknown"],
            vec!["gc", "--unknown"],
        ] {
            assert!(
                parse(argv(&rejected), tty()).is_err(),
                "accepted an unknown option: {rejected:?}"
            );
        }
        assert!(parse(argv(&["nonsense"]), tty()).is_err());
        assert!(parse(argv(&[]), tty()).is_err());
    }

    /// Pins that every published verb now parses into real work — nothing is deferred.
    #[test]
    fn every_verb_parses_into_real_work() {
        assert!(matches!(parsed(&["gc"]), Ok(Invocation::Gc { .. })));
        assert!(matches!(
            parsed(&["volume", "list"]),
            Ok(Invocation::VolumeList { .. })
        ));
        assert!(matches!(
            parsed(&["volume", "prune", "--yes"]),
            Ok(Invocation::VolumePrune { .. })
        ));
        assert!(matches!(
            parsed(&["destroy", "--yes"]),
            Ok(Invocation::Destroy { .. })
        ));
    }

    /// Pins the `generations` family grammar: forms, values, and the exactly-one retention rule.
    #[test]
    fn the_generations_family_parses_and_refuses() {
        assert!(matches!(
            parsed(&["generations", "list"]),
            Ok(Invocation::GenerationsList { .. })
        ));
        assert!(matches!(
            parsed(&["generations", "activate", "3"]),
            Ok(Invocation::GenerationsActivate { number: 3, .. })
        ));
        assert!(matches!(
            parsed(&["generations", "rollback"]),
            Ok(Invocation::GenerationsRollback { .. })
        ));
        assert!(matches!(
            parsed(&["generations", "prune", "--keep", "2"]),
            Ok(Invocation::GenerationsPrune {
                keep: Some(2),
                older_than_seconds: None,
                ..
            })
        ));
        assert!(matches!(
            parsed(&["generations", "prune", "--older-than", "30d"]),
            Ok(Invocation::GenerationsPrune {
                keep: None,
                older_than_seconds: Some(2_592_000),
                ..
            })
        ));
        for rejected in [
            vec!["generations"],                  // no subcommand
            vec!["generations", "bogus"],         // unknown subcommand
            vec!["generations", "activate"],      // no number
            vec!["generations", "activate", "x"], // not a number
            vec!["generations", "prune"],         // no retention argument
            vec!["generations", "prune", "--keep", "1", "--older-than", "1d"], // both
            vec!["generations", "prune", "--keep", "x"], // bad count
            vec!["generations", "prune", "--older-than", "7"], // no unit
            vec!["generations", "prune", "--older-than", "d"], // no count
            vec!["generations", "prune", "--older-than", "7w"], // unknown unit
        ] {
            assert!(
                parse(argv(&rejected), tty()).is_err(),
                "accepted: {rejected:?}"
            );
        }
    }

    /// Pins `start --generation`: the value survives the lift, and the rebuild pair conflicts.
    #[test]
    fn start_generation_takes_a_value_and_conflicts_with_rebuilds() {
        assert!(matches!(
            parsed(&["start", "--generation", "2"]),
            Ok(Invocation::Start {
                generation: Some(2),
                ..
            })
        ));
        // The lift must not read `2` as a verb-local token to inspect: `-v` after the value
        // proves the value was stepped over.
        assert!(matches!(
            parsed(&["start", "--generation", "2", "-v"]),
            Ok(Invocation::Start {
                generation: Some(2),
                ..
            })
        ));
        for rejected in [
            vec!["start", "--generation"],
            vec!["start", "--generation", "x"],
            vec!["start", "--generation", "2", "--rebuild"],
            vec!["start", "--generation", "2", "--no-rebuild"],
        ] {
            assert!(
                parse(argv(&rejected), tty()).is_err(),
                "accepted: {rejected:?}"
            );
        }
    }

    /// `--keep-volumes` was parsed and discarded, which is the shape of bug a flag test catches
    /// and a usage line does not: the invocation accepted it and nothing carried it anywhere.
    #[test]
    fn every_destroy_flag_reaches_the_invocation() {
        assert!(matches!(
            parsed(&["destroy", "--yes", "--keep-volumes"]),
            Ok(Invocation::Destroy {
                keep_volumes: true,
                yes: true,
                ..
            })
        ));
        assert!(matches!(
            parsed(&["destroy", "-f"]),
            Ok(Invocation::Destroy {
                keep_volumes: false,
                yes: true,
                ..
            })
        ));
    }

    /// `prune` shares `destroy`'s consent rail, except that a preview removes nothing and so is
    /// not gated on consent at all — including off a terminal, where the gate would otherwise make
    /// `--dry-run` impossible to run from a script.
    #[test]
    fn a_prune_preview_needs_no_consent_and_a_prune_does() {
        assert!(parse(argv(&["volume", "prune"]), piped()).is_err());
        for preview in [
            vec!["volume", "prune", "-n"],
            vec!["volume", "prune", "--dry-run"],
        ] {
            assert!(matches!(
                parsed(&preview),
                Ok(Invocation::VolumePrune { dry_run: true, .. })
            ));
        }
        assert!(matches!(
            parsed(&["volume", "prune", "--yes"]),
            Ok(Invocation::VolumePrune {
                dry_run: false,
                yes: true,
                ..
            })
        ));
        // On a terminal the prompt is what asks, so the flag is optional there.
        assert!(matches!(
            parse(argv(&["volume", "prune"]), tty()).map(|parsed| parsed.invocation),
            Ok(Invocation::VolumePrune { yes: false, .. })
        ));
    }

    /// Pins `--json` as recognized on every verb that carries a specified JSON shape.
    #[test]
    fn json_is_recognized_wherever_it_is_specified() -> Result<(), String> {
        for rest in [
            vec!["config", "--json"],
            vec!["manifest", "list", "--json"],
            vec!["volume", "list", "--json"],
            vec!["stop", "--json"],
        ] {
            let invocation = parse(argv(&rest), tty())
                .map(|parsed| parsed.invocation)
                .map_err(|error| error.message)?;
            let output = output_of(&invocation)
                .ok_or_else(|| format!("{rest:?} parsed as the private handoff"))?;
            assert!(output.is_json(), "--json was dropped by {rest:?}");
        }
        Ok(())
    }

    /// Pins ADR-0026's placement rule: a global flag reads the same before and after the verb.
    #[test]
    fn a_global_flag_is_positionless() -> Result<(), String> {
        let before = parse(argv(&["-v", "status"]), piped()).map_err(|error| error.message)?;
        let after = parse(argv(&["status", "-v"]), piped()).map_err(|error| error.message)?;
        assert_eq!(before, after);
        assert_eq!(before.verbosity, super::Verbosity::Verbose);
        assert!(matches!(before.invocation, Invocation::Status { .. }));
        Ok(())
    }

    /// Pins stacking, the `-vv` spellings, and last-one-wins between `-q` and `-v` (ADR-0026).
    #[test]
    fn verbosity_stacks_and_the_last_side_wins() -> Result<(), String> {
        use super::Verbosity;
        for (rest, expected) in [
            (vec!["status"], Verbosity::Normal),
            (vec!["status", "-q"], Verbosity::Quiet),
            (vec!["status", "-v", "-v"], Verbosity::Debug),
            (vec!["status", "-vv"], Verbosity::Debug),
            (vec!["status", "-vvv"], Verbosity::Trace),
            (vec!["status", "-v", "-v", "-v", "-v"], Verbosity::Trace),
            (vec!["-v", "status", "-q"], Verbosity::Quiet),
            (vec!["-q", "status", "-v"], Verbosity::Verbose),
        ] {
            let parsed = parse(argv(&rest), piped()).map_err(|error| error.message)?;
            assert_eq!(parsed.verbosity, expected, "{rest:?}");
        }
        Ok(())
    }

    /// Pins the `--` boundary: a guest `-v` is the guest's, byte for byte (spec/12).
    #[test]
    fn the_lift_stops_at_the_guest_boundary() -> Result<(), String> {
        let parsed = parse(argv(&["exec", "-v", "--", "cargo", "build", "-v"]), piped())
            .map_err(|error| error.message)?;
        assert_eq!(parsed.verbosity, super::Verbosity::Verbose);
        let Invocation::Exec(session) = parsed.invocation else {
            return Err("did not parse as exec".to_owned());
        };
        assert_eq!(session.argv, ["cargo", "build", "-v"].map(OsString::from));
        // And help past the boundary is the guest's too.
        let parsed = parse(argv(&["exec", "--", "cargo", "--help"]), piped())
            .map_err(|error| error.message)?;
        assert!(matches!(parsed.invocation, Invocation::Exec(_)));
        Ok(())
    }

    /// Pins the verb-keyed value table: `stop -t` takes seconds, so the lift must not read the
    /// token after it — `viv stop -t -v` stays the arity-shaped error it is, never a quiet parse.
    #[test]
    fn the_lift_never_reads_past_a_value_flag() {
        assert!(parse(argv(&["stop", "-t", "-v"]), tty()).is_err());
        // The same spelling on `exec` is a boolean, so the `-v` beside it really is global.
        let parsed = parse(argv(&["exec", "-t", "-v", "--", "true"]), tty());
        assert!(matches!(
            parsed.map(|parsed| parsed.verbosity),
            Ok(super::Verbosity::Verbose)
        ));
        // And a global between `--manifest` and its value would break the pair; the lift keeps it.
        assert!(matches!(
            parsed_with(&["config", "--manifest", "-v"], piped()),
            Ok(Invocation::Config { manifest: Some(name), .. }) if name == "-v"
        ));
    }

    fn parsed_with(rest: &[&str], streams: Streams) -> Result<Invocation, String> {
        parse(argv(rest), streams)
            .map(|parsed| parsed.invocation)
            .map_err(|error| error.message)
    }

    /// Pins `--help` and `--version` as global, verb-aware, and winning over malformed rests.
    #[test]
    fn help_and_version_answer_before_anything_can_fail() {
        assert!(matches!(
            parsed(&["--help"]),
            Ok(Invocation::Help { verb: None })
        ));
        assert!(matches!(
            parsed(&["start", "--help"]),
            Ok(Invocation::Help { verb: Some(ref name) }) if name == "start"
        ));
        // A malformed rest does not matter: the user asked about the tool, not for it.
        assert!(matches!(
            parsed(&["start", "--bogus", "-h"]),
            Ok(Invocation::Help { verb: Some(ref name) }) if name == "start"
        ));
        assert!(matches!(parsed(&["--version"]), Ok(Invocation::Version)));
        assert!(matches!(
            parsed(&["status", "--version"]),
            Ok(Invocation::Version)
        ));
    }
}
