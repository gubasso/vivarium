//! The human error presentation fixed by `docs/reference/spec/14-exit-codes.md`, held once.
//!
//! The skeleton is a compatibility surface rather than a courtesy. Neither file a user writes
//! carries a schema version, so an unknown key is how "this file was written for a newer vivarium"
//! reaches its reader; ADR-0075 lists the two compatibility messages among the three surfaces that
//! are already permanent before 1.0. That is why the slots live here instead of at each call site:
//! a second rendering would be a second promise, and the two would drift.
//!
//! The wording rules are the renderer's job for the same reason — lowercase start, no terminal
//! punctuation, and the fixed slot order hold for every caller without any of them restating them.

use std::fmt;
use std::path::{Path, PathBuf};

/// The namespace half of a diagnostic id.
///
/// Closed on purpose: spec/14 reserves exactly these, and a category that cannot be spelled is one
/// that cannot silently appear.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Namespace {
    /// A defect in a manifest or an `inputs.toml`.
    Manifest,
    /// A defect in something vivarium wrote under the state root.
    State,
    /// A defect that only the merged configuration reveals.
    Merge,
    /// A lockfile or lock-acquisition failure.
    Lock,
    /// A host prerequisite or host-side operation.
    Host,
    /// The virtual machine and its lifecycle.
    Vm,
    /// The guest agent and processes inside the guest.
    Guest,
    /// The Nix store and its contents.
    Store,
    /// A fault in vivarium itself.
    Internal,
}

impl Namespace {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::State => "state",
            Self::Merge => "merge",
            Self::Lock => "lock",
            Self::Host => "host",
            Self::Vm => "vm",
            Self::Guest => "guest",
            Self::Store => "store",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One `<namespace>.<condition>` handle, stable and never reassigned.
///
/// The id is the instance handle the exit code cannot carry. It is deliberately not a branch
/// surface: consumers branch on the code, and this exists to be greppable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticId {
    namespace: Namespace,
    condition: &'static str,
}

impl DiagnosticId {
    /// Names one condition within a namespace.
    #[must_use]
    pub const fn new(namespace: Namespace, condition: &'static str) -> Self {
        Self {
            namespace,
            condition,
        }
    }

    /// Whether the condition half matches spec/14's `[a-z0-9-]+` grammar.
    ///
    /// Checked rather than enforced in the constructor because rejecting a malformed id at
    /// construction would mean panicking in a `const fn`, and an id is minted in source rather than
    /// from input. The test that walks every minted id is what makes this load-bearing.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        !self.condition.is_empty()
            && self
                .condition
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }
}

impl fmt::Display for DiagnosticId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.namespace, self.condition)
    }
}

/// The `where` slot, in the three forms spec/14 admits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Locus {
    /// A parser could supply a position: `path:line:column`.
    Position {
        /// The file the position is inside.
        path: PathBuf,
        /// One-based line.
        line: usize,
        /// One-based column, counted in characters.
        column: usize,
    },
    /// A file is involved but no position within it is known.
    File(PathBuf),
    /// No file at all — `state registry`, `control socket`, a check id.
    Named(&'static str),
}

impl Locus {
    /// Converts a byte offset into the position slot.
    ///
    /// The conversion lives here rather than beside each parser because the offset is only ever
    /// wanted in order to render it, and one implementation cannot disagree with itself about
    /// whether lines and columns are zero- or one-based.
    ///
    /// An offset past the end of `source` yields the position just after the last character, which
    /// is what a truncated document should report rather than a panic.
    #[must_use]
    pub fn in_source(path: impl Into<PathBuf>, source: &str, offset: usize) -> Self {
        let mut line = 1;
        let mut column = 1;
        for character in source[..offset.min(source.len())].chars() {
            if character == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        Self::Position {
            path: path.into(),
            line,
            column,
        }
    }

    /// The file this locus names, when it names one.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Position { path, .. } | Self::File(path) => Some(path),
            Self::Named(_) => None,
        }
    }
}

impl fmt::Display for Locus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position { path, line, column } => {
                write!(formatter, "{}:{line}:{column}", path.display())
            }
            Self::File(path) => write!(formatter, "{}", path.display()),
            Self::Named(name) => formatter.write_str(name),
        }
    }
}

/// One failure, rendered in the fixed slot order.
///
/// Four slots are mandatory and two are conditional. `accepted here:` appears only for an unknown
/// key or an unknown value, and `hint:` is omitted when no honest local repair exists rather than
/// filled with advice that does not act.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    id: DiagnosticId,
    what: String,
    locus: Locus,
    why: String,
    accepted: Vec<String>,
    hint: Option<String>,
}

impl Diagnostic {
    /// Builds a diagnostic carrying the four mandatory slots.
    #[must_use]
    pub fn new(
        id: DiagnosticId,
        what: impl Into<String>,
        locus: Locus,
        why: impl Into<String>,
    ) -> Self {
        Self {
            id,
            what: what.into(),
            locus,
            why: why.into(),
            accepted: Vec::new(),
            hint: None,
        }
    }

    /// Adds the conditional `accepted here:` slot, listing what is accepted at this position.
    #[must_use]
    pub fn with_accepted<I, S>(mut self, accepted: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.accepted = accepted.into_iter().map(Into::into).collect();
        self
    }

    /// Adds the `hint:` slot. An embedded newline continues the hint on the next rendered line,
    /// indented to align under the first, which is the only wrapping the skeleton has.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The id this diagnostic carries.
    #[must_use]
    pub const fn id(&self) -> DiagnosticId {
        self.id
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "error[{}]: {}", self.id, self.what)?;
        writeln!(formatter, "  --> {}", self.locus)?;
        write!(formatter, "  why: {}", self.why)?;
        if !self.accepted.is_empty() {
            write!(formatter, "\n  accepted here: {}", self.accepted.join(", "))?;
        }
        if let Some(hint) = &self.hint {
            // Eight spaces is the width of "  hint: ", so a wrapped hint stays under its own text
            // rather than under the slot label.
            let mut lines = hint.lines();
            if let Some(first) = lines.next() {
                write!(formatter, "\n  hint: {first}")?;
            }
            for line in lines {
                write!(formatter, "\n        {line}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DiagnosticId, Locus, Namespace};

    /// Pins the whole skeleton against the worked example in spec/14, slot for slot.
    #[test]
    fn the_rendered_skeleton_matches_the_specified_shape() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(Namespace::Manifest, "unknown-key"),
            "unknown key `schema_version` in manifest `rust-web`",
            Locus::Position {
                path: "manifests/rust-web.toml".into(),
                line: 7,
                column: 1,
            },
            "not part of the manifest grammar viv 0.4.1 understands",
        )
        .with_accepted(["image", "pieces", "extends", "env"])
        .with_hint(concat!(
            "remove the key, or upgrade vivarium — a manifest written for a newer\n",
            "vivarium reports its new keys exactly this way",
        ));

        assert_eq!(
            diagnostic.to_string(),
            concat!(
                "error[manifest.unknown-key]: unknown key `schema_version` ",
                "in manifest `rust-web`\n",
                "  --> manifests/rust-web.toml:7:1\n",
                "  why: not part of the manifest grammar viv 0.4.1 understands\n",
                "  accepted here: image, pieces, extends, env\n",
                "  hint: remove the key, or upgrade vivarium — a manifest written for a newer\n",
                "        vivarium reports its new keys exactly this way",
            )
        );
    }

    /// Pins both conditional slots as omitted rather than rendered empty.
    #[test]
    fn the_conditional_slots_disappear_when_unused() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(Namespace::State, "unreadable"),
            "cannot read the project registry",
            Locus::Named("state registry"),
            "the file exists but its contents are not TOML",
        );
        assert_eq!(
            diagnostic.to_string(),
            concat!(
                "error[state.unreadable]: cannot read the project registry\n",
                "  --> state registry\n",
                "  why: the file exists but its contents are not TOML",
            )
        );
    }

    /// Pins the three `where` forms spec/14 admits, including the file-without-position degrade.
    #[test]
    fn the_locus_degrades_through_its_three_forms() {
        assert_eq!(
            Locus::Position {
                path: "a/b.toml".into(),
                line: 3,
                column: 12,
            }
            .to_string(),
            "a/b.toml:3:12"
        );
        assert_eq!(Locus::File("a/b.toml".into()).to_string(), "a/b.toml");
        assert_eq!(Locus::Named("control socket").to_string(), "control socket");
    }

    /// Pins the offset conversion as one-based on both axes, and total on a truncated document.
    #[test]
    fn a_byte_offset_becomes_a_one_based_position() {
        let source = "image = \"rust\"\npieces = []\n";
        assert_eq!(
            Locus::in_source("m.toml", source, 0),
            Locus::Position {
                path: "m.toml".into(),
                line: 1,
                column: 1,
            }
        );
        assert_eq!(
            Locus::in_source("m.toml", source, 15),
            Locus::Position {
                path: "m.toml".into(),
                line: 2,
                column: 1,
            }
        );
        assert!(matches!(
            Locus::in_source("m.toml", source, 9_999),
            Locus::Position { line: 3, .. }
        ));
    }

    /// Pins the id grammar. The condition half is minted in source, so this is what keeps a
    /// malformed one from reaching a user through a surface that promises stable ids.
    #[test]
    fn a_diagnostic_id_matches_the_reserved_grammar() {
        for condition in ["unknown-key", "syntax", "wrong-type", "a1"] {
            assert!(DiagnosticId::new(Namespace::Manifest, condition).is_well_formed());
        }
        for condition in ["", "Unknown", "unknown_key", "unknown.key"] {
            assert!(!DiagnosticId::new(Namespace::Manifest, condition).is_well_formed());
        }
    }
}
