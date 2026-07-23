//! TEMPORARY LIBRARY + DOCTEST STUB — REMOVE AFTER THE FIRST REAL DOCTEST LANDS.
//!
//! vivarium is currently a binary-only crate, but the `cargo test --doc`
//! pre-push hook hard-errors with "no library targets found in package
//! `vivarium`" when no library target exists — doctests can only live on a
//! `lib` target. This minimal library target exists solely to give that hook
//! a real library to run against, and the stub item below carries one passing
//! doctest so the hook stays green.
//!
//! Delete this entire file — and drop `src/lib.rs` from the crate — the moment
//! genuine library code with real doctests lands (or the doctest hook is
//! otherwise reconsidered). Nothing in `src/main.rs` depends on it.

/// Placeholder that keeps the `cargo test --doc` hook satisfied.
///
/// ```
/// assert_eq!(vivarium::stub_until_first_real_doctest(), ());
/// ```
pub const fn stub_until_first_real_doctest() {}
