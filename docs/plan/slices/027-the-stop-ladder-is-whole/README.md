# 027 — The stop ladder is whole

## Goal

Stopping a sandbox is specified as a three-rung ladder and begins at the second rung, because the control protocol carries no shutdown request. The grace the operator asks for bounds the wrong rung, so asking for longer buys a longer wait before the group is killed and the same fixed wait before the guest is destroyed. And there is no way to stop everything at once, which is the shape of an ordinary end of day. After this slice a stop asks the guest first, the operator's grace bounds that ask, and one command stops every running sandbox.

## Appetite

3 implementation sessions.

## Core

`viv stop` asks the in-guest agent for an orderly shutdown before it reaches for the backend's power signal, `--timeout` bounds that ask rather than only the hard rung below it, and `viv stop --all` applies the same ladder to every running sandbox from any directory. The one outcome the core forbids is a sweep that reports success for a sandbox it could not confirm stopped.

## In scope

Ordered, because the rung has to exist before a grace can bound it and a sweep can apply it.

1. Land the first rung. A shutdown request in [`../../../../src/protocol/message.rs`](../../../../src/protocol/message.rs) and its handler in the guest agent, over the control socket the session verbs already use. The backend's power signal stays exactly where [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) puts it, as the fallback for an agent that cannot be reached — which is the case the missing rung costs today.
2. Consume [`Q-018`](../../open-questions.md). The graceful rung belongs to the supervisor, which raises the power signal, waits a compiled-in constant, and destroys the VM, with nothing carrying the requested grace across to it ([`../../../../src/launch/supervisor.rs`](../../../../src/launch/supervisor.rs)). Either a launch-time channel carries the operator's value to the running supervisor before its cancellation path runs, or the graceful rung is bounded by the shipped default and the lifecycle page is amended to say that `--timeout` bounds only the hard one. Whichever holds, the whole ladder still fits inside the ten-second default when no flag is given.
3. Land the sweep. `--all` enumerates through [slice 025](../025-the-fleet-is-visible/README.md)'s reader, applies the same ladder to each running sandbox, and needs no manifest — which makes it the one path that does not refuse a working directory no manifest declares, and the reason that exception is stated rather than discovered. Nothing running stays a no-op that exits `0`, and the first failure is reported rather than swallowed by a sweep that keeps going and returns success.
4. Consume [`Q-021`](../../open-questions.md). Both `stop` and `destroy` are bound in one sentence to emit a record under `--json`, no page fixes that record's shape, and neither emits one, so a caller in machine mode parses zero bytes. The sweep forces the question, because its result is per-sandbox rather than a single state change. Either one entry in [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) fixes both records and the verbs render them, or the record is withdrawn for side-effect verbs and the exit code is the machine-readable result.

## Out of scope

- A cross-project `viv destroy`. Removing data across sandboxes in one command is a different risk from stopping them, and the prompt that guards a single destroy does not generalize by being repeated.
- Sweeping a sandbox no invocation is responsible for. A unit stranded by a killed process is [`Q-024`](../../open-questions.md)'s subject and stays there; this slice stops what is enumerable, and a background reaper is exactly the arbitration the charter refuses.
- Changing what a stop preserves. Volumes, build generations, and the sandbox itself all survive, and the durability the ACPI rung already buys is not renegotiated by adding a rung above it.
- Ordered remainder, cut first when the appetite binds: per-sandbox progress output during a sweep, and item 4's rendering for `destroy` where the record's shape is settled but only `stop` renders it.

## Governed by

- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — fixes the three-rung ladder, the grace, and what a stop preserves.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — fixes the control transport item 1's request rides.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns the `--all` entry and the `--json` records item 4 settles.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — fixes the sweep's reporting of the first failure and the codes a stop answers with.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N18, the preservation the new rung must not weaken.
- [`../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the transport the shutdown request is carried on.
- [`../../../decisions/ADR-0065-control-socket-wire-protocol.md`](../../../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes the wire protocol item 1 extends.
- [`../../../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md`](../../../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) — fixes the unit whose stop the ladder walks and whose kill mode makes the graceful rung reachable at all.
- [`../../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](../../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) — fixes the boundary between stopping and removing, which item 4's shared record must not blur.
- [`../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md`](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) — fixes the refusal item 3's sweep is the stated exception to.
- [`../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) — fixes the key the sweep enumerates and reports failures against.
- [`../../open-questions.md`](../../open-questions.md) — carries `Q-018` and `Q-021`, which items 2 and 4 consume.

## Acceptance

When a running sandbox whose agent is reachable is stopped, the shutdown SHALL be initiated through the agent, and a trial SHALL distinguish that rung from the backend's power signal by evidence rather than by assumption.

When the agent is unreachable and the sandbox is running, the stop SHALL fall through to the backend's power signal and SHALL still complete.

When a guest is deliberately slow to commit and a longer `--timeout` is given, the guest SHALL be allowed that time before the ladder escalates; under no flag, the whole ladder SHALL still complete inside the ten-second default.

When two sandboxes are running, `viv stop --all` invoked from a directory no manifest declares SHALL stop both and SHALL exit `0`, and a second invocation SHALL be a no-op that also exits `0`. If one sandbox cannot be confirmed stopped, then the sweep SHALL report that failure rather than exiting `0`.

An unsynced guest write SHALL survive both the agent rung and the sweep, so N18 holds through the rung this slice adds.

## Rabbit holes

- Making the shutdown request a general remote-command channel — escape: it is one request with one meaning; running commands in the guest is what `exec` is, and it already exists.
- Reaching for a longer compiled-in constant instead of carrying the operator's value — escape: `Q-018` records why that repair is unavailable, since the ladder has to fit inside the default when no flag is given.
- Letting a sweep keep going quietly past a sandbox it could not stop — escape: the exit-code page requires the first failure to be reported, and a sweep that returns `0` over a live VM is worse than one that stops early.
- Growing `--all` into a fleet manager with selection and ordering — escape: it applies one ladder to everything running; choosing which sandbox matters is the user's, which is the same rule admission control follows.
- Settling the `--json` record from inside one verb — escape: `Q-021` names why the two verbs have different things to say, so item 4 fixes the record where the command surface defines records, not where a stop happens to be implemented.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, `Q-018` and `Q-021` are closed and removed with their chosen exits recorded where those exits belong, the records this slice enacts carry it and reach the status that enactment earns, the rows this slice changes are moved in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-20, before any work started, from the gap paragraph in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md). Three of its entries are the same verb: the ladder entered at its second rung, `--all` unimplemented, and — through `Q-018` — a flag that bounds a rung it was not written for. Grouping them is what lets one trial exercise the whole ladder instead of three trials exercising three halves.

The missing rung is the quietest of the gaps because nothing refuses. A stop works, and the guest still runs its own shutdown transaction on the backend's signal, so what the rung costs is the narrow case where the agent is reachable and the backend's socket is not — not durability in general. That narrowness is why this slice sits after the two that close louder gaps, and why it is nonetheless not optional: the specification says three rungs, and a specification that describes a rung nothing walks is the defect this repository's own status page exists to name.

Sequenced behind [slice 025](../025-the-fleet-is-visible/README.md) because item 3 enumerates through its reader, and behind [slice 021](../021-the-manifest-is-the-sandbox/README.md) because a sweep reports per sandbox and the sandbox is what that slice rekeys.
