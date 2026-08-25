# Testing lanes

> The host-tier work these lanes cannot reach — booting a guest and reading its console — is done today by the hand-run scripts in [`microvm-verification-harness.md`](./microvm-verification-harness.md), which owns what each one proves; `store-gc-interlock-check` and `generations-check` are called out here because they are the two that remove from the host store — the first by targeted deletion, the second by running the whole-store sweep `viv gc` ships — and `guest-agent-check` because it is the one whose host tier drives a Rust trial rather than a shell probe, running `tests/guest_agent_host.rs` twice against a booted guest. They are the base a host lane grows from, and their two-tier gate is the same distinction the lanes below draw. Note its extra prerequisite when that lane is automated: it needs a store this user may delete from, so on a default multi-user Nix it skips wholesale rather than failing.

The named lanes vivarium's test suite is organized into, what each one proves, and which are required. The lane taxonomy is decided in [`../decisions/ADR-0076-test-lanes-and-what-each-proves.md`](../decisions/ADR-0076-test-lanes-and-what-each-proves.md); the two absence-proving lanes get their own decision in [`../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md`](../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md). What is implemented today is tracked in [`implementation-status.md`](./implementation-status.md), which this page does not restate. The executable form of this taxonomy will be one profile per lane in `.config/nextest.toml`; today that file carries four stage profiles — `default`, `pre-commit`, `pre-push`, and `ci` — which encode the commit/push split rather than these lanes, and the per-lane profiles remain slice `007`'s work.

## The unifying rule

Every lane below except Evaluation and Purity runs with no Nix, no network, and no virtualization, and asserts on a rendered artifact — a generated file, a printed record, an exit code — rather than on internal state. That is what makes the bulk of the suite meaningful on a contributor's laptop and on a runner that has none of the three. The two exceptions need Nix, because each one's evidence is something only Nix produces — an evaluation and a derivation graph — but neither needs network or virtualization.

Each invariant is proved by the cheapest lane that can prove it. A lane is not a topic; it is a claim plus the evidence admissible for it.

## The lanes

| # | Lane                 | Proves                                                              | Needs                | Required             |
| - | -------------------- | ------------------------------------------------------------------- | -------------------- | -------------------- |
| 1 | Unit                 | parse, resolve, precedence, placement, validation boundaries        | nothing              | yes                  |
| 2 | Structured golden    | the generated flake and the `--json` records are what the spec says | nothing              | yes                  |
| 3 | Text-contract golden | usage output and the human error skeleton                           | nothing              | yes                  |
| 4 | Evaluation           | the generated flake evaluates                                       | Nix                  | where Nix is present |
| 5 | Purity               | N3, N19 — no launch-channel value in a build input                  | Nix                  | where Nix is present |
| 6 | Non-invasion         | N9 — no user file is touched                                        | nothing              | yes                  |
| — | Acceptance (gated)   | end-to-end behavior                                                 | Nix + virtualization | informational        |

### 1 — Unit

Manifest and `inputs.toml` parsing, closed-key rejection, name resolution against the config libraries, the manifest-resolution precedence chain, XDG root placement and the unset/empty/relative rules, and secret-path validation.

Property tests cover two things and are not spread further: that the key table round-trips and rejects every unknown key, and that manifest precedence ([`spec/02-config-and-xdg-layout.md`](spec/02-config-and-xdg-layout.md)) is total and independent of the order the sources are probed in. Everywhere else, table-driven cases are clearer about which spec line they encode.

Unit tests get the same XDG isolation the acceptance harness has — a temporary project with all five roots — rather than a second, weaker fixture.

### 2 — Structured golden

Snapshots of the generated flake tree, of each `--json` record, and of `config eval` output.

Four classes of value are nondeterministic and are filtered before comparison: absolute XDG paths, fixture manifest names, store hashes, and timestamps. A snapshot that still contains one of them is a defect in the test, not a reason to re-record.

CI fails on a missing or changed snapshot rather than writing one. Re-recording is an explicit local act, reviewed like any other change.

### 3 — Text-contract golden

Usage and `--help` output, and the five-slot human error skeleton — `error[<id>]:`, the location line, `why:`, `accepted here:`, `hint:` ([`spec/14-exit-codes.md`](spec/14-exit-codes.md)). These are literally text contracts, so they are compared as transcripts rather than as structured snapshots.

### 4 — Evaluation

That the generated flake evaluates — not that it matches a snapshot. A byte-identical generated flake can still fail to evaluate, so lane 2 does not subsume this one.

It evaluates without building: no derivation is realized and no VM is booted. It therefore runs at the middle rung of the existing three-level runtime gate — Nix present, virtualization absent — which is the rung that gate was built for and which nothing used before this lane existed.

### 5 — Purity

Two assertions, both structural. Neither scans for values that look suspicious; a deny-list is the heuristic [`../decisions/ADR-0069-redaction-is-by-construction.md`](../decisions/ADR-0069-redaction-is-by-construction.md) refused for redaction, and it has the same unbounded tail here.

- Canary. Each run plants a unique random token in every launch-channel value, then asserts the token appears nowhere in the recursive derivation graph — input sources, input derivations, and environment. For the value it tracks, a unique token has no false negatives.
- Metamorphic equality. The same manifest is built twice with only launch-channel data changed. The derivation must be identical either way. This states N3 and N19 as an equality rather than as a search, so it catches a leak nobody thought to look for.

Both halves narrowed with [`../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md`](../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md), and the narrowing is the point rather than a concession. A declared workspace's expanded path is now build-channel: it is the guest path the tree is mounted at, so a build output depends on it by design. Planting a canary there, or building one manifest from two host paths and demanding one derivation, would now assert the opposite of what the product promises. What the lane still proves is that nothing which resolves at launch — a `[[mounts]]` source, `[env]`, `[resources]` — reaches a build input, which is the whole of N19 as it now reads.

The derivation-inspection format this lane reads is documented upstream as experimental. That is a known dependency: if it changes, this lane changes with it, and the invariants it proves do not.

### 6 — Non-invasion

Every command runs against a fixture project whose complete tree and version-control status are captured before and after. No vivarium-owned file is permitted inside that tree (N9, [`spec/08-invariants-and-guarantees.md`](spec/08-invariants-and-guarantees.md)); every command must leave it byte-identical.

The comparison is automatic in the fixture, not an opt-in helper. An opt-in invariant check is the one a new trial forgets to call.

Kernel-enforced write confinement would prove more, but it needs privileges or kernel features many hosts lack, which turns the assertion into a skip — and an observation of the execution environment is never a fact about a host ([`../../AGENTS.md`](../../AGENTS.md)).

## Both egress modes

The two egress modes ([`spec/05-networking-and-egress.md`](spec/05-networking-and-egress.md)) are covered twice, deliberately:

- as a golden — `mode = "open"` and `mode = "allowlist"` render different, snapshot-stable guest configuration, with the allowlist case using two names on different addresses. This catches a policy regression with no VM and no network.
- end-to-end, in the gated acceptance lane, where the policy is actually enforced.

The duplication is the point: the first is fast and always runs, the second is authoritative and usually cannot.

## Two rules that keep the suite honest

A disk-heavy lane asks where its bytes go before it writes any: [`../../tests/host/disk-preflight`](../../tests/host/disk-preflight), the rule in [`../../AGENTS.md`](../../AGENTS.md), and [`microvm-verification-harness.md`](./microvm-verification-harness.md) for how the drive is configured and what happens when it is not there. This is a rule about honesty rather than about capacity — a lane that fills the disk halfway through reports a failure that belongs to the disk, in the vocabulary of whatever change was under test.

At the commit and push stages the same rule is machinery rather than convention: every hook that compiles, evaluates a flake, or boots runs through [`../../tests/host/heavy-run`](../../tests/host/heavy-run), which binds the resolved drive into the hook's environment and refuses when there is none ([`../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md`](../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md)). The gate's own contract is a lane in this taxonomy, not a note about one: `tests/heavy_gate.rs` asserts what a gated run's child is handed and that a refused run's command never starts.

Every golden file names the spec line it encodes, in a header comment. Without that, accepting a diff is a keystroke rather than a decision, and a snapshot nobody traced back to the spec quietly becomes the contract — which inverts the resolution rule in [`../../AGENTS.md`](../../AGENTS.md).

Every lane needs at least one ungated trial. The test runner treats a run in which every matching trial was ignored as a failure, so a lane whose trials are all gated reports a failure that means nothing. The existing `harness_self_check` is the pattern.

## What CI requires

Lanes 1, 2, 3, and 6 are required on every change and run anywhere. Lanes 4 and 5 are required wherever Nix is present — which includes CI, so in practice both gate every change; a contributor without Nix runs the other four and CI supplies the rest.

The gated acceptance lane stays informational until a virtualization-capable runner exists. A skipped trial is never implementation evidence — that rule is [`implementation-status.md`](./implementation-status.md)'s, and it is why the middle status level exists.

The profiles state that grading as of 2026-08-14. `profile.pre-push` selects plain `kind(test)` with no subtraction: slice 004 deleted the last excluded acceptance trial when the egress allowlist became enforceable, so the stage is a real gate on a capable host. `profile.ci` still runs everything. The history of the subtractions — whole binary, then three named trials, then one, then none — and the obligation any future subtraction re-opens belong to [`../plan/sequencing.md`](../plan/sequencing.md).

Mutation testing runs on a schedule against the parser, resolver, and precedence modules — never per-change. It guards the specific risk of a suite that is mostly snapshots: assertions that would not notice if the code changed underneath them.
