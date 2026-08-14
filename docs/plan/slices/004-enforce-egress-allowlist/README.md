# 004 — Enforce the egress allowlist

## Goal

Enforce the specified name-level egress allowlist before a guest can use an answer while preserving open egress by default.

## Appetite

4 implementation sessions.

## Core

Each VM has isolated networking and a gating resolver that installs address policy before releasing permitted DNS answers; denials reject quickly.

## In scope

- Resolve Q-005 and select the networking crate seams.
- Implement network namespace, tap, nftables, and resolver libraries.
- Implement address expiry and update behavior from the specification.
- Run Q-006's direct vhost-user experiment and use the tap-plus-uplink fallback if it fails.
- Add implementation-level ordering and denial tests.
- Land `workflow_05_restrict_egress_allowlist` unskipped on a capable host and delete the last pre-push subtraction, returning `profile.pre-push` to `kind(test)`. This trial is the final one the acceptance harness holds against an unimplemented verb, so this slice is where the stage becomes a real gate again; [`../../sequencing.md`](../../sequencing.md) owns the order and the two slices that narrowed the clause before it.

## Out of scope

- Changing allowlist syntax or its default.
- Wildcard support.
- A system DNS proxy.
- Inbound networking.

## Governed by

- [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) — defines the enforcement contract.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — defines N8.
- [`../../../explanation/networking-and-egress.md`](../../../explanation/networking-and-egress.md) — owns the subsystem topology.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns backend network modes.
- [`../../../decisions/ADR-0007-default-open-egress.md`](../../../decisions/ADR-0007-default-open-egress.md) — fixes the default.
- [`../../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md`](../../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md) — fixes host enforcement and denial behavior.
- [`../../../decisions/ADR-0064-egress-allowlist-enforcement-model.md`](../../../decisions/ADR-0064-egress-allowlist-enforcement-model.md) — fixes the gating-resolver model.

## Acceptance

While egress is open, the sandbox SHALL reach destinations without allowlist enforcement.

While allowlist mode is active, when a permitted name resolves, the gating resolver SHALL install its addresses before releasing the answer.

If a denied name or address is used, then the host filter SHALL reject the connection inside the specified budget.

When the uplink topology experiment fails, the tap-plus-uplink fallback SHALL preserve the same contract.

## Rabbit holes

- The direct vhost-user combination is undocumented — escape: run one bounded experiment, then use the fallback.
- Resolving before filtering — escape: make answer release depend on successful rule installation.
- Crate search sprawl — escape: evaluate Q-005 against the four explicit seams only.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-005 and Q-006 exit through their recorded slice revision and experiment, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Q-005 exited on 2026-08-14 with the four seams selected and exercised on the capable host, against the tooling this repository's own [`nix/flake.lock`](../../../../nix/flake.lock) pins (nixpkgs `f13ff45a`, reached as store paths through `nix flake archive`: util-linux 2.42.2, iproute2 7.1.0, nftables 1.1.6, passt 2026_07_16). The namespace seam is the pinned util-linux pair rather than a crate: the workspace forbids `unsafe` and the supervisor's runtime is multi-threaded, so an in-process `unshare(CLONE_NEWUSER)` is unavailable twice over, and the shape is `unshare --user --net --map-root-user` holding the pair while `nsenter --preserve-credentials --user --net` joins it by pid — measured three times, each showing uid 0 and the full `CapEff` mask inside an otherwise `lo`-only namespace for an unprivileged caller, and three times a host-side `AF_UNIX` listener answered a client run inside the pair, which is what keeps the VMM's api and console sockets reachable across the boundary. The tap seam is the pinned iproute2 `ip`, a launch-time one-shot a netlink crate would overserve: `ip tuntap add mode tap`, address, up, and default route all took inside the pair twice, with `ip -j` output parsed as the assertion; the interface reports `NO-CARRIER` until a VMM opens it, which is expected and recorded so it is not read as failure. The nftables seam is the `nftables` crate's typed model of the libnftables JSON schema applied through the pinned `nft` binary — `rustables` is GPL-licensed and refused by `deny.toml`'s allow list, and `cargo deny check licenses` passes with the chosen three — and the spike applied the default-deny reject-not-drop ruleset with timeout-flagged sets via `nft -j -f`, watched a five-second element expire on time, and measured one element add through a fresh `nft` process at 2.956 and 2.838 ms median, 3.958 and 3.967 ms max, over two cycles of one hundred adds, so the per-answer subprocess sits well under any upstream lookup it accompanies. The resolver seam is `hickory-proto` for wire parsing with vivarium's own serve loop, because the contract is an ordering and a server framework owns answer release exactly where the gate must interpose, while `hickory-resolver`'s cache would release answers no rule was programmed for and the `domain` crate's license is outside the allow list; the gating core landed as [`src/net/resolver.rs`](../../../../src/net/resolver.rs) with unit tests holding a denied name to `REFUSED` with zero upstream packets, a permitted answer released byte-verbatim only after the filter hook accepted its addresses, a refused installation downgrading the answer to `SERVFAIL`, and `NXDOMAIN` passing through untouched, over an in-process forwarder and a real loopback socket both. The four skeletons live under [`src/net/`](../../../../src/net/) with the allowlist grammar of spec/03 fully matched and refused-not-narrowed; the spike transcript is in the untracked `.draft/q005/` workspace and this paragraph carries the numbers that outlive it.

Q-006 exited on 2026-08-14 through its bounded experiment, and the answer is yes with a finding that decides against using it. The pinned passt (2026_07_16, `--vhost-user`) accepted a connection from the pinned Cloud Hypervisor v53.0 (`--net vhost_user=true,socket=...` against a `--memory shared=on` guest), the device negotiated, and the guest kernel booted past device setup with no VMM error — upstream documents qemu alone, and the combination works. It is not the shipped topology, for a reason the experiment made concrete rather than arguable: a vhost-user uplink carries guest traffic from shared memory into its own process and out through host sockets, so no packet ever crosses a netfilter hook, and the allowlist's default-deny forward chain — the enforcement point ADR-0064 fixes inside the VM's own namespace — would have nothing to attach to. Splitting topologies by mode would make `open` and `allowlist` differ in shape rather than in policy, so both modes ship tap-plus-uplink: the pair held by pinned `unshare`, the tap the VMM opens by name, kernel forwarding raised by the supervisor's own in-namespace entrypoint, and one unprivileged `pasta` per VM joining the pair's namespaces while keeping its sockets on the host side. Measured live on the capable host the same day: a booted guest held `ens3` with the launcher's MAC, a default route via the tap gateway, working name resolution through `pasta`'s DNS forward, and an HTTPS fetch of `cache.nixos.org/nix-cache-info` returning 200 — the first packet a vivarium guest has ever gotten out.

Q-022 exited on 2026-08-14 through the first of its two doors: a two-legged trial whose allowed leg is a reachable destination and whose denied leg is `REFUSED` distinguished from `NXDOMAIN`. The fixture lives in [`tests/support/egress.rs`](../../../../tests/support/egress.rs) and is spec/05's proving shape made executable, entirely inside the VM's own namespace so the trial needs no external network: the harness re-executes the acceptance binary through the same `nsenter` join the supervisor uses, reached by the VMM's recorded pid. A stub upstream binds the uplink's DNS-forward address, added to the pair's loopback so the gating resolver's upstream exchanges land on it; the two `.test` endpoints live in a nested network namespace behind a veth — nested rather than on the pair's own loopback, because a destination local to the pair would be reached over the input path and the allowed leg would never traverse the forward chain the allowlist filters. Two names, two different addresses, only the first allowed; the allowed leg reads the endpoint's bytes back through the filter, the denied name is `REFUSED` at the resolver where an allowlisted name whose upstream says `NXDOMAIN` passes that through verbatim, and a connect to the second address by literal is reset inside the budget — which is the leg that fails if the filter admits everything, closing the vacuous pass Q-022 named from the other side. One fixture defect was found and fixed in the same session: the endpoint crosses three namespaces (host, pair, its own), so waiting for its namespace to differ from the pair alone is satisfied at spawn, and the veth peer landed wherever the process happened to be — the wait now requires distinctness from both. `workflow_05_restrict_egress_allowlist` passed twice on the capable host, `profile.pre-push` returned to plain `kind(test)`, and the restored gate ran 31 of 31 twice at 399s and 364s.
