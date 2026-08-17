//! Running a child with its stderr watched, in place of a buffered `.output()`.
//!
//! The four long children — evaluation, build, the runner render, the launch handoff — used to
//! run fully buffered, showing nothing for minutes and then everything at once. This helper keeps
//! the capture (a diagnostic's `why` slot is published text, and it must stay byte-identical to
//! what `.output()` produced) while also forwarding each stderr line to the live step as it
//! arrives.
//!
//! Both pipes are drained concurrently or Nix deadlocks: `nix eval --json` fills stdout while
//! writing stderr, `nix build` does the reverse, and a reader that finished one pipe before
//! touching the other would park the child on a full buffer. stdout takes a thread; stderr is
//! read here, line by line, because the lines are what the step forwards.

use std::io::{self, BufRead as _, BufReader, Read as _};
use std::process::{Command, ExitStatus, Stdio};

use super::Step;

/// What `.output()` used to answer, with stderr already decoded.
///
/// `stderr` is complete and uncapped, always — the `why` slot it feeds is published, so a ring
/// buffer here would be a silent change to a diagnostic. Decoded lossily per line, which equals
/// the whole-buffer decode `.output()` callers performed: a UTF-8 sequence never contains the
/// newline byte, so line boundaries never split one.
pub struct Captured {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Runs `command` to completion, forwarding each stderr line to `step` as it arrives.
///
/// Stream dispositions match `.output()`: stdin closed, both outputs captured.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] when the child cannot be spawned, a pipe cannot be read,
/// or the child cannot be awaited — the same faults `.output()` reported.
pub fn output(command: &mut Command, step: &Step<'_>) -> io::Result<Captured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        if let Some(mut stdout) = stdout {
            stdout.read_to_end(&mut buffer)?;
        }
        Ok(buffer)
    });

    let mut stderr = String::new();
    if let Some(pipe) = child.stderr.take() {
        let mut lines = BufReader::new(pipe);
        let mut raw = Vec::new();
        loop {
            raw.clear();
            if lines.read_until(b'\n', &mut raw)? == 0 {
                break;
            }
            let line = String::from_utf8_lossy(&raw);
            step.line(line.trim_end_matches(['\n', '\r']));
            stderr.push_str(&line);
        }
    }

    let status = child.wait()?;
    let stdout = reader
        .join()
        .map_err(|_| io::Error::other("the stdout reader thread panicked"))??;
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ui::Ui;

    /// The capture is `.output()`'s, byte for byte — the `why` slots downstream depend on it.
    #[test]
    fn the_capture_matches_a_buffered_run() {
        let ui = Ui::silent();
        let step = ui.step("watching");
        let script = "printf 'out'; printf 'first\\nsecond' >&2; exit 3";
        let captured =
            output(Command::new("sh").args(["-c", script]), &step).expect("the child runs");
        assert_eq!(captured.stdout, b"out");
        // Including the unterminated last line, and the real exit status.
        assert_eq!(captured.stderr, "first\nsecond");
        assert_eq!(captured.status.code(), Some(3));
    }

    /// Both pipes fill beyond one buffer without deadlock — the fault this module exists to avoid.
    #[test]
    fn a_child_filling_both_pipes_completes() {
        let ui = Ui::silent();
        let step = ui.step("watching");
        // 1 MiB down each pipe, interleaved by the shell, far past a 64 KiB pipe buffer.
        let script = "for i in $(seq 1 4096); do printf '%256s' x; printf '%256s' y >&2; done";
        let captured =
            output(Command::new("sh").args(["-c", script]), &step).expect("the child runs");
        assert_eq!(captured.stdout.len(), 1_048_576);
        assert_eq!(captured.stderr.len(), 1_048_576);
        assert!(captured.status.success());
    }
}
