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

Implemented 2026-08-25 in one pass, core and both remainder items. The rung rides a new additive frame pair (`0x10` shutdown request, `0x11` acknowledgement) with the schema version unchanged, and the guest side keeps the agent unprivileged: the agent touches a file in its own runtime directory and a root-owned path unit runs `systemctl poweroff`, an option chosen over a capability grant and over polkit after a survey of prior art — the QEMU guest agent runs as root and calls the system's own shutdown, Kata's runtime powers the VM off from outside, and neither shape fits an agent whose bounding set is deliberately empty. The acknowledgement follows the trigger write, so it promises motion rather than completion, which is the QEMU `guest-shutdown` semantic; the host confirms by watching the unit go inactive.

`Q-018` closed on its recorded second exit, sharpened by the rung this slice adds: no launch-time channel exists, the operator's `--timeout` fully bounds the orderly ask — which is the rung the flag was written for — and the supervisor's power window stays compiled, now as the bound of the fallback rather than of the whole stop. One acceptance clause was revised by the owner on 2026-08-25, before the ladder landed: a guest that acknowledges the request and then wedges inside its own shutdown is handed to the power path's compiled ten-second window before the group is killed, because a kill at the deadline would skip the supervisor's sweep and turn a slow-but-orderly guest into an unconfirmed teardown. Under no flag the whole ladder still completes inside the ten-second default for every guest except two named cases, each a bounded overshoot stated in `spec/10`: the acknowledged-then-wedged guest above, and — reshaped by review round 1 below — a silent agent that holds the ask open without answering, whose two-second budget is not deducted from the fallback's grace. Both are Acceptance revisions: where the wall-clock clause collided with the clause that an unreachable agent's stop still completes, completing won, and the cost is bounded rather than open.

`Q-021` closed on its first exit: one `spec/01` section fixes both records — `{manifest, state, rung}` for a stop, with the sweep repeating that row under `projects`, and `{manifest, removed, spared, volumes_kept}` for a destroy — and both verbs render them. A declined destroy prompt emits no record, because a record documents a performed run. `destroy` also gained the first rung for free, since it walks the same ladder.

The sweep's failure policy is spelled out where the acceptance only implied it: every enumerated running sandbox is attempted, each failure is named on stderr as it happens, and the first failure is the exit after the sweep finishes — stopping early would leave the rest running behind a nonzero exit, which is the same defect as returning `0` over a live VM in a quieter coat. One acceptance leg is held by construction rather than by a trial: a genuinely unconfirmable sandbox cannot be soundly fabricated at any gate — fabricated runtime markers without a unit read as already-gone, correctly, and a wedged real unit is exactly what a suite must not create — so the forbidden outcome rests on the sweep running the same teardown confirmation the local trials prove, plus a unit test that a later failure never displaces the first.

Host evidence, 2026-08-25, one pass each then repeated by the full `pre-push` profile: `workflow_27_stop_agent_rung_evidence` 40.5 s (the stop itself under the eight-second floor the power path cannot beat, with the backend's API socket deleted and the record reading `agent`; the unsynced write survived; the control-socket leg fell through to `power-signal`), `workflow_27_stop_all_sweep` 58.4 s (two running sandboxes stopped from an undeclared directory, second sweep an empty no-op, unsynced write survived the sweep), `workflow_27_stop_slow_guest_grace` 70.1 s (a 20-second `ExecStop` guest finished through the agent rung under `--timeout 40` after more time than the default allows; under `--timeout 3` the escalation cut it short through `power-signal`), `workflow_27_stop_destroy_records` at the CLI gate. The guest image change ships `TimeoutStopSec=5` on the agent unit beside the poweroff pair, so one signal-trapping session cannot stall an orderly shutdown behind systemd's 90-second default.

Review round 1 (2026-08-25) surfaced three findings, all fixed before the round-2 re-review. The unreachable-agent fallback had been handed only what remained of the grace after the ask, which a fully-hung two-second ask could squeeze below the supervisor's roughly eight-second floor — the fallback now gets the operator's grace restarted, the bounded ≤2 s stretch stated in `spec/10`, with a silent-agent timeout test beside the client. `destroy` had trusted the ladder's answer where `stop` re-checks it: it now re-reads the runtime records after the ladder and refuses at `69` while they read live, without demanding the swept directory a crashed VM legitimately lacks. And `spec/01`'s first wording of the destroy record promised repeated `--keep-volumes` runs the same `removed` boundary, which the deliberate run-time carve-out expansion cannot give; the record's semantics are now stated as the plan-as-executed, static names retained when absent and the carve-out enumerated live.

Review round 2 held the destroy gate to a sharper standard and it was right to: the state model names a live pid with an unaskable unit `failed` — broken records — and the first gate read `Failed` as down, so a manager outage could still have let a destroy unlink beside a live guest. The gate now uses the three-way presence judgment the session verbs already own: only a positively dead answer — no process behind the records, or an owning unit that disowns them — permits removal, and live and could-not-tell both refuse at `69`. The same round found the Acceptance record contradicting itself about the ten-second default, which the paragraph above now reconciles: two named bounded overshoots, not one.

Review round 3 tightened the same gate once more, and the lesson generalizes: `vm_presence` had read a missing, unreadable, or malformed pid record as `Dead`, which let an absence of evidence authorize a removal. The pid reading is now three-valued — only a pid that parsed and whose process the kernel says is gone is a positive death — and the whole judgment is a pure function (`presence_of`) whose no-evidence rows land on `Indeterminate`, unit-tested row by row. A side effect improves the session verbs too: a running VM whose pid record went missing now reads `Live` from its owning unit rather than being force-stopped as stale.

Review round 4 carried the same principle one syscall deeper: the `kill(pid, 0)` probe had classified every errno except `EPERM` as death, where the kernel gives only `ESRCH` that meaning — a failed probe is now no evidence, alive is `Ok` or `EPERM`, and the pid-record unit test asserts all three readings, a reaped child included.
