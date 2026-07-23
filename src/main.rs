fn main() {
    println!("Hello, world!");
}

// TEMPORARY TEST STUB — REMOVE AFTER THE FIRST REAL TEST LANDS.
//
// This crate has no tests yet, but the `cargo nextest` pre-commit/pre-push
// hooks fail with "no tests to run" (exit code 4) against an empty test set.
// This placeholder unit test (kind(bin), the `pre-commit` profile's filter)
// keeps the hooks green until the first genuine unit test exists. Delete this
// whole module — and this comment — the moment a real test lands.
#[cfg(test)]
mod tests {
    #[test]
    fn stub_until_first_real_test() {}
}
