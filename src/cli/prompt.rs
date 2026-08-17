//! Asking a yes/no question before doing something irreversible.
//!
//! spec/01 and spec/10 both say `viv destroy` prompts on a TTY, and ADR-0067 gives `viv volume
//! prune` the same rail. This is that prompt and nothing else: no policy about who should be
//! asked, which is the grammar's (a missing `--yes` off a terminal is already a `64`), and no
//! knowledge of what either verb removes, which is the verb's.
//!
//! Pure in its two streams, so every row of the answer table is testable without a terminal — the
//! same seam [`super::grammar::Streams`] gives the grammar and `Environment` gives resolution. The
//! acceptance harness runs `viv` with piped stdin and therefore exercises only the `--yes` path,
//! which would leave this untested if it read the process's own handles directly.

use std::io::{self, BufRead, Write};

use crate::ui::style::Palette;

/// What a destructive verb asks before it acts.
pub struct Question<'a> {
    /// The one-line question, without the `[y/N]` suffix this renders.
    pub headline: &'a str,
    /// What is about to be removed, one line each.
    ///
    /// Never empty by construction of every caller: a verb that cannot enumerate what it is about
    /// to remove has not established that it should be asking rather than refusing.
    pub scope: Vec<String>,
    /// What the verb will not touch, one line each. Empty when there is nothing worth saying.
    ///
    /// A user approving something irreversible should be told the boundary and not only the blast
    /// radius — `destroy` leaving the manifest binding and the workspace alone is the difference
    /// between a rebuild and a loss, and it is not guessable from the scope.
    pub spared: Vec<String>,
}

/// Asks `question` on `output` and reads one line from `input`.
///
/// Writes to stderr in every caller, including under `--json`: spec/01 puts the result on stdout
/// and everything else on stderr, and a prompt is not a result. It cannot go through
/// [`super::Success::notes`] either, which is flushed after the command has already run.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] when the question cannot be written or the answer cannot
/// be read. A prompt that could not be shown is never treated as approval.
pub fn confirm(
    question: &Question<'_>,
    palette: &Palette,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<bool> {
    for line in &question.scope {
        writeln!(output, "  {line}")?;
    }
    for line in &question.spared {
        writeln!(output, "  {} {line}", palette.label.apply_to("kept:"))?;
    }
    // Capitalised `N`, which is the promise the empty answer below keeps.
    write!(
        output,
        "{} {} ",
        palette.what.apply_to(question.headline),
        palette.label.apply_to("[y/N]"),
    )?;
    output.flush()?;

    let mut answer = String::new();
    // A closed stdin is nobody there to say yes, not a channel fault — and the grammar has already
    // made the non-terminal case a `64`, so this is only reachable when a terminal goes away
    // mid-prompt. Read zero bytes, answer no.
    if input.read_line(&mut answer)? == 0 {
        writeln!(output)?;
        return Ok(false);
    }
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ask(answer: &str) -> (bool, String) {
        let question = Question {
            headline: "destroy `demo`?",
            scope: vec!["state: /s/projects/demo".to_owned()],
            spared: vec!["the manifest binding".to_owned()],
        };
        let mut input = answer.as_bytes();
        let mut output: Vec<u8> = Vec::new();
        let confirmed = confirm(&question, &Palette::plain(), &mut input, &mut output).unwrap();
        (confirmed, String::from_utf8(output).unwrap())
    }

    #[test]
    fn only_an_explicit_yes_is_a_yes() {
        for affirmative in ["y\n", "Y\n", "yes\n", "YES\n", "  yes  \n", "Yes"] {
            assert!(ask(affirmative).0, "{affirmative:?}");
        }
        // The empty answer is the one the `[y/N]` capitalisation promises, and `yeah` is the one
        // worth naming: a prefix match would accept it, and this is not a prefix match.
        for negative in ["", "\n", "n\n", "no\n", "yeah\n", "1\n", "true\n", " \n"] {
            assert!(!ask(negative).0, "{negative:?}");
        }
    }

    #[test]
    fn the_question_carries_its_whole_scope_and_what_survives() {
        let (_, rendered) = ask("n\n");
        assert!(rendered.contains("state: /s/projects/demo"), "{rendered}");
        assert!(
            rendered.contains("kept: the manifest binding"),
            "{rendered}"
        );
        assert!(rendered.contains("destroy `demo`? [y/N] "), "{rendered}");
    }

    #[test]
    fn a_question_that_cannot_be_shown_is_never_an_approval() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("no terminal"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let question = Question {
            headline: "destroy?",
            scope: vec!["everything".to_owned()],
            spared: Vec::new(),
        };
        // The error propagates rather than becoming `false`, so the caller reports why it could
        // not ask instead of reporting a refusal the user never gave.
        assert!(
            confirm(
                &question,
                &Palette::plain(),
                &mut b"y\n".as_slice(),
                &mut Broken
            )
            .is_err()
        );
    }
}
