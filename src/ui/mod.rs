//! The presentation layer: how vivarium looks, held apart from what it says.
//!
//! A leaf module beside `diagnostic` and `exit`, usable by every layer — `config` needs the
//! evaluation face and `diagnostic` needs the palette, and neither may reach into `cli` for them.
//! The one upward import is the `Environment` seam, so resolving the face stays a function of
//! injected inputs like everything else (ADR-0103).
//!
//! Two rules run through all of it. The library split follows the stream split spec/01 fixes:
//! stderr is the human face and stdout is the result, so nothing here ever writes the result.
//! And suppression is resolved once, in [`Ui::resolve`], from the stream facts, the environment,
//! the verbosity, and the output mode — no command ever asks whether it is quiet; it speaks, and
//! a face that has nothing to show drops the words.

pub mod chain;
pub mod style;
pub mod table;
pub mod theme;
pub mod watch;

use crate::config::Environment;
use style::Palette;

/// How much the stderr face says. `-q` and `-v` are global and positionless (ADR-0026); the
/// grammar lifts them out before verb dispatch, and this module is the only reader.
///
/// Ordered so a face can ask `>=` — spec/16 fixes what each level shows, not each caller.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Verbosity {
    /// `-q`: errors only.
    Quiet,
    /// The default: warnings, errors, and normal progress.
    Normal,
    /// `-v`: per-step detail, and child stderr streams through.
    Verbose,
    /// `-vv`.
    Debug,
    /// `-vvv`.
    Trace,
}

/// The face one invocation speaks through, resolved once at the process boundary.
///
/// A value rather than a trait: silence is data inside it, so the no-op face needs no second
/// implementation and every test that wants one writes [`Ui::silent`]. `Sync` is load-bearing —
/// `&Context` crosses an `.await` in ensure-running — which is why nothing here holds interior
/// mutability.
pub struct Ui {
    /// Whether steps animate: stderr is a TTY, the level admits progress, and the caller is
    /// human. One conjunction, resolved once — the whole suppression table lives in `resolve`.
    animate: bool,
    verbosity: Verbosity,
    /// The stderr palette, worn by progress, warnings, and the failure skeleton.
    palette: Palette,
    /// The stdout palette, worn by the result — resolved per stream, because `viv status | less`
    /// keeps its stderr color while its piped result goes plain.
    palette_out: Palette,
    /// Whether an [`Ui::intro`] gutter is open and unclosed. Atomic rather than a `Cell` because
    /// the face must stay `Sync`; it is written only on the animate face.
    gutter: std::sync::atomic::AtomicBool,
}

impl Ui {
    /// Resolves the face from the same injected inputs everything else reads.
    ///
    /// The suppression table, in one place (spec/01, spec/16):
    /// animation needs a stderr TTY, `Normal` or above, and a human caller — `--json` suppresses
    /// progress regardless of TTY, because a caller that selected machine mode is a machine.
    /// Child stderr passthrough needs only `-v`: it is detail, not animation, so it survives a
    /// pipe. Warnings and notes need `Normal`. The failure rendering is never this module's; it
    /// always prints.
    pub fn resolve<E: Environment>(
        stdout_is_tty: bool,
        stderr_is_tty: bool,
        environment: &E,
        verbosity: Verbosity,
        machine_output: bool,
    ) -> Self {
        Self {
            animate: stderr_is_tty && verbosity >= Verbosity::Normal && !machine_output,
            verbosity,
            palette: Palette::resolve(chain::colors_enabled(stderr_is_tty, environment)),
            palette_out: Palette::resolve(chain::colors_enabled(stdout_is_tty, environment)),
            gutter: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The face that says nothing it is not handed: no animation, plain palette, `Normal` level.
    ///
    /// What a test or a headless caller gets; `note` and `warn` still write, byte-identical to
    /// the bare `eprint!` they replaced.
    #[must_use]
    pub const fn silent() -> Self {
        Self {
            animate: false,
            verbosity: Verbosity::Normal,
            palette: Palette::plain(),
            palette_out: Palette::plain(),
            gutter: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The level the grammar resolved; `-v` gates child-stderr streaming through [`Step::line`].
    #[must_use]
    pub const fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    /// The stderr palette, for the one renderer that writes there: the failure skeleton.
    #[must_use]
    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The stdout palette, for the result renderers.
    #[must_use]
    pub const fn palette_out(&self) -> &Palette {
        &self.palette_out
    }

    /// A mid-command announcement that must reach the user whatever happens after it.
    ///
    /// The formalization of the one bare `eprintln!` the surface used to carry: something that is
    /// not the result, not a failure, and not deferrable to `note` — the shed lock node was the
    /// precedent. Suppressed only by `-q`.
    pub fn warn(&self, message: &str) {
        if self.verbosity > Verbosity::Quiet {
            eprintln!("{}", self.palette.warn.apply_to(message));
        }
    }

    /// The deferred stderr channel: `Success::notes`, written after the result.
    ///
    /// Verbatim off a TTY on purpose — the notes' wording is pinned by tests and this face adds
    /// nothing to it there. On the animate face the same words close an open gutter, so a command
    /// that ended early ends visually rather than leaving its frame hanging. Suppressed only by
    /// `-q` (spec/16; the created-pin gap is `Q-028`).
    pub fn note(&self, notes: &str) {
        if notes.is_empty() || self.verbosity == Verbosity::Quiet {
            return;
        }
        if self.animate
            && self
                .gutter
                .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            let _ = cliclack::outro(notes.trim_end());
        } else {
            eprint!("{notes}");
        }
    }

    /// Opens the gutter for a long command: the `┌ title` head of the clack frame.
    ///
    /// Only on the terminal face — the gutter is progress, and spec/01 keeps progress off pipes,
    /// off `-q`, and off `--json`.
    pub fn intro(&self, title: &str) {
        if self.animate {
            let _ = cliclack::intro(title);
            self.gutter
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Closes the gutter with the command's closing word: `└ message`.
    pub fn outro(&self, message: &str) {
        if self.animate {
            let _ = cliclack::outro(message);
            self.gutter
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Closes an open gutter on the failure path, ahead of the skeleton.
    ///
    /// Without this, a command that failed mid-frame would print its diagnostic below a hanging
    /// `│` — the frame must end before the failure speaks.
    pub fn cancel(&self) {
        if self.animate
            && self
                .gutter
                .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            let _ = cliclack::outro_cancel("failed");
        }
    }

    /// Opens one step of a long operation: a spinner on the terminal face, nothing elsewhere.
    ///
    /// The step erases itself when dropped, so every `?` on the way out leaves the terminal clean
    /// before the result prints.
    #[must_use]
    pub fn step(&self, title: &str) -> Step<'_> {
        let bar = self.animate.then(|| {
            let bar = indicatif::ProgressBar::new_spinner()
                .with_style(spinner_style())
                .with_message(title.to_owned());
            bar.enable_steady_tick(std::time::Duration::from_millis(80));
            bar
        });
        Step { ui: self, bar }
    }
}

/// The spinner's shape. The `.cyan` tint is applied through console's stderr color global, which
/// [`theme::install`] pins to the first-party chain's answer at the process boundary; the glyph
/// itself animates regardless, because a spinner exists only where `resolve` already found a
/// terminal.
fn spinner_style() -> indicatif::ProgressStyle {
    indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
        .unwrap_or_else(|_| indicatif::ProgressStyle::default_spinner())
}

/// One live step: what a long operation shows while it runs.
///
/// Dropping it erases the animation; the step never leaves residue on the terminal. The lines it
/// forwards are ephemeral detail, not the result and not a contract — spec/01 keeps progress
/// deliberately loose, which is why nothing here is worth asserting on.
pub struct Step<'a> {
    ui: &'a Ui,
    /// The animation, present only on the terminal face.
    bar: Option<indicatif::ProgressBar>,
}

impl Step<'_> {
    /// One line of a child's stderr, passed through under `-v` (spec/16's per-step detail).
    ///
    /// TTY-independent on purpose: `-v` sets the level and the TTY rule governs animation, so a
    /// piped `-v` run still carries the detail. With an animation live, the line is written above
    /// it — `suspend` clears the spinner, prints, and redraws — so the two never interleave.
    pub fn line(&self, line: &str) {
        if self.ui.verbosity < Verbosity::Verbose {
            return;
        }
        let write = || eprintln!("{}", self.ui.palette.label.apply_to(line));
        self.bar
            .as_ref()
            .map_or_else(write, |bar| bar.suspend(write));
    }

    /// Replaces the step's message: elapsed time on a wait, a phase on a build.
    pub fn update(&self, message: &str) {
        if let Some(bar) = &self.bar {
            bar.set_message(message.to_owned());
        }
    }

    /// Closes the step with its completed line: the spinner erases itself and `◇ message` takes
    /// its place in the gutter.
    ///
    /// Consuming on purpose — a step is either dropped (erased without a trace, the error path)
    /// or done (replaced by its past tense), never both.
    pub fn done(mut self, message: &str) {
        let animated = self.bar.take().is_some_and(|bar| {
            bar.finish_and_clear();
            true
        });
        if animated {
            let _ = cliclack::log::step(message);
        }
    }
}

impl Drop for Step<'_> {
    /// The animation never survives the step: every `?` on the way out erases it before the
    /// result or the failure prints.
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `&Context` crosses an `.await`; a face that stopped being `Sync` would surface there as an
    /// opaque future error far from its cause. This is the near assertion.
    #[test]
    fn the_face_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<Ui>();
    }

    /// The suppression table's rows, exercised through `resolve` — the only place they live.
    #[test]
    fn animation_needs_a_tty_a_level_and_a_human() {
        let none = |_: &'static str| None::<std::ffi::OsString>;
        assert!(Ui::resolve(false, true, &none, Verbosity::Normal, false).animate);
        assert!(!Ui::resolve(false, false, &none, Verbosity::Normal, false).animate);
        assert!(!Ui::resolve(false, true, &none, Verbosity::Quiet, false).animate);
        assert!(!Ui::resolve(false, true, &none, Verbosity::Normal, true).animate);
    }
}
