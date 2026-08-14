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

## Sessions 2 and 3 — merged execution, 2026-08-14

The operator directed one pass over the remaining work; checkpoints stay per former session as commits. Within former session 3, open-mode connectivity lands before the allowlist wiring, because it is what unblocks milestone 014.

- [x] Netns lifecycle: `holder_program`, `await_pair` readiness watch in `src/net/netns.rs`.
- [x] Tap configuration: `src/net/tap.rs` `ip` sequences and `ip -j` assertion; no-carrier pre-attach recorded as expected state.
- [x] nftables apply: `NftRunner` (direct and `nsenter`-entered), `NftProgrammer`, `timed_elements` batches, literal address/CIDR interval sets `allow4net`/`allow6net`.
- [x] Resolver: UDP serve loop with per-query upstream exchange; IPv6 parity; AAAA withholding behind `ServeConfig`; upstream failure to `SERVFAIL`, never silence.
- [x] Manifest seam closed: `egress.allow` entries validated against the grammar at parse.
- [x] Gated integration lane `tests/net_host.rs`: pair distinct and joinable, tap without carrier, ruleset applied and elements expiring through real `nft`, resolver installing through real `nft` before release. 4 of 4 twice on this host.
- [x] Launch schema 6: egress fields, network fields, new `backend_programs`.
- [x] `nix/launch-arguments.nix`, `nix/runner.sh`, `nix/guest.nix` NIC plumbing.
- [x] Supervisor spawn order: holder, tap, uplink, resolver, VMM-in-namespace.
- [x] Open-mode connectivity measured live: `ens3` with the launcher's MAC, default route via the tap gateway, DNS through `pasta`'s forward, HTTPS 200 from `cache.nixos.org`. `tests/host/exec-and-shell-check` green twice on the new path.
- [x] Q-006 bounded experiment run and recorded in `Revisions`: vhost-user drives the pinned VMM, tap-plus-uplink adopted anyway because enforcement needs the in-namespace hook.
- [x] Allowlist mode verified live: `resolv.conf` holds the gateway resolver alone, the allowed name returned A records (AAAA withheld) and fetched HTTPS 200, the denied name failed in 9 ms, and a denied literal TCP connect was refused in 15 ms.

## Session 4 — acceptance

- [ ] Q-022 two-legged fixture: in-namespace listener, two `.test` names on different addresses, only the first allowed; `REFUSED` distinguished from `NXDOMAIN`.
- [ ] `workflow_05_restrict_egress_allowlist` unskipped and green on this host.
- [ ] `profile.pre-push` returned to plain `kind(test)`; `pre-push` twice.
- [ ] Implementation status rewritten; Q-022 exit recorded; row to `done`; this file deleted.
