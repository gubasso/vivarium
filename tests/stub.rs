//! TEMPORARY INTEGRATION-TEST STUB — REMOVE AFTER THE FIRST REAL TEST LANDS.
//!
//! The `cargo nextest` pre-push integration hook filters on `kind(test)` —
//! separate test binaries compiled from `tests/*.rs`. With none present,
//! nextest exits with "no tests to run" (exit code 4) and fails the hook.
//! This placeholder keeps the pre-push gate green until the first genuine
//! integration test exists. Delete this file the moment a real one lands.

#[test]
fn stub_until_first_real_integration_test() {}
