# Tasks — 004 enforce-egress-allowlist

Working state for the active slice; exists because the work crosses context resets, and is deleted when the [`milestones.md`](../../milestones.md) row flips to `done`. Durable outcomes go to `README.md` `Revisions`, not here.

## Session map

1. Resolve Q-005 with executable evidence: select the namespace, tap, nftables, and resolver seams; exit through a `Revisions` line; commit the four library skeletons under `src/net/` with unit tests.
2. Implement the libraries for real: netns lifecycle, tap configuration, nftables apply plus TTL-driven element expiry and update behavior, full gating resolver (IPv6 parity, AAAA withholding, `NXDOMAIN`/`SERVFAIL` passthrough); gated integration tests; hook the allowlist grammar into manifest validation at the deferred seam in `src/config/manifest.rs`.
3. Wiring: launch schema 6 with egress fields and new `backend_programs` entries; `nix/launch-arguments.nix` and `nix/runner.sh` plumbing; `nix/guest.nix` NIC and resolver-as-only-DNS; supervisor spawn order holder, tap, uplink, resolver, VMM-in-namespace; run Q-006's bounded `passt --vhost-user` experiment with tap-plus-uplink as the pre-authorized fallback.
4. Acceptance: Q-022's two-legged fixture with an in-namespace listener and two `.test` names on different addresses; `workflow_05_restrict_egress_allowlist` unskipped on a capable host; `profile.pre-push` returned to plain `kind(test)`; implementation status rewritten; row to `done`; this file deleted.

## Session 1 checklist

- [x] Milestone row `active`.
- [x] Spike A — user+net namespace pair unprivileged, `CAP_NET_ADMIN` inside, host-side unix socket reachable from inside; three repeats.
- [x] Spike B — tap created and configured inside the pair with pinned `ip`.
- [x] Spike C — default-deny reject-not-drop ruleset with a timeout set via the `nftables` crate and pinned `nft`; element expiry observed; element-add latency measured over 100 adds, two cycles.
- [x] Spike D — `hickory-proto` gating loop on loopback, promoted into `src/net/resolver.rs` unit tests: `REFUSED` without forwarding, release only after the install hook resolves, `NXDOMAIN` passthrough, TTL extracted.
- [x] `cargo deny check licenses` with the new dependencies.
- [x] Skeletons committed: `src/net/allowlist.rs`, `src/net/nft.rs`, `src/net/resolver.rs`, `src/net/netns.rs`.
- [x] Q-005 removed from `open-questions.md`; `Revisions` paragraph with dated evidence; explanation page `Unresolved` updated.
- [x] Verification lanes green; `pre-push` twice (26 of 26 at 411s and 386s on 2026-08-14, after one red to the recorded readiness flake under pre-warm build pressure).
