//! The process exit taxonomy, held once.
//!
//! [`ExitKind`] mirrors the legend in `docs/reference/spec/14-exit-codes.md`, which is the single
//! source of truth for what every `viv` command returns. It exists as a type rather than a set of
//! `u8` constants for the reason ADR-0033 gives: the taxonomy is a closed, permanent, append-only
//! API, and a closed set is what an enum is for. Choosing a category becomes one exhaustive match
//! a new failure mode cannot compile past, instead of a byte picked at a call site.
//!
//! The full set is present even though today's code reaches only part of it. These variants are
//! not speculative — each one is a row of a specification table that is already fixed, so the enum
//! is a transcription rather than a guess about future needs.
//!
//! The `sysexits` crate is deliberately not used. It admits only `0` and `64..=78`, so it cannot
//! hold [`ExitKind::DoctorStrict`] or [`ExitKind::GuestStatus`], and adopting it would split
//! ownership of the taxonomy across two sources.

use std::process::ExitCode;

/// One category of process outcome, as fixed by the exit-code specification.
///
/// Variants are named for the BSD sysexits constants they carry, plus the two codes vivarium owns
/// outright: `1` for `viv doctor --strict` and `128+S` for a guest killed by signal `S`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitKind {
    /// Success, including an idempotent no-op.
    Success,
    /// `EX_USAGE` — bad invocation.
    Usage,
    /// `EX_DATAERR` — the merged configuration carries a defect vivarium's own rules reject.
    DataErr,
    /// `EX_UNAVAILABLE` — a required host prerequisite, capacity reserve, VM, agent, or store is
    /// unavailable.
    Unavailable,
    /// `EX_SOFTWARE` — a Nix evaluation or build fault, or an internal error vivarium owns.
    Software,
    /// `EX_IOERR` — I/O failure on a channel vivarium owns, the channel failing rather than its
    /// contents being wrong.
    IoErr,
    /// `EX_TEMPFAIL` — transient: a lock race, or a VM-state precondition clearable in one step.
    TempFail,
    /// `EX_NOPERM` — a host permission failure before the guest process starts.
    NoPerm,
    /// `EX_CONFIG` — no manifest resolves, or one in force makes the command impossible.
    Config,
    /// The only sanctioned bare `1`: `viv doctor --strict` promoting a soft warning to a failure.
    DoctorStrict,
    /// A guest process result passed through verbatim, or `128+S` for a guest killed by signal `S`.
    GuestStatus(u8),
}

impl ExitKind {
    /// The number a caller reads from `$?`.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::DoctorStrict => 1,
            Self::Usage => 64,
            Self::DataErr => 65,
            Self::Unavailable => 69,
            Self::Software => 70,
            Self::IoErr => 74,
            Self::TempFail => 75,
            Self::NoPerm => 77,
            Self::Config => 78,
            Self::GuestStatus(status) => status,
        }
    }
}

impl From<ExitKind> for ExitCode {
    fn from(kind: ExitKind) -> Self {
        Self::from(kind.code())
    }
}

#[cfg(test)]
mod tests {
    use super::ExitKind;

    /// One row per variant, transcribed from the specification legend. The numbers are a permanent
    /// API, so they are asserted here rather than trusted to survive an edit to the match above.
    #[test]
    fn every_category_carries_its_specified_number() {
        let rows = [
            (ExitKind::Success, 0),
            (ExitKind::DoctorStrict, 1),
            (ExitKind::Usage, 64),
            (ExitKind::DataErr, 65),
            (ExitKind::Unavailable, 69),
            (ExitKind::Software, 70),
            (ExitKind::IoErr, 74),
            (ExitKind::TempFail, 75),
            (ExitKind::NoPerm, 77),
            (ExitKind::Config, 78),
        ];
        for (kind, expected) in rows {
            assert_eq!(kind.code(), expected, "wrong code for {kind:?}");
        }
    }

    /// The guest pass-through is the one variant whose number is data rather than a constant: a
    /// guest status is returned verbatim, and a guest killed by signal `S` becomes `128+S`.
    #[test]
    fn a_guest_status_passes_through_unchanged() {
        for status in [0_u8, 1, 69, 127, 137, 255] {
            assert_eq!(ExitKind::GuestStatus(status).code(), status);
        }
    }
}
