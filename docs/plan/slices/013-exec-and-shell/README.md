# 013 — Exec and shell

## Goal

Give `viv exec` and `viv shell` everything they need to reach into the sandbox and report what actually happened: the invocation payload spec/12 fixes, a host session client that speaks the wire protocol the guest already answers, and the reuse-or-boot routine that means a user runs a command rather than first arranging for a VM.

## Appetite

3 implementation sessions.

## Core

`viv exec` runs a command in the project's sandbox, booting it if it is not already running, and returns the guest command's own code; `viv shell` opens an interactive PTY session with job control and the right size from its first frame; and neither leaves the host terminal or the runtime directory worse than it found them.

## In scope

Ordered, because each item is the precondition of the next: an invocation that discards its argv has nothing to send, a session client with nothing to connect to cannot be exercised, and the PTY case adds sizing and signal behavior on top of a path already known to work.

1. Carry the invocation payload. [`../../../../src/cli/grammar.rs`](../../../../src/cli/grammar.rs) parses `viv exec` for its fail-closed answers and then discards everything it read: `Invocation::Deferred` holds a verb and an output mode, and no argv, no `-t`/`-T`, and no `--env`. Give both verbs an executed invocation that carries the argv after the first `--` byte for byte, the terminal-allocation choice, and each repeated `--env KEY[=VAL]`.
2. Build the host session client. [`../../../../src/launch/control.rs`](../../../../src/launch/control.rs) holds the current-boot readiness handshake and nothing else: no `Start`, `Stdin`, `StdinEnd`, `Resize`, or `Signal` sender, and no `Stdout`, `Stderr`, `Exit`, or `Error` reader. The frame types and the guest half exist and are host-proved, so this is construction against a settled contract rather than protocol work.
3. Implement ensure-running for both verbs — the per-target `flock`, the cheap `Ping`, the `boot.json` identity match, the step-4 staleness rules, and releasing the lock before the session starts. Share the boot path `viv start` already owns rather than growing a second one.
4. Enforce the guest process environment: deny-by-default passthrough with the fixed allowlist plus what `--env` names, the workspace cwd, and the non-root default user. No host variable crosses unnamed, and the agent channel stays the relay's to set rather than something a forwarded name could stand in for.
5. Map results in both directions. A guest status returns verbatim, a guest killed by signal `S` returns `128+S`, an agent `Error` before spawn takes its own category, and a transport that dies after the guest starts returns `74` with a diagnostic rather than a guessed guest code.
6. Wire `viv shell` onto the PTY session: raw mode on the host terminal restored on every exit path including a signal, the size sent before `Start`, a re-send on every host resize, and the interrupt forwarded as a byte so the guest line discipline raises it against the guest's own foreground group.
7. Write the host runbook that drives the gated half, `tests/host/exec-and-shell-check`, as a sibling of [`../../../../tests/host/guest-agent-check`](../../../../tests/host/guest-agent-check) and on the same two-tier shape: an evaluation tier that runs anywhere Nix is present, and a host tier gated on `/dev/kvm`, a systemd user manager, and `$XDG_RUNTIME_DIR`. It calls [`../../../../tests/host/disk-preflight`](../../../../tests/host/disk-preflight) before it writes anything, because ensure-running builds and boots; it runs the lane twice, since the assertions this slice cares most about are session-lifetime ones that pass once and fail on a Tuesday; and it leaves `VIVARIUM_TEST_REQUIRE=1` to CI, having proved the gate itself before running the trial. Two constraints are paid for and should not be rediscovered: the trial's runtime directory and every socket it holds go under `$XDG_RUNTIME_DIR` rather than under the scratch root the preflight returns, because a drive-backed path overruns the 108-byte socket limit and fails as an unexplained child exit; and the lane pins all three of the generated flake's baseline inputs, the product flake included, or it reports on the GitHub rate limit in a store failure's vocabulary. The register in [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) carries both measurements.
8. Land the `Virtualization` trial this slice's `Acceptance` names, unskipped, through that runbook on a capable host, give `viv shell` the interactive coverage [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) records as absent, record what the lane measured in [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md), and move the rows this slice earns. The same item narrows the pre-push subtraction: `profile.pre-push` currently excludes the acceptance binary because these trials assert verbs this slice implements, so landing them green without narrowing that filter would move a row while leaving the gate off. [`../../sequencing.md`](../../sequencing.md) owns the obligation and the order; slice 014 removes what remains of it.

Ordered remainder, cut first when the appetite binds:

1. The live-session count `viv status` reports, which is a reporting surface over a session model this slice already builds.
2. The host-subdirectory cwd mapping; the fallback is the workspace root, which is the same directory for the ordinary invocation.
3. Explicit `Signal` frames on the non-PTY path beyond the interrupt the core needs.

## Out of scope

- Any change to the guest agent, the wire protocol, or the credential relay. All three are done and proved; a change here means a defect was found, and it is recorded as one.
- Agent forwarding behavior beyond confirming the declared second port still works after this slice's wiring.
- Volume persistence and whether a write crosses the workspace mount, which are [slice 014](../014-workspace-and-persistence/README.md)'s. This slice sends a cwd and assumes nothing about what the mount does with it.
- Egress restriction inside an exec session.
- Admission control before a reuse-or-boot decision, which stays [slice 005](../005-cli-runtime-plumbing/README.md)'s alongside the gap `viv start` already records.
- Refining the before-start not-found and not-executable cases onto `126` and `127`, which spec/12 reserves for later.

## Governed by

- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — defines the grammar, the argv boundary, the environment policy, ensure-running, and the wire protocol.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines how an inner code differs from a vivarium code.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — fixes the two verbs' surface and the session count item 1 of the remainder would report.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — owns the ensure-running routine item 3 reuses.
- [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md) — owns the agent channel item 4 must not let an environment variable substitute for.
- [`../../../explanation/guest-control-and-secrets.md`](../../../explanation/guest-control-and-secrets.md) — owns the control topology this slice connects to.
- [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) — records the interactive-PTY coverage gap item 8 closes.
- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — fixes which lane the runbook's two tiers belong to and what each is required to prove.
- [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) — owns the runbook findings register item 8 writes into.
- [`../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the transport and the exec contract.
- [`../../../decisions/ADR-0065-control-socket-wire-protocol.md`](../../../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes the framing this slice speaks.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes the second port the relay uses.
- [`../../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../../decisions/ADR-0015-cli-output-and-failure-contract.md) — fixes which stream carries guest output and which carries vivarium's own.
- [`../../../decisions/ADR-0033-error-handling-and-exit-codes.md`](../../../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes the closed taxonomy item 5 selects from.
- [`../../../decisions/ADR-0013-vm-lifecycle-and-up.md`](../../../decisions/ADR-0013-vm-lifecycle-and-up.md) — fixes that a command needing a VM brings one up.
- [`../../../decisions/ADR-0030-vm-status-and-state-model.md`](../../../decisions/ADR-0030-vm-status-and-state-model.md) — fixes the states item 3 discriminates between before deciding to reuse or boot.
- [`../../../decisions/ADR-0055-runtime-directory-is-required.md`](../../../decisions/ADR-0055-runtime-directory-is-required.md) — fixes the session-scoped runtime root that makes the unauthenticated control socket safe.
- [`../../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md`](../../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md) — fixes the workspace path item 4 starts a session in.

## Acceptance

When `viv exec` runs a command that fails inside the guest, the host process SHALL return the guest command's code and `workflow_06_exec_exit_code_propagation` SHALL pass unskipped.

Where a failure is vivarium's rather than the guest command's, the code returned SHALL be distinguishable from any code the guest command could have produced, and a transport that dies after the guest process starts SHALL return `74` rather than any guest-shaped code.

When `viv exec` is issued against a project whose VM is not running, the command SHALL bring one up and run, and when it is issued against a live VM of matching identity it SHALL reuse that VM rather than boot a second.

When a host variable is neither on the fixed allowlist nor named by `--env`, the guest process SHALL NOT see it.

When `viv shell` opens a session, the guest SHALL receive the terminal size before the session starts, job control SHALL work, a later host resize SHALL reach the guest, and the host terminal SHALL be restored on every exit path including a signal.

While the control socket is not yet ready, `viv exec` SHALL fail with a stated reason rather than hang.

## Rabbit holes

- Reopening protocol questions the guest agent already answered — escape: the guest half passes twice on a capable host; treat it as fixed and change the host side.
- Reading the settled guest half as meaning the host half is nearly done — escape: the host has a readiness handshake and a verb that discards its own arguments, so items 1 through 3 are construction and are budgeted as such.
- Growing a second boot path because ensure-running is reached from a different verb — escape: item 3 shares what `viv start` owns, and a difference between the two is a defect in one of them.
- Building a general terminal multiplexing layer — escape: one PTY session per invocation is the contract, and the transport does the multiplexing.
- Conflating an inner non-zero exit with a vivarium error — escape: item 5 exists precisely to keep the two apart, and the acceptance assertions test both directions.
- Leaving the host terminal in raw mode when a session ends badly — escape: restoration is an acceptance assertion, not a cleanup detail, because the failure it prevents outlives the process.

## Done when

Every acceptance assertion above holds and is demonstrated on a capable host by the trial it names, driven by `tests/host/exec-and-shell-check` and clean on two runs rather than one, `viv shell` carries interactive coverage rather than usage-only coverage, any cut remainder is named in `Revisions` rather than left implied, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

- Reshaped before start, from 2 sessions to 3. The previous shape read item 1 as connecting to an existing host control client; [`../../../../src/launch/control.rs`](../../../../src/launch/control.rs) holds only the readiness handshake, [`../../../../src/cli/grammar.rs`](../../../../src/cli/grammar.rs) discards the exec payload it parses, and nothing implements ensure-running, so three of spec/12's contracts had no owner. Widening `Core`, adding items 1, 3, 4, and 6, and naming an ordered remainder is what the extra session buys. Item 7 was added in the same pass: the gated half needs a capable host, and every earlier slice that needed one carried the runbook that drives it rather than leaving the run to be arranged by hand.
