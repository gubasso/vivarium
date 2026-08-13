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
    /// The binding assistant. Read-only unless `write`.
    Init {
        manifest: Option<String>,
        write: bool,
        yes: bool,
        no_input: bool,
        output: Output,
    },
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
    /// The slice-002 supervisor handoff, which `nix/runner.sh` invokes directly.
    ///
    /// Kept as a sub-form of `start` rather than promoted to its own verb because it is not part of
    /// the published surface: spec/01's `start` is the manifest-driven one, and this is the private
    /// spelling the generated runner uses to hand a resolved specification back to vivarium.
    StartSpec { spec: PathBuf },
    /// A verb this slice parses but does not perform.
    ///
    /// Each is owned by a later slice. They are here because their grammar and their fail-closed
    /// behavior are already contracts — spec/14's matrix commits to `64` for a malformed
    /// invocation and `78` for an unbound project — and those two answers do not depend on the
    /// work behind them existing yet.
    Deferred { verb: Deferred, output: Output },
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

/// The verbs whose grammar is settled here and whose work belongs to slice 014.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Deferred {
    VolumeList,
    Destroy,
    /// The one cross-project sweep. Needs no binding, so it never answers `78`.
    Gc,
}

impl Deferred {
    /// Whether this verb requires a manifest to be bound.
    #[must_use]
    pub const fn needs_binding(self) -> bool {
        // `gc` is a whole-store sweep across every project, so demanding a binding would make an
        // unbound directory refuse a global operation that has nothing to do with it.
        !matches!(self, Self::Gc)
    }

    /// The verb as spelled on the command line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VolumeList => "volume list",
            Self::Destroy => "destroy",
            Self::Gc => "gc",
        }
    }
}

/// What the host can tell a command about its own streams.
///
/// Passed in rather than probed here so the whole grammar stays decidable without a terminal —
/// `-t` off a TTY is a usage error, and a test that could not say so would have to run under a pty
/// to exercise it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Streams {
    pub stdin_is_tty: bool,
}

/// Parses one invocation.
///
/// `argv` includes the program name, as `std::env::args_os` yields it.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown verb, an unknown flag, wrong arity, or two flags that
/// contradict each other. Every one of them is `64`.
pub fn parse<I>(argv: I, streams: Streams) -> Result<Invocation, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let tokens: Vec<OsString> = argv.into_iter().skip(1).collect();
    let Some(verb) = tokens.first() else {
        return Err(UsageError::new("no command given", Some(TOP_USAGE)));
    };
    let rest = &tokens[1..];

    match verb.to_str() {
        Some("init") => init(rest),
        Some("config") => config(rest),
        Some("manifest") => manifest(rest),
        Some("start") => start(rest),
        Some("status") => status(rest),
        Some("shell") => shell(rest),
        Some("exec") => exec(rest, streams),
        Some("stop") => stop(rest),
        Some("volume") => volume(rest),
        Some("destroy") => destroy(rest, streams),
        Some("gc") => deferred_flagless(Deferred::Gc, rest, GC_USAGE),
        _ => Err(UsageError::new(
            format!("unknown command `{}`", verb.to_string_lossy()),
            Some(TOP_USAGE),
        )),
    }
}

const TOP_USAGE: &str =
    "viv <init|config|manifest|start|status|shell|exec|stop|volume|destroy|gc> [options]";
const INIT_USAGE: &str = "viv init [--manifest <name>] [--write] [--yes] [--json] [--no-input]";
const CONFIG_USAGE: &str = "viv config [--manifest <name>] [--json]";
const CONFIG_EVAL_USAGE: &str = "viv config eval [--json]";
const CONFIG_SOURCES_USAGE: &str = "viv config sources [--json]";
const MANIFEST_USAGE: &str = "viv manifest <list|show <name>> [--json]";
const START_USAGE: &str = "viv start [--rebuild|--no-rebuild] [--json]";
const STATUS_USAGE: &str = "viv status [--json] [-g|--global]";
const SHELL_USAGE: &str = "viv shell";
const EXEC_USAGE: &str =
    "viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <command> [args...]";
const STOP_USAGE: &str = "viv stop [--all] [--force] [-t|--timeout <secs>] [--json]";
const VOLUME_USAGE: &str = "viv volume list [--json]";
const DESTROY_USAGE: &str = "viv destroy [-f|--yes] [--keep-volumes] [--json]";
const GC_USAGE: &str = "viv gc [--json]";

fn init(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let mut manifest = None;
    let (mut write, mut yes, mut no_input) = (false, false, false);
    let mut output = Output::Human;
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        match token.to_str() {
            Some("--manifest") => manifest = Some(value(&mut tokens, "--manifest", INIT_USAGE)?),
            Some("--write") => write = true,
            Some("--yes" | "-y") => yes = true,
            Some("--no-input") => no_input = true,
            Some("--json") => output = Output::Json,
            _ => return Err(unknown(token, INIT_USAGE)),
        }
    }
    Ok(Invocation::Init {
        manifest,
        write,
        yes,
        no_input,
        output,
    })
}

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
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("--rebuild") => rebuild = true,
            Some("--no-rebuild") => no_rebuild = true,
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
    Ok(Invocation::Start {
        rebuild,
        no_rebuild,
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

fn volume(rest: &[OsString]) -> Result<Invocation, UsageError> {
    let Some(sub) = rest.first() else {
        return Err(UsageError::new(
            "`viv volume` needs a subcommand",
            Some(VOLUME_USAGE),
        ));
    };
    match sub.to_str() {
        Some("list") => {
            let output = flags_only(&rest[1..], VOLUME_USAGE)?;
            Ok(Invocation::Deferred {
                verb: Deferred::VolumeList,
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
    let mut yes = false;
    let mut output = Output::Human;
    for token in rest {
        match token.to_str() {
            Some("--yes" | "-f") => yes = true,
            Some("--keep-volumes") => {}
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
    Ok(Invocation::Deferred {
        verb: Deferred::Destroy,
        output,
    })
}

fn deferred_flagless(
    verb: Deferred,
    rest: &[OsString],
    usage: &'static str,
) -> Result<Invocation, UsageError> {
    let output = flags_only(rest, usage)?;
    Ok(Invocation::Deferred { verb, output })
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

    use super::{Deferred, Invocation, Output, Streams, parse};

    fn argv(rest: &[&str]) -> Vec<OsString> {
        std::iter::once("viv")
            .chain(rest.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn tty() -> Streams {
        Streams { stdin_is_tty: true }
    }

    fn piped() -> Streams {
        Streams {
            stdin_is_tty: false,
        }
    }

    fn parsed(rest: &[&str]) -> Result<Invocation, String> {
        parse(argv(rest), piped()).map_err(|error| error.message)
    }

    /// The `--json` slot of whichever invocation carries one.
    const fn output_of(invocation: &Invocation) -> Option<Output> {
        match invocation {
            Invocation::Config { output, .. }
            | Invocation::ConfigEval { output }
            | Invocation::ConfigSources { output }
            | Invocation::ManifestList { output }
            | Invocation::ManifestShow { output, .. }
            | Invocation::Init { output, .. }
            | Invocation::Start { output, .. }
            | Invocation::Status { output, .. }
            | Invocation::Stop { output, .. }
            | Invocation::Deferred { output, .. } => Some(*output),
            // Neither carries a `--json` slot: the private handoff predates the published surface,
            // and the two session verbs hand their streams to a guest process.
            Invocation::StartSpec { .. } | Invocation::Exec(_) | Invocation::Shell(_) => None,
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
        let pty_of = |rest: &[&str]| match parse(argv(rest), tty()).map_err(|error| error.message) {
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
            vec!["init", "--unknown"],
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

    /// Pins `gc` as the one deferred verb that needs no binding, so it cannot answer `78`.
    #[test]
    fn only_gc_is_exempt_from_needing_a_binding() {
        assert!(!Deferred::Gc.needs_binding());
        for verb in [Deferred::VolumeList, Deferred::Destroy] {
            assert!(
                verb.needs_binding(),
                "{} should need a binding",
                verb.as_str()
            );
        }
    }

    /// Pins `--json` as recognized on every verb that carries a specified JSON shape.
    #[test]
    fn json_is_recognized_wherever_it_is_specified() -> Result<(), String> {
        for rest in [
            vec!["config", "--json"],
            vec!["manifest", "list", "--json"],
            vec!["init", "--json"],
            vec!["volume", "list", "--json"],
            vec!["stop", "--json"],
        ] {
            let invocation = parse(argv(&rest), tty()).map_err(|error| error.message)?;
            let output = output_of(&invocation)
                .ok_or_else(|| format!("{rest:?} parsed as the private handoff"))?;
            assert!(output.is_json(), "--json was dropped by {rest:?}");
        }
        Ok(())
    }
}
