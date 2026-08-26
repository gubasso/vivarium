//! `viv start --attach` — the launch-time console stream (spec/10, spec/16).
//!
//! The stream follows the supervisor-owned `console.log`, never the serial socket: the
//! supervisor is the socket's one reader, and the file is the same capture it tees to the log
//! (spec/16), so a follower loses nothing and races nobody. The follower replays what capture
//! already holds, keeps byte-for-byte continuity across the supervisor's rotation, and calls
//! the stream ended only after the VM left its running states and the file stayed quiet for a
//! bounded run of polls — never on one racy read.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::lifecycle::{self, Runtime, StartOutcome, State};
use super::{Context, Failure, diagnosed, session};
use crate::config::Environment;
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;

/// How often the follower looks for appended bytes.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Quiet polls, after the VM left its running states, before EOF is called stable.
///
/// The supervisor's final buffered write can land after the unit already reads as gone, so one
/// empty read is a race, not an end; a bounded run of them, followed by one last drain, is.
const STABLE_EOF_POLLS: u32 = 3;

/// One follow of the console capture.
struct Follower {
    path: PathBuf,
    open: Option<Opened>,
}

/// An open generation of the capture, identified so a rotation is detectable.
struct Opened {
    file: std::fs::File,
    /// `(device, inode)`: the open file's identity, compared against the path on every EOF —
    /// Linux keeps a renamed file readable through the held descriptor, so identity drift is
    /// exactly a rotation.
    identity: (u64, u64),
}

impl Follower {
    /// Follows `path` from wherever it stands right now.
    ///
    /// A file that already exists is a previous boot's capture — the sink appends — so its
    /// bytes are skipped: replaying a console this boot never wrote would attribute another
    /// boot's output to it. The capture this boot establishes is then streamed from its first
    /// byte, because the follower is in place before the launch creates it (spec/16 establishes
    /// capture before the VM starts).
    fn resuming(path: PathBuf) -> Self {
        use std::io::{Seek as _, SeekFrom};
        let mut follower = Self { path, open: None };
        if let Ok(mut file) = std::fs::File::open(&follower.path)
            && file.seek(SeekFrom::End(0)).is_ok()
            && let Ok(metadata) = file.metadata()
        {
            follower.open = Some(Opened {
                file,
                identity: identity(&metadata),
            });
        }
        follower
    }

    /// Forwards every byte appended since the last poll to `sink`, reopening across a rotation,
    /// and returns the count forwarded.
    ///
    /// # Errors
    ///
    /// Propagates the sink's write errors and the capture's read errors; a path that does not
    /// exist yet, or was torn down, is quiet rather than an error.
    fn poll(&mut self, sink: &mut dyn std::io::Write) -> std::io::Result<u64> {
        use std::io::Read as _;
        let mut forwarded: u64 = 0;
        loop {
            if self.open.is_none() {
                match std::fs::File::open(&self.path) {
                    Ok(file) => {
                        let metadata = file.metadata()?;
                        self.open = Some(Opened {
                            file,
                            identity: identity(&metadata),
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                    Err(error) => return Err(error),
                }
            }
            let Some(opened) = self.open.as_mut() else {
                break;
            };
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                let count = opened.file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                sink.write_all(&buffer[..count])?;
                forwarded += count as u64;
            }
            // At EOF: the writer stopped appending to this generation the moment it renamed it,
            // so a drained descriptor plus an identity check loses nothing across a rotation.
            match std::fs::metadata(&self.path) {
                Ok(metadata) if identity(&metadata) == opened.identity => break,
                Ok(_) => {
                    // Rotated: this generation is drained; reopen the new one and drain it too.
                    self.open = None;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // Torn down: the boot ended and the sweep took the capture with it.
                    self.open = None;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if forwarded > 0 {
            sink.flush()?;
        }
        Ok(forwarded)
    }
}

/// The stable identity a rotation changes.
fn identity(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt as _;
    (metadata.dev(), metadata.ino())
}

/// What ended a stream that was not cancelled.
enum StreamEnd {
    /// The VM left its running states and the capture stayed quiet: spec/10's "the stream ends".
    Ended,
    /// The caller cancelled: a detach, with one final drain behind it.
    Detached,
}

/// The stream itself, spawned before the launch so capture is consumed from its first byte.
async fn stream(
    mut follower: Follower,
    runtime: Runtime,
    booted: Arc<AtomicBool>,
    cancel: CancellationToken,
) -> std::io::Result<StreamEnd> {
    let mut sink = std::io::stdout();
    let mut quiet: u32 = 0;
    loop {
        let forwarded = follower.poll(&mut sink)?;
        if forwarded > 0 {
            quiet = 0;
        } else if booted.load(Ordering::Acquire) {
            // End-detection waits for the boot's conclusion: before it, "not running" is only a
            // boot that has not reached its unit yet, and ending there would detach from every
            // first build.
            if matches!(
                lifecycle::discriminate(&runtime, true),
                State::Running | State::Starting | State::Stopping
            ) {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet >= STABLE_EOF_POLLS {
                    // One last drain after the stable EOF, so a final write that raced the last
                    // poll still reaches the terminal.
                    follower.poll(&mut sink)?;
                    return Ok(StreamEnd::Ended);
                }
            }
        }
        tokio::select! {
            () = cancel.cancelled() => {
                let _ = follower.poll(&mut sink);
                return Ok(StreamEnd::Detached);
            }
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
}

/// Takes the spawned stream task out of the slot the boot observer filled.
fn take_task(
    slot: &Mutex<Option<JoinHandle<std::io::Result<StreamEnd>>>>,
) -> Option<JoinHandle<std::io::Result<StreamEnd>>> {
    slot.lock().ok().and_then(|mut guard| guard.take())
}

/// `viv start --attach`: the whole detached pipeline, then the console stream (spec/10).
///
/// Returns when the stream ends, and guarantees nothing about the VM at that moment — a guest
/// that powered itself off ends the stream rather than being reported running. `SIGINT`
/// detaches at `0` and leaves the VM untouched; the other terminating signals report `128+S`
/// the way a session does.
///
/// # Errors
///
/// The detached form's ([`super::lifecycle::start`] documents the codes), plus a console stream
/// that failed to read (`74`). A closed stdout is a reader that left, not a failure.
pub async fn start_attached<E: Environment + Sync>(
    context: &Context<'_, E>,
    rebuild: bool,
    no_rebuild: bool,
    generation: Option<u64>,
) -> Result<ExitKind, Failure> {
    // Installed before the blocking boot: a terminating signal that lands mid-boot is consumed
    // here and answered the moment the stream phase opens, rather than killing a launch that
    // spec/10 says a detach never stops. The cost is stated: aborting an attached start's build
    // takes a second signal source (`SIGKILL`), not `Ctrl-C`.
    let mut signals = session::TerminatingSignals::install();
    let cancel = CancellationToken::new();
    let booted = Arc::new(AtomicBool::new(false));
    let stream_task: Arc<Mutex<Option<JoinHandle<std::io::Result<StreamEnd>>>>> =
        Arc::new(Mutex::new(None));

    let observer = {
        let cancel = cancel.clone();
        let booted = Arc::clone(&booted);
        let stream_task = Arc::clone(&stream_task);
        move |runtime: &Runtime| {
            let handle = tokio::spawn(stream(
                Follower::resuming(runtime.console_log()),
                runtime.clone(),
                Arc::clone(&booted),
                cancel.clone(),
            ));
            if let Ok(mut slot) = stream_task.lock() {
                *slot = Some(handle);
            }
        }
    };

    // The pipeline is synchronous and blocks through the boot; `block_in_place` tells the
    // runtime so the stream task keeps running beside it. The follower is spawned by the
    // observer the moment a boot is certain, which is what makes the attached form stream-driven
    // during the boot rather than silent until readiness.
    let outcome = tokio::task::block_in_place(|| {
        lifecycle::perform_start(context, rebuild, no_rebuild, generation, Some(&observer))
    });

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(failure) => {
            cancel.cancel();
            if let Some(handle) = take_task(&stream_task) {
                let _ = handle.await;
            }
            return Err(failure);
        }
    };

    match outcome {
        StartOutcome::AlreadyRunning { notes } => {
            // The launch-time stream attaches to a boot this command performed; reaching a VM
            // that was already up is the slice's recorded out-of-scope, said rather than done.
            cancel.cancel();
            if let Some(handle) = take_task(&stream_task) {
                let _ = handle.await;
            }
            context.ui.note(&format!(
                "{notes}`--attach` streams only a boot this command performs, so nothing was \
                attached\n"
            ));
            // The eager watchers consumed any signal that landed while the pipeline blocked, so
            // a path that never reaches the stream's own select must still answer it: swallowing
            // a `SIGTERM` and exiting `0` would report success for a command that was told to
            // die. A pending `SIGINT` maps to success, which this no-stream outcome already is.
            if let Some(signal) = pending_signal(&mut signals).await {
                return Ok(signal_exit(signal));
            }
            Ok(ExitKind::Success)
        }
        StartOutcome::Booted { resources } => {
            context.ui.outro(&format!(
                "running · {} MiB · {} vcpu · streaming the console; Ctrl-C detaches",
                resources.mem_mib, resources.vcpu
            ));
            booted.store(true, Ordering::Release);
            let Some(mut handle) = take_task(&stream_task) else {
                // The observer fires on every path that reaches a boot, so this arm is
                // unreachable in practice; an empty slot still ends cleanly rather than hanging.
                return Ok(ExitKind::Success);
            };
            tokio::select! {
                joined = &mut handle => stream_exit(joined),
                signal = signals.recv() => {
                    cancel.cancel();
                    let _ = handle.await;
                    Ok(signal_exit(signal))
                }
            }
        }
    }
}

/// The attached form's exit for a terminating signal: `SIGINT` is the designed detach gesture
/// and this form's success (spec/10 — the console closes, the VM keeps running); every other
/// terminating signal keeps a signal's meaning, `128+S`, the way a session reports them.
const fn signal_exit(signal: u8) -> ExitKind {
    if signal == 128 + 2 {
        ExitKind::Success
    } else {
        ExitKind::GuestStatus(signal)
    }
}

/// A terminating signal that arrived while the pipeline blocked, consumed by the eager
/// watchers instead of the default disposition; `None` when none is pending.
///
/// A zero timeout polls the watchers exactly once: a coalesced signal is ready immediately, and
/// anything else answers `None` rather than waiting for one.
async fn pending_signal(signals: &mut session::TerminatingSignals) -> Option<u8> {
    tokio::time::timeout(Duration::ZERO, signals.recv())
        .await
        .ok()
}

/// Maps the finished stream task to the attached form's exit.
fn stream_exit(
    joined: Result<std::io::Result<StreamEnd>, tokio::task::JoinError>,
) -> Result<ExitKind, Failure> {
    match joined {
        Ok(Ok(StreamEnd::Ended | StreamEnd::Detached)) => Ok(ExitKind::Success),
        // A closed stdout is the reader leaving — the pipe's end of a detach, not a failure.
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(ExitKind::Success),
        Ok(Err(error)) => Err(diagnosed(
            Namespace::Vm,
            "console-stream",
            "the console stream could not be read",
            Locus::Named("console capture"),
            error.to_string(),
            ExitKind::IoErr,
        )),
        Err(join_error) => Err(diagnosed(
            Namespace::Internal,
            "console-stream-task",
            "the console stream task ended abnormally",
            Locus::Named("console capture"),
            join_error.to_string(),
            ExitKind::Software,
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Follower, pending_signal, signal_exit};
    use crate::exit::ExitKind;
    use crate::test_support::ScratchDirectory;
    use std::fs;
    use std::io::Write as _;
    use std::time::Duration;

    /// A capture that appears after the follower is in place replays from its first byte, and
    /// later appends arrive on the next poll.
    #[test]
    fn a_fresh_capture_replays_from_zero_then_follows_appends() {
        let scratch = ScratchDirectory::new().unwrap();
        let path = scratch.path().join("console.log");
        let mut follower = Follower::resuming(path.clone());
        let mut sink = Vec::new();
        assert_eq!(follower.poll(&mut sink).unwrap(), 0);

        fs::write(&path, b"boot bytes").unwrap();
        assert_eq!(follower.poll(&mut sink).unwrap(), 10);
        assert_eq!(sink, b"boot bytes");

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b" and more").unwrap();
        follower.poll(&mut sink).unwrap();
        assert_eq!(sink, b"boot bytes and more");
    }

    /// A capture that already exists at spawn is a previous boot's: its bytes are skipped, and
    /// only what this boot appends is streamed.
    #[test]
    fn resuming_skips_a_previous_boots_bytes() {
        let scratch = ScratchDirectory::new().unwrap();
        let path = scratch.path().join("console.log");
        fs::write(&path, b"stale boot\n").unwrap();
        let mut follower = Follower::resuming(path.clone());
        let mut sink = Vec::new();
        assert_eq!(follower.poll(&mut sink).unwrap(), 0);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"fresh").unwrap();
        follower.poll(&mut sink).unwrap();
        assert_eq!(sink, b"fresh");
    }

    /// The rotation handoff loses nothing: a follower behind by several bytes when the writer
    /// rotates drains the renamed generation and the new one in a single poll, byte for byte.
    #[test]
    fn rotation_hands_off_byte_for_byte_while_the_reader_is_behind() {
        let scratch = ScratchDirectory::new().unwrap();
        let path = scratch.path().join("console.log");
        let mut follower = Follower::resuming(path.clone());
        let mut sink = Vec::new();

        fs::write(&path, b"first-").unwrap();
        follower.poll(&mut sink).unwrap();

        // The writer runs ahead: appends, rotates, and starts the next generation before the
        // follower polls again — the sink-side rotation order `ConsoleSink` performs.
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"tail-").unwrap();
        drop(file);
        fs::rename(&path, scratch.path().join("console.log.1")).unwrap();
        fs::write(&path, b"second").unwrap();

        follower.poll(&mut sink).unwrap();
        assert_eq!(sink, b"first-tail-second");
    }

    /// Teardown is quiet: a capture the sweep removed reads as an ended stream, not an error,
    /// and the held descriptor still drains what the sweep unlinked.
    #[test]
    fn teardown_drains_the_unlinked_generation_and_goes_quiet() {
        let scratch = ScratchDirectory::new().unwrap();
        let path = scratch.path().join("console.log");
        let mut follower = Follower::resuming(path.clone());
        let mut sink = Vec::new();

        fs::write(&path, b"last words").unwrap();
        follower.poll(&mut sink).unwrap();

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b", kept").unwrap();
        drop(file);
        fs::remove_file(&path).unwrap();

        follower.poll(&mut sink).unwrap();
        assert_eq!(sink, b"last words, kept");
        assert_eq!(follower.poll(&mut sink).unwrap(), 0);
    }

    /// `SIGINT` is the detach gesture and maps to success; every other terminating signal
    /// keeps its `128+S` meaning — on the stream's select and the already-running path alike.
    #[test]
    fn signal_exit_maps_sigint_to_success_and_the_rest_to_their_status() {
        assert!(matches!(signal_exit(128 + 2), ExitKind::Success));
        assert!(matches!(signal_exit(128 + 1), ExitKind::GuestStatus(129)));
        assert!(matches!(signal_exit(128 + 3), ExitKind::GuestStatus(131)));
        assert!(matches!(signal_exit(128 + 15), ExitKind::GuestStatus(143)));
    }

    /// A signal that landed while nothing polled the watchers is answered by a later
    /// zero-timeout poll rather than waited for — the pending-event contract the
    /// already-running path depends on. Safe to raise for real: the installed watchers
    /// replace the default disposition, so the `SIGHUP` is consumed, not fatal.
    #[tokio::test]
    async fn a_pending_signal_is_answered_without_waiting() {
        let mut signals = crate::cli::session::TerminatingSignals::install();
        assert_eq!(pending_signal(&mut signals).await, None);

        rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::HUP)
            .unwrap();
        // Delivery runs through the runtime's signal driver, so allow it a bounded moment.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(signal) = pending_signal(&mut signals).await {
                assert_eq!(signal, 128 + 1);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the raised SIGHUP never surfaced"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// A sink that refuses the write surfaces the error rather than dropping bytes silently.
    #[test]
    fn a_failing_sink_propagates_its_error() {
        struct RefusingSink;
        impl std::io::Write for RefusingSink {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let scratch = ScratchDirectory::new().unwrap();
        let path = scratch.path().join("console.log");
        fs::write(&path, b"bytes").unwrap();
        let mut follower = Follower { path, open: None };
        let error = follower.poll(&mut RefusingSink).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
}
