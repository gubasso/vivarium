# 023 — A declared port crosses inward

## Goal

Nothing outside a sandbox reaches a service running inside one, and three ordinary workflows have no answer while that holds: an editor attaching over SSH, a guest development server opened in the host's browser, and a notebook served from inside. After this slice a manifest declares which ports cross inward, `viv start` makes them reachable from the host, everything undeclared stays unreachable, and `viv status` names every port that is open.

## Appetite

3 sessions.

## Core

A port named in the manifest is reachable from the host on a running sandbox; a port not named is not; and a user can see, without reading the manifest, which ports a running sandbox has open. The one outcome the core forbids is a port that crosses without appearing in both the declaration and the status output.

## In scope

Ordered, because the mechanism cannot be enacted before it is priced and decided.

1. Price the two mechanisms by measurement against this host, not by reading. The relay: a third vsock port on the pattern [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) already proved, host end an ordinary loopback listener, guest end the agent dialing the service — measure whether the opaque byte relay carries a long-lived listening service as well as it carries the agent channel, and what a second concurrent connection costs. The flag: `pasta` takes forwarding arguments and the uplink in [`../../../../src/launch/policy.rs`](../../../../src/launch/policy.rs) already runs one per VM — measure what the forwarded path exposes in the guest's namespace and on the host's address space. Price both against the same question: what does an open port cost the boundary that `N8` draws for the other direction.
2. Record the decision as an ADR: the chosen mechanism, and the rejected one with the measured reason that rejected it. The declaration is not in question; the ADR decides only how a declared port is carried.
3. Land the declaration. The option beside `sandbox.egress` in [`../../../../nix/vivarium-options.nix`](../../../../nix/vivarium-options.nix), its carriage through [`../../../../nix/launch-arguments.nix`](../../../../nix/launch-arguments.nix) and [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs), and the refusal a malformed or conflicting declaration meets before boot under a code [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) owns. Two sandboxes claiming the same host port is a refusal, not a race.
4. Enact the carriage the ADR chose, in the launch path the mechanism belongs to.
5. Make an open port visible. Every declared port appears in both the human and JSON renderings in [`../../../../src/cli/render.rs`](../../../../src/cli/render.rs), under the status contract [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) owns. This item does not survive a cut: a boundary hole nobody can see is worse than no hole.
6. Extend [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) to state both directions rather than egress alone, including what a declared port costs the boundary, and close `Q-033`.

## Out of scope

- Reaching a sandbox from another machine. The host is the only outside this slice admits; a port binds where the ADR says and never on a routable address.
- An on-demand command that opens a port without a manifest edit. The declaration is the surface; a `viv forward` verb is a separate shaping question if the manifest turns out to be too coarse in use.
- Changing what `exec` and `shell` ride on. They keep the control plane whatever mechanism wins, and the agent channel's own port is untouched.
- The egress allowlist. Which destinations a guest may reach stays `sandbox.egress`; this slice adds a direction beside it and re-litigates nothing.
- Which host interface the guest's egress leaves by — the neighbouring question, logged separately as `Q-032`.
- Ordered remainder, cut first when the appetite binds: a `viv doctor` check for a declared port that nothing is listening on, per-port access control beyond declared or not, and UDP.

## Governed by

- [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) — describes egress only, and is the page this slice makes cover both directions.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — owns `N8`, the egress rule the new direction is stated beside rather than folded into.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — fixes the control transport and the second-port precedent the relay candidate extends.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns the status output an open port has to appear in.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns the code a refused declaration answers with.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes the relay pattern one candidate reuses, and the reason the agent channel is a separate port.
- [`../../open-questions.md`](../../open-questions.md) — carries `Q-033`, which this slice consumes.

## Acceptance

When a manifest declares an inbound port, a trial SHALL reach a service listening on it inside a booted guest, from the host, at the address the ADR fixes. In the same trial a second service listening inside the guest on a port the manifest does not declare SHALL be unreachable from the host, and the negative SHALL be demonstrated rather than assumed.

`viv status` SHALL name every declared inbound port of a running sandbox in both the human and JSON renderings, and a declaration that is malformed, or that claims a host port another running sandbox holds, SHALL fail before boot under the code [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) assigns.

[`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) SHALL state the inbound direction beside the egress posture, including what an open port costs the boundary, and the decision SHALL be recorded as an ADR whose rejected mechanism carries the measured reason that rejected it.

## Rabbit holes

- Building a general port-forwarding manager because one port needed to cross — escape: the declaration is a list of ports; nothing schedules, pools, or load-balances them.
- Making the relay a multiplexer — escape: `ADR-0071` chose a separate port over sharing the control stream for exactly this reason, and that choice holds here.
- Reaching a sandbox from off the machine — escape: out of scope above; admitting it later is a different slice against a different threat.
- Re-opening the egress posture while the page is being edited — escape: `N8` is settled; this slice adds a direction and states it beside the existing one.
- Waiting on the comparison to publish — escape: nothing here depends on it. The comparison found the gap; the gap stands on its own.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, the decision ADR is `Implemented`, `Q-033` is closed and removed, `spec/05` states both directions, and the `milestones.md` row flips to `done`.

## Revisions

Shaped 2026-08-19, before any work started, from a comparison of vivarium's network surface against three other sandboxes. The row that raised it reads no for vivarium, no for `glaipnir`, partial for `flake-pilot`, and yes for `podman` at its microVM runtime.

Reshaped the same day, before landing, after the first shaping put the decision itself in the slice: it offered to enact either the capability or a recorded non-goal, whichever an ADR chose. That hedge was wrong once the owner had said the capability is wanted, so the refusing branch is gone and the slice builds. What remains undecided is only how a declared port is carried, which item 1 prices and item 2 records. The declaration surface was fixed at the same time — a manifest key rather than an on-demand verb, so an open port is project state a reader can review and not invocation state that vanishes with the shell.

`Q-033` still states a two-sided exit and keeps it until this slice lands, since a question is answered by the work rather than by the plan to do it. Landing this slice narrows that exit to the side taken here.
