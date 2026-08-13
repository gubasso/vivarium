//! `viv exec` and `viv shell`: the two verbs that reach into a running guest.
//!
//! Separated from [`super::run`] because a session is the one thing the synchronous command surface
//! cannot express. Its result is the guest's own exit status rather than a category from vivarium's
//! taxonomy, its output is bytes on the host's own streams rather than a string vivarium
//! accumulates and prints, and its I/O is concurrent in both directions for as long as the guest
//! lives. `Invocation::StartSpec` already established the shape this uses: the process boundary
//! dispatches what needs a runtime, and everything else stays synchronous.
//!
//! What this file owns is the host side of a session — the environment policy, the terminal, and
//! the mapping from what the agent reported back to what `$?` will say. The wire itself is
//! [`crate::launch::control`]'s, and deciding that a VM is there to talk to is
//! [`super::lifecycle::ensure_running`]'s.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt as _;

use tokio::sync::mpsc;

use super::lifecycle::{self, Prepared};
use super::{Context, Failure, diagnosed};
use crate::cli::grammar::Session as Requested;
use crate::config::Environment;
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::launch::GuestSession;
use crate::launch::control::{Session, SessionError, SessionOutcome};
use crate::protocol::{
    EnvironmentVariable, STREAM_PAYLOAD_MAX, SessionMode, StartRequest, TerminalSize, UnixBytes,
    path_to_unix_bytes,
};

/// The host variables that cross into the guest without being named (spec/12).
///
/// Deny-by-default is the rule and this is the whole of the exception: variables that describe the
/// terminal the user is sitting at, which the guest cannot learn any other way. Nothing here can
/// carry a credential or a host path, which is what makes a fixed list safe to apply silently —
/// and why `SSH_AUTH_SOCK` is not on it and naming it is not a way onto it.
const ALLOWLIST: [&str; 5] = ["TERM", "COLORTERM", "NO_COLOR", "FORCE_COLOR", "LANG"];

/// The one prefix rule, for the locale category variables spec/12 admits as a family.
const ALLOWLIST_PREFIX: &str = "LC_";

/// How long a session waits for the control socket to complete a connection.
///
/// Short, because ensure-running has already proved the agent answers: reaching this at all means
/// the VM went away between the handshake and now, and spec/12 requires a stated reason for that
/// rather than a wait.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Runs one session and returns what the host process should exit with.
///
/// # Errors
///
/// Returns [`Failure`] for everything that happens before a guest process could exist: an unbound
/// project, a host that cannot boot, a contended startup, an agent that will not answer, a control
/// socket that fails as a channel. Once a guest process exists, its status is an `Ok` — a guest
/// that failed is not vivarium failing, and that boundary is the whole of spec/12's exit contract.
pub async fn run<E: Environment + Sync>(
    requested: &Requested,
    mode: SessionMode,
    context: &Context<'_, E>,
    host_environment: &[(OsString, OsString)],
) -> Result<ExitKind, Failure> {
    let prepared = lifecycle::ensure_running(context).await?;
    let start = StartRequest {
        mode,
        argv: requested
            .argv
            .iter()
            .map(|argument| UnixBytes::new(argument.as_bytes().to_vec()))
            .collect(),
        environment: guest_environment(&prepared.session, host_environment, &requested.env),
        cwd: path_to_unix_bytes(&prepared.workspace_cwd),
        pty: requested.pty,
    };

    // The terminal is put in raw mode before the first byte moves, and restored by the guard's own
    // drop on every path out of this function — including the one a signal takes, which is why the
    // signal is watched below rather than left to its default disposition.
    let terminal = requested.pty.then(Terminal::enter).flatten();
    let session = Session {
        control_socket: &prepared.control_socket,
        metadata: &prepared.boot,
        start,
        initial_size: terminal.as_ref().and_then(|_| window_size()),
        input: Some(spawn_input()),
        resize: terminal.as_ref().and_then(|_| spawn_resize()),
        stdout: tokio::io::stdout(),
        stderr: tokio::io::stderr(),
        connect_timeout: CONNECT_TIMEOUT,
    };

    let outcome = tokio::select! {
        outcome = session.run() => outcome,
        signal = terminating_signal() => {
            // Restoring here rather than only on drop is not redundant: it puts the terminal back
            // before anything else this function might print, so a diagnostic is never written to
            // a terminal that is still raw.
            drop(terminal);
            return Ok(ExitKind::GuestStatus(signal));
        }
    };
    drop(terminal);

    match outcome.map_err(|error| refuse(&prepared, error))? {
        // spec/12: the guest's status is returned verbatim for `0..255`, and a guest killed by
        // signal `S` yields `128+S` — which the agent has already computed, so this is the byte it
        // sent and not a second derivation of it.
        SessionOutcome::Exited(status) => Ok(ExitKind::GuestStatus(status)),
        SessionOutcome::Refused(code) => Err(refused(&code)),
        SessionOutcome::Lost(expected) => Err(diagnosed(
            Namespace::Vm,
            "session-transport-lost",
            "the connection to the guest ended before the command's status was known",
            Locus::Named("control socket"),
            format!("the session ended while waiting for {expected}"),
            // spec/12 is explicit that a transport death after guest start is `74` and that no
            // guest-shaped code may be guessed for it: the command may well have succeeded, and
            // reporting a number for it would be inventing the one fact that was lost.
            ExitKind::IoErr,
        )),
    }
}

/// The environment one guest process runs with, in the order the guest applies it.
///
/// Three layers, and the order between them is the contract. The guest's own defaults come first
/// because they are facts of the image rather than anything the host offered. The fixed allowlist
/// comes next, because a terminal description is a host fact the guest cannot have. `--env` comes
/// last, because naming a variable explicitly is the strongest thing a caller can say about it.
///
/// Nothing else crosses. The guest agent clears the environment before every spawn, so this list is
/// exhaustive rather than additive — a host variable absent from it is absent in the guest, which
/// is what makes deny-by-default true rather than merely intended.
fn guest_environment(
    defaults: &GuestSession,
    host: &[(OsString, OsString)],
    requested: &[(OsString, Option<OsString>)],
) -> Vec<EnvironmentVariable> {
    let mut environment = vec![
        variable("PATH", defaults.path.as_bytes()),
        variable("HOME", defaults.home.as_os_str().as_bytes()),
        variable("USER", defaults.user.as_bytes()),
        variable("LOGNAME", defaults.user.as_bytes()),
        variable("SHELL", defaults.shell.as_os_str().as_bytes()),
    ];
    for (name, value) in host {
        if allowed(name) {
            environment.push(pair(name, value));
        }
    }
    for (name, value) in requested {
        match value {
            Some(value) => environment.push(pair(name, value)),
            // The `--env KEY` form copies the host's value when there is one. A name the host does
            // not define contributes nothing rather than an empty string, because an empty value is
            // a value and a program that tests for one would read the two differently.
            None => {
                if let Some((_, found)) = host.iter().find(|(candidate, _)| candidate == name) {
                    environment.push(pair(name, found));
                }
            }
        }
    }
    environment
}

fn allowed(name: &OsStr) -> bool {
    name.as_bytes().starts_with(ALLOWLIST_PREFIX.as_bytes())
        || ALLOWLIST.iter().any(|allowed| name == OsStr::new(allowed))
}

fn variable(name: &str, value: &[u8]) -> EnvironmentVariable {
    EnvironmentVariable {
        name: UnixBytes::new(name.as_bytes().to_vec()),
        value: UnixBytes::new(value.to_vec()),
    }
}

fn pair(name: &OsStr, value: &OsStr) -> EnvironmentVariable {
    EnvironmentVariable {
        name: UnixBytes::new(name.as_bytes().to_vec()),
        value: UnixBytes::new(value.as_bytes().to_vec()),
    }
}

/// The host terminal's settings, restored when this is dropped.
///
/// A guard rather than a pair of calls because restoration is an acceptance assertion rather than a
/// cleanup detail: a terminal left in raw mode outlives the process that broke it, and the user's
/// next shell is the one that pays.
struct Terminal(rustix::termios::Termios);

impl Terminal {
    /// Puts the host terminal in raw mode, or does nothing when there is no terminal to put.
    ///
    /// Raw mode is what makes the interrupt work correctly: the host stops interpreting the byte
    /// and forwards it, and the guest's own line discipline raises the signal against the guest's
    /// own foreground process group. Synthesizing a signal instead would deliver it to whatever the
    /// host happened to think was in front, which under a job-control shell is the wrong process.
    fn enter() -> Option<Self> {
        let stdin = std::io::stdin();
        let original = rustix::termios::tcgetattr(&stdin).ok()?;
        let mut raw = original.clone();
        raw.make_raw();
        rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Flush, &raw).ok()?;
        Some(Self(original))
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = rustix::termios::tcsetattr(
            std::io::stdin(),
            rustix::termios::OptionalActions::Flush,
            &self.0,
        );
    }
}

/// The host terminal's current size, when it has one.
fn window_size() -> Option<TerminalSize> {
    let size = rustix::termios::tcgetwinsize(std::io::stdin()).ok()?;
    Some(TerminalSize {
        rows: size.ws_row,
        columns: size.ws_col,
    })
}

/// Host standard input, read into chunks the encoder will accept.
///
/// Not cancel-safe and therefore not a `select!` arm: an abandoned read would drop bytes the user
/// has already typed. So it has a reader of its own — and that reader is an ordinary thread rather
/// than a task, which is the part that is load-bearing rather than stylistic.
///
/// The async runtime waits for its blocking pool when it shuts down. A terminal never reaches
/// end-of-file, so a read parked on one would still be parked when the session ended, and the
/// process would sit there holding a restored terminal and a finished session forever. Measured:
/// `viv shell` completed its work and never exited, which the interactive trial found as a run that
/// timed out rather than as a session that failed. A detached thread is not the runtime's to wait
/// for, so the process leaves when its work is done and the kernel reclaims the read.
fn spawn_input() -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel(4);
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut stdin = std::io::stdin().lock();
        let mut buffer = vec![0u8; STREAM_PAYLOAD_MAX];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => {
                    if sender.blocking_send(buffer[..read].to_vec()).is_err() {
                        return;
                    }
                }
            }
        }
    });
    receiver
}

/// Every later host resize, so the guest's PTY follows the window rather than the boot.
fn spawn_resize() -> Option<mpsc::Receiver<TerminalSize>> {
    let mut winch =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change()).ok()?;
    let (sender, receiver) = mpsc::channel(1);
    tokio::spawn(async move {
        while winch.recv().await.is_some() {
            let Some(size) = window_size() else { return };
            if sender.send(size).await.is_err() {
                return;
            }
        }
    });
    Some(receiver)
}

/// Resolves when a signal that would otherwise end this process arrives, yielding `128+S`.
///
/// Watched rather than left to the default disposition for one reason: the default kills the
/// process, and a process killed mid-session never restores the terminal it made raw. The guest end
/// needs nothing from this — the agent terminates a session whose client disconnects — so the whole
/// job here is to unwind the host side and report the way a shell reports a signalled child.
async fn terminating_signal() -> u8 {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut interrupt), Ok(mut quit), Ok(mut terminate), Ok(mut hangup)) = (
        signal(SignalKind::interrupt()),
        signal(SignalKind::quit()),
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
    ) else {
        // A host that will not let these be watched still runs sessions; it only loses the tidy
        // unwind. Parking here leaves the default disposition in place rather than failing a
        // command over its own cleanup.
        return std::future::pending().await;
    };
    // `128+S`, the same arithmetic a shell reports a signalled child with, and the same one the
    // guest agent applies on its side of the wire.
    tokio::select! {
        _ = interrupt.recv() => 128 + 2,
        _ = quit.recv() => 128 + 3,
        _ = hangup.recv() => 128 + 1,
        _ = terminate.recv() => 128 + 15,
    }
}

/// Everything that went wrong before a guest process could exist.
fn refuse(prepared: &Prepared, error: SessionError) -> Failure {
    match error {
        SessionError::NotListening => diagnosed(
            Namespace::Vm,
            "control-socket-unavailable",
            "the guest is not accepting connections",
            Locus::File(prepared.control_socket.clone()),
            "the control socket did not complete a connection, so no session could be opened",
            ExitKind::Unavailable,
        )
        .with_hint("run `viv status` to see what this project's VM is doing"),
        SessionError::Identity => diagnosed(
            Namespace::Vm,
            "boot-identity-mismatch",
            "the guest that answered belongs to a different boot",
            Locus::File(prepared.control_socket.clone()),
            "the agent's identity did not match the boot record for this target, so the \
            socket is stale or the VM was replaced between the check and the session",
            ExitKind::Unavailable,
        )
        .with_hint("run `viv stop` and start again"),
        SessionError::Transport(step) => diagnosed(
            Namespace::Vm,
            "control-socket-failed",
            "the connection to the guest failed",
            Locus::File(prepared.control_socket.clone()),
            format!("the control socket failed while trying to {step}"),
            ExitKind::IoErr,
        ),
    }
}

/// The agent's own closed refusal vocabulary, mapped onto the taxonomy.
///
/// Fixed words rather than a message, deliberately: the underlying failure can name bytes of a
/// request that are secret-class, so the agent reports a category and nothing else. Vivarium
/// therefore classifies rather than quotes.
fn refused(code: &str) -> Failure {
    let (id, what, why, kind) = match code {
        "spawn" => (
            "spawn-failed",
            "the guest could not start the command",
            concat!(
                "the guest agent reported that the process could not be spawned: the ",
                "command may not exist in the guest, may not be executable, or the ",
                "working directory may be missing",
            )
            .to_owned(),
            // spec/12 reserves `127` and `126` for a later refinement of exactly this case, so it
            // is reported as the agent being unable to perform the request rather than pre-empting
            // those numbers with a guess about which of the three it was.
            ExitKind::Unavailable,
        ),
        "authorization" => (
            "unauthorized",
            "the guest agent refused the connection",
            "the agent did not accept the boot identity this session presented".to_owned(),
            ExitKind::Unavailable,
        ),
        other => (
            "protocol-fault",
            "the guest agent and vivarium disagreed about the session",
            // The raw word is carried in the explanation rather than in the identifier, because a
            // diagnostic id is a stable name a consumer may match on and the agent's vocabulary is
            // its own to extend.
            format!(
                "the agent reported `{other}`, which is a defect in one of the two ends {}",
                "rather than a condition the invocation can correct",
            ),
            ExitKind::Software,
        ),
    };
    diagnosed(
        Namespace::Guest,
        id,
        what,
        Locus::Named("guest agent"),
        why,
        kind,
    )
    .with_hint("run `viv status` to see what this project's VM is doing")
}

#[cfg(test)]
mod tests {
    use super::{ALLOWLIST, guest_environment};
    use crate::launch::GuestSession;
    use std::ffi::OsString;

    fn defaults() -> GuestSession {
        GuestSession {
            user: "vivarium".to_owned(),
            home: "/home/vivarium".into(),
            shell: "/run/current-system/sw/bin/bash".into(),
            path: "/run/wrappers/bin:/run/current-system/sw/bin".to_owned(),
        }
    }

    fn host(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .collect()
    }

    fn rendered(
        host: &[(OsString, OsString)],
        requested: &[(OsString, Option<OsString>)],
    ) -> Vec<(String, String)> {
        guest_environment(&defaults(), host, requested)
            .into_iter()
            .map(|variable| {
                (
                    String::from_utf8_lossy(variable.name.as_bytes()).into_owned(),
                    String::from_utf8_lossy(variable.value.as_bytes()).into_owned(),
                )
            })
            .collect()
    }

    /// Pins deny-by-default as an exhaustive list rather than an intention.
    ///
    /// The guest agent clears the environment and applies exactly what the request carries, so a
    /// name absent from this list is absent in the guest. That makes the assertion that matters an
    /// assertion about what is *not* here.
    #[test]
    fn no_host_variable_crosses_unless_it_is_named() {
        let environment = rendered(
            &host(&[
                ("TERM", "xterm-256color"),
                ("LC_ALL", "C"),
                ("AWS_SECRET_ACCESS_KEY", "must-not-cross"),
                ("SSH_AUTH_SOCK", "/host/path/must-not-cross"),
                ("HOME", "/home/host-user"),
                ("PATH", "/host/bin"),
            ]),
            &[],
        );
        let names: Vec<&str> = environment.iter().map(|(name, _)| name.as_str()).collect();
        assert!(!names.contains(&"AWS_SECRET_ACCESS_KEY"));
        // spec/12 is explicit that agent forwarding is not an exception to the rule. The variable
        // names a host socket a guest `connect()` cannot reach, so forwarding it would hand the
        // guest a name for nothing; the agent sets its own value from the relay.
        assert!(!names.contains(&"SSH_AUTH_SOCK"));
        assert!(names.contains(&"TERM"));
        assert!(names.contains(&"LC_ALL"));
        // The host's own `HOME` and `PATH` are not forwarded; the guest's are supplied instead.
        assert_eq!(
            environment
                .iter()
                .filter(|(name, _)| name == "HOME" || name == "PATH")
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            vec![
                "/run/wrappers/bin:/run/current-system/sw/bin",
                "/home/vivarium"
            ]
        );
    }

    /// Pins that the guest's own defaults are present and complete.
    ///
    /// `PATH` in particular: the agent inherits nothing, so a session without one cannot resolve a
    /// bare program name and a plain `viv exec -- true` would fail as a missing command.
    #[test]
    fn the_guest_supplies_what_no_host_variable_could() {
        let environment = rendered(&[], &[]);
        for (name, value) in [
            ("PATH", "/run/wrappers/bin:/run/current-system/sw/bin"),
            ("HOME", "/home/vivarium"),
            ("USER", "vivarium"),
            ("LOGNAME", "vivarium"),
            ("SHELL", "/run/current-system/sw/bin/bash"),
        ] {
            assert!(
                environment.contains(&(name.to_owned(), value.to_owned())),
                "the guest session is missing {name}"
            );
        }
    }

    /// Pins the two `--env` forms and that the last word on a name is the one that was named.
    #[test]
    fn named_variables_win_and_the_bare_form_copies_only_what_exists() {
        let requested = [
            (OsString::from("TERM"), Some(OsString::from("dumb"))),
            (OsString::from("PRESENT"), None),
            (OsString::from("ABSENT"), None),
            (OsString::from("EMPTY"), Some(OsString::new())),
        ];
        let environment = rendered(
            &host(&[("TERM", "xterm-256color"), ("PRESENT", "from-host")]),
            &requested,
        );
        // The guest applies these in order and the last assignment wins, so `--env TERM=dumb`
        // overriding the allowlist copy is the order doing its job rather than a duplicate.
        assert_eq!(
            environment.last(),
            Some(&("EMPTY".to_owned(), String::new()))
        );
        let terms: Vec<&str> = environment
            .iter()
            .filter(|(name, _)| name == "TERM")
            .map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(terms, vec!["xterm-256color", "dumb"]);
        assert!(environment.contains(&("PRESENT".to_owned(), "from-host".to_owned())));
        assert!(!environment.iter().any(|(name, _)| name == "ABSENT"));
    }

    /// Pins the allowlist itself against spec/12, including that it is a list and one prefix.
    #[test]
    fn the_allowlist_is_the_one_spec_12_fixes() {
        assert_eq!(
            ALLOWLIST,
            ["TERM", "COLORTERM", "NO_COLOR", "FORCE_COLOR", "LANG"]
        );
        let environment = rendered(
            &host(&[("LC_TIME", "C"), ("LCD", "not-a-locale"), ("LC_", "edge")]),
            &[],
        );
        let names: Vec<&str> = environment.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"LC_TIME"));
        assert!(names.contains(&"LC_"));
        // The prefix is `LC_`, not `LC`: a name that merely starts with those two letters is an
        // ordinary host variable and stays behind.
        assert!(!names.contains(&"LCD"));
    }
}
