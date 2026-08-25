# 031 — A declared agent channel reaches the guest

## Goal

The credential relay is specified, decided, built, and unreachable. [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md) fixes a closed two-id channel, [`ADR-0071`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) fixes its transport, the guest agent implements both ends, and the parked pool on the host is proven under more concurrent clients than it has slots. What no code does is hand `viv start` the host socket, so a piece that declares `ssh` does not get a degraded relay — it gets a launch that fails at the runner's usage guard. After this slice a user who declares the channel pushes over SSH and signs a commit from inside a sandbox, with the key material still on the host.

## Appetite

2 implementation sessions.

## Core

A layer declares `vivarium.credentials.agents`, `viv start` resolves the host source that declaration names and carries it to the launch, and a guest process authenticates through the relay without private key material crossing the boundary. A declared channel whose host source is unusable refuses before boot, naming which of the four faults tripped. The one outcome the core forbids is a start that proceeds with a declared channel silently absent, because a relay that is missing rather than refused is discovered as an authentication failure inside the guest.

## In scope

Ordered, because nothing can be resolved before it is visible and nothing can be carried before it is resolved.

1. Publish the declaration. [`../../../../nix/vivarium-report.nix`](../../../../nix/vivarium-report.nix) carries env, mounts, volumes, resources, and egress, and not `vivarium.credentials.agents`, so `viv` cannot see that any layer declared a channel. The value and its provenance join the report on the same terms as every other composed value, which is also what lets `viv config eval` and `viv config sources` show a user which layer opted them in. The declaration surface itself is not in question: spec/07 makes it the typed Nix option precisely because the enum carries no host path, which is what lets a shared piece declare it without violating N11.
2. Resolve the host source, and record the rule for the half that has none. For `ssh` it is `$SSH_AUTH_SOCK`, which spec/07 already names. For `gpg` it is the agent's restricted extra socket and never the ordinary one, and no page says how a host resolves it — item 2 fixes that rule where spec/07 states the pair, because "the restricted extra socket" names a thing and not a way to find it.
3. Carry it. [`../../../../nix/runner.sh`](../../../../nix/runner.sh) already takes `--ssh-agent-socket` and `--gpg-agent-socket`, validates each against the contract's own `credentialIds`, and renders the socket legs the supervisor consumes; [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs) builds the runner's argument group and passes neither. This item is that argument group and nothing else — the launch half below it is built and host-proven.
4. Refuse before boot. The runner's guard is a belt: it exits at a usage line, which is the wrong surface and the wrong code for a user whose agent is not running. The refusal belongs where a mount whose source is unusable already meets one, on the two-tier shape [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) records — decidable from the declaration at evaluation, and only visible after host-side resolution at launch. A host source that is unset, missing, not a socket, or not owned by the user is the launch tier, so `78` unless this item records a reason for another code.
5. Make the probe real. [`../../../../src/doctor/project.rs`](../../../../src/doctor/project.rs) skips `agent-source-usable` as `not-applicable`, and its comment says no channel is declarable — the accurate statement, after item 1, is that none was visible. The probe reports the four faults [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) enumerates, stays soft for the reason that page gives, and never connects.
6. Prove it end to end, as the user. A trial declares the channel in a piece, boots, and performs one operation inside the guest that only a working relay can complete, then asserts that no private key file exists anywhere in the guest. The negative is the half that matters: a trial that only shows an operation succeeding cannot tell a relay from a key that was copied in.
7. Move the rows the slice changes in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and add the row this surface has never had.

## Out of scope

- A TOML key for the channel. spec/07 makes the typed Nix option the declaration surface and gives the reason; a manifest spelling would be a second surface for one decision, and a user who wants it per project writes a one-line piece.
- Widening the closed allowlist. Two ids, and a third is a different question against a different threat.
- Agent-side restrictions inside the guest. spec/07 states the two limits — a compromised guest can use a forwarded key for the session, and a byte relay does not carry the signal an agent uses to recognize a forwarded connection — as limits rather than defects, and this slice does not engineer either away.
- Any part of the pool, the transport, or the guest end. All three are built and host-proven, including the half-close that [`../../../reference/known-issues/resolved/KI-0002.md`](../../../reference/known-issues/resolved/KI-0002.md) records as a backend version floor rather than vivarium's own code.
- Forwarding a host path of any kind. The option's value is an enum member; an arbitrary socket path is what N11 exists to refuse.
- Ordered remainder, cut first when the appetite binds: item 7, then the `gpg` half of items 2 through 6, which leaves `ssh` whole and names `gpg` as owed. The two halves do not share a resolution rule, which is what makes cutting one of them possible without leaving the other half-built.

## Governed by

- [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md) — owns the closed allowlist, the host source of each id, the fixed guest paths, and the half-close requirement; item 2 is the one rule it leaves unstated.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes why forwarding is a relay on its own port and never a mount, which is the reason no `[[mounts]]` row can substitute for this slice.
- [`../../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../../decisions/ADR-0010-secrets-never-in-nix-store.md) — fixes N10, the rule that makes runtime injection the only shape available and the agent channel its primary form.
- [`../../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md) — fixes that a shared piece declares a launch channel by type and the host resolves it at launch, which is the shape items 1 through 3 complete.
- [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) — owns `agent-source-usable`, its four faults, and why it is soft; item 5 implements that entry.
- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — records the two-tier evaluation-then-launch refusal shape item 4 follows rather than invents.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns the code item 4's refusal answers with.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — fixes N17's deny-by-default environment rule and why the guest's `SSH_AUTH_SOCK` is tool-generated rather than forwarded.
- [`../../../guides/keep-secrets-out-of-the-store.md`](../../../guides/keep-secrets-out-of-the-store.md) — its Step 1 already walks a user through the declaration this slice makes true.

## Acceptance

When a piece declares `vivarium.credentials.agents = [ "ssh" ]` and a host agent is running, `viv start` SHALL boot, and a guest process SHALL complete an operation that requires the key. In the same guest, no file holding private key material SHALL exist, and the assertion SHALL be made by inspection rather than by inference from the operation succeeding.

When a channel is declared and its host source is unset, missing, not a socket, or not owned by the user, `viv start` SHALL refuse before it builds or boots, SHALL name which of those four faults tripped, and SHALL exit under the code [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) assigns. No start SHALL proceed with a declared channel absent.

When no layer declares a channel, `viv start` SHALL pass no agent socket and the guest SHALL hold no credential socket, which is the same run every current trial already performs — so the whole existing suite SHALL be unchanged by this slice.

`viv config eval` and `viv config sources` SHALL show the declared channel and the layer that declared it, and `viv doctor` SHALL report `agent-source-usable` as a real finding rather than as `not-applicable`, naming the fault when one is present.

## Rabbit holes

- Reaching for `--env SSH_AUTH_SOCK` because the variable is right there — escape: the guide already says it is not a substitute, and spec/07 explains why a socket's endpoint cannot cross a filesystem share; a variable naming a host path in a guest with its own kernel names nothing.
- Building a general socket-forwarding surface because two ids look like a special case of one feature — escape: the enum is the whole allowlist and its closedness is what makes a shared piece safe to distribute.
- Executing a user-named program to find a socket — escape: spec/07 item 5 forbids a provider hook that puts plaintext in vivarium's address space. Asking the GPG component where its own socket lives returns a path and not a secret, which is a different act, and item 2 records the boundary rather than assuming it.
- Widening the report to expose everything a layer set, because one option needed exposing — escape: item 1 adds one value on the existing terms; the report's shape is a contract and not a debugging surface.
- Repairing the pool, the framing, or the half-close because the relay is finally being used — escape: all three are proven by `tests/guest_agent_host.rs` on a capable host, twice. A fault seen here is a launch-path fault until measurement says otherwise.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial item 6 lands, item 2's resolution rule is written where spec/07 states the pair rather than only in this document, the `agent-source-usable` row and the credential-channel row in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) say what runs, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-25, before any work started, from an owner review of what stands between vivarium and daily use in place of a container-based agent sandbox. Four gaps were priced in that review; this one was ranked first on the ratio of what it unblocks to what it costs, because git over SSH and signed commits are daily operations with no workaround inside a sandbox, and because the expensive halves — the transport, the guest agent, the parked pool, the half-close — were all delivered by slice 003 and proven on a capable host.

The appetite is two sessions rather than more because the shaping found the gap to be three hops on the host side of an otherwise complete subsystem: a value missing from the report, a resolution rule that no page states for the `gpg` half, and an argument group that passes neither socket. What made it look larger from outside is that its symptom is a failed launch rather than a missing feature, so the surface reads as unbuilt when only its last hop is.

No decision record accompanies this slice. Both choices that would need one are already recorded: spec/07 fixes the declaration surface and the closed allowlist, and `ADR-0071` fixes the carriage. What items 2 and 4 settle are a resolution rule and a refusal code, which spec/07 and spec/14 own and which this slice writes into those pages rather than into a new record.

Implemented 2026-08-25, one pass under the appetite, items 1 through 7 in order. Three findings from the pass, none a scope change:

- Item 1 cost one move the shaping did not price: the option was declared in `nix/guest.nix`, and the report's provenance side evaluates each layer alone against `nix/vivarium-options.nix` only, so a layer's declaration was invisible to `viv config sources` no matter what joined the tracked keys. The declaration moved to the option surface, and both product readers (`nix/guest.nix`, `nix/launch-arguments.nix`) now `or`-default it for the shipped-image path, the same pattern `vivarium.volumes` already uses.
- Item 4's refusal runs at two sites sharing one resolver: before the build from the merged analysis — a cold start must not spend minutes building toward a launch whose agent is already known unusable — and before the boot from the built contract, which is the only source of the list under `--no-rebuild`. Exit `78` held; spec/14's `69` legend gained the disambiguating clause naming its "agent" as the guest agent.
- Item 2's rule is `gpgconf --list-dirs agent-extra-socket`, recorded in spec/07 with the boundary argued in shaping: a path query is not the provider hook spec/07 item 5 forbids.

Evidence, on the capable host: `workflow_23_agent_channel_relay` passed twice (47.6s cold, 25.4s warm) — refusal at `78` naming `agent-source-unset` with no build record written, a host agent's key listed from inside the guest, the guest's `SSH_AUTH_SOCK` at the fixed relay path, and a decoy-proven scan finding no private key material; `workflow_23_agent_channel_config_surface` shows the channel and its declaring layer on both config surfaces; the full pre-push suite passed 56 of 56, unchanged, which is the third acceptance clause observed rather than argued.

One cut, per the ordered remainder: the `gpg` half of item 6. The `gpg` resolution rule, launch refusal, and doctor leg all shipped with the `ssh` ones — the resolver's `gpg` arm is one match branch — but no end-to-end trial arranges a `gpg-agent` with an extra socket and signs from inside the guest, and the implementation-status row names that as owed.
