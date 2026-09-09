# Testing lanes

The register of what vivarium's suite is divided into, what each division needs, and where each one runs. Two taxonomies meet here and they are not the same thing: a proof lane is a claim plus the evidence admissible for it, decided in [`../decisions/ADR-0076-test-lanes-and-what-each-proves.md`](../decisions/ADR-0076-test-lanes-and-what-each-proves.md); an executable lane is a set of tests plus the host capability they need, decided in [`../decisions/ADR-0114-a-lane-runs-where-a-place-names-it.md`](../decisions/ADR-0114-a-lane-runs-where-a-place-names-it.md). The second is what a runner and a git hook select on. The first is what a reviewer asks a new test to belong to.

## The rules

1. A lane runs where a place names it, and nowhere else. No test decides for itself whether to run.
2. A lane declares what it needs, once, in its binary's `main`. An unmet need is a failure carrying the reason, never a skip. A skipped acceptance trial reads exactly like a passing one from outside, and [`implementation-status.md`](./implementation-status.md) already refuses to count one as evidence.
3. [`../../.pre-commit-config.yaml`](../../.pre-commit-config.yaml) is the only place a lane's command is written. `just`, `git`, and [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml) reach a lane by hook id and never repeat its command.
4. A place is an explicit list of hook ids: the `stages:` keys in the hook file, and the matrix in the workflow. No environment variable selects tests.
5. No profile carries `retries`. One clean run is evidence of possibility, not of reliability, and a retry converts the run that was not clean into a green one. A claim that needs a repeated observation runs its lane twice and says so.

## The executable lanes

| Lane          | Needs                                                                         | Selected by                | Where it runs                    |
| ------------- | ----------------------------------------------------------------------------- | -------------------------- | -------------------------------- |
| `unit`        | nothing                                                                       | `kind(lib) + kind(bin)`    | commit, push, CI                 |
| `local`       | the built `viv` and a filesystem                                              | `binary(local_*)`          | push, CI                         |
| `eval`        | `nix` on `PATH`                                                               | `binary(eval_*)`           | push, CI                         |
| `net`         | `unshare`, `nsenter`, `ip`, `nft`, and a creatable unprivileged user+net pair | `binary(net_*)`            | push, CI                         |
| `boot`        | `/dev/kvm` read-write, a systemd user manager, `XDG_RUNTIME_DIR`, `nix`       | `binary(boot_*)`           | push, when product paths changed |
| `doc`         | nothing                                                                       | `cargo test --doc`         | push, CI                         |
| `lane-<name>` | varies; each script says                                                      | one manual hook per script | by hand                          |

The need names the binary because nextest offers no other tag. Its filter language selects on package, kind, binary name, and test name, and there is no test attribute to filter on, so the lane a test belongs to is spelled in the file name that compiles it. [`../../Cargo.toml`](../../Cargo.toml) requires the prefix, [`../../.config/nextest.toml`](../../.config/nextest.toml) carries one profile per lane, [`../../tests/support/preflight.rs`](../../tests/support/preflight.rs) is where a binary asks its host for what it declared, and [`../../scripts/check-lane-register`](../../scripts/check-lane-register) fails a change that moves one of those four and not the others.

`unit` and `doc` carry no binary prefix. `unit` selects by kind across every crate in the workspace, and `doc` is `cargo test --doc`, which nextest cannot run at all.

Every lane hook passes `--workspace`. Without it cargo tests the root package alone, and `crates/vivarium-guest-agent`'s 37 unit tests never ran: not at the commit stage, not at push, and not in CI. Found on 2026-09-08 by counting what the two spellings select, 300 against 337.

## Running one

```console
$ just test unit          # or local, eval, net, boot, doc
$ just push-checks        # exactly what `git push` runs
$ just lane base-image    # one host lane, by name
```

Each of those invokes a hook. The long form works anywhere and is what CI's matrix calls:

```console
$ pre-commit run --all-files --hook-stage manual test-eval
```

## Where each lane runs, and why not elsewhere

`boot` is the one lane a place declines. It costs twenty-seven boots, roughly fifteen minutes, so at the push stage it is scoped by path: a push that changes nothing under `src/`, `crates/`, `nix/`, `tests/boot_*`, `tests/support/`, `Cargo.toml`, `Cargo.lock`, or `rust-toolchain.toml` pays nothing. Under `--all-files` it always runs.

No runner runs `boot` or `net` today, and the reason is the same one for both. Measured on 2026-09-09, a standard GitHub runner carries `/dev/kvm` at mode `0666` and a systemd user manager that answers after `loginctl enable-linger`, so neither virtualization nor [`../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md`](../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md)'s transient unit is what stops them. What stops them is the unprivileged user namespace: every VM's network lives in its own host-side user and network namespace pair ([`spec/05-networking-and-egress.md`](spec/05-networking-and-egress.md)), and Ubuntu's `AppArmor` policy refuses to create one, reporting `write failed /proc/self/uid_map: Operation not permitted`.

The `boot-experiment` job is what would notice that changing. It carries `continue-on-error` and is absent from the gate's `needs`, so it holds no merge, and its capability step reports each premise by name. Two consecutive green runs and the namespace premise held are what would promote it.

The workflow declares every push-staged hook no runner runs, with a reason each, in its `NOT_ON_A_RUNNER` list. The register check fails a push-staged hook that appears neither there nor in the matrix, so a check that quietly stops running in CI cannot exist unremarked.

## The host lanes

The measurement and acceptance scripts under [`../../tests/host/`](../../tests/host/) are `manual` hooks named `lane-<name>`. Each boots a real guest, measures a real store, or both; several take tens of minutes, and two remove from the invoking user's own store. None of that belongs at a git stage or on a runner. [`microvm-verification-harness.md`](./microvm-verification-harness.md) says what each one proves and how it is read.

They are declared as hooks rather than left as bare scripts for two reasons. They appear in one register beside every other lane, and the register check holds the list against the filesystem, so a script added without a hook is a failure rather than a lane nobody remembers.

A script that selects trials by name passes `--ignore-default-filter`, and that flag is load-bearing. A command-line `-E` is applied within the active profile's `default-filter` rather than instead of it, and `profile.default` selects `kind(lib) + kind(bin) + binary(local_*)`, so without the override a filterset naming trials in two lanes silently selects only the `local` one. Measured 2026-09-09 on `tests/host/exec-and-shell-check`, which listed one trial of its three and would have reported a pass having booted nothing.

## The proof lanes, and where each one runs

ADR-0076's six lanes are claims. This is where each one is proved today.

| # | Proof lane           | Proves                                                              | Executable lane   |
| - | -------------------- | ------------------------------------------------------------------- | ----------------- |
| 1 | Unit                 | parse, resolve, precedence, placement, validation boundaries        | `unit`            |
| 2 | Structured golden    | the generated flake and the `--json` records are what the spec says | `local`           |
| 3 | Text-contract golden | usage output and the human error skeleton                           | `local`           |
| 4 | Evaluation           | the generated flake evaluates                                       | `eval`            |
| 5 | Purity               | N3, N19 — no launch-channel value in a build input                  | `lane-base-image` |
| 6 | Non-invasion         | N9 — no user file is touched                                        | `local`           |
| — | Acceptance           | end-to-end behavior                                                 | `boot`            |

Lanes 2, 3, and 6 have no dedicated home yet: their helpers exist — `snapshot_tree` and `expect_tree_unchanged` for non-invasion, the `json` module for the structured comparisons — and the assertions are folded into `local` trials rather than gathered. Building them out is [slice 007](../plan/slices/007-build-test-lanes/README.md)'s remaining scope. What that slice no longer owns is the profile work, which ADR-0114 enacted.

Each invariant is proved by the cheapest lane that can prove it. Everything outside `eval`, `boot`, and the host lanes runs with no Nix, no network, and no virtualization, and asserts on a rendered artifact — a generated file, a printed record, an exit code — rather than on internal state. That is what makes the bulk of the suite meaningful on a contributor's laptop and on a runner that has none of the three.

## Both egress modes

The two egress modes ([`spec/05-networking-and-egress.md`](spec/05-networking-and-egress.md)) are covered twice, deliberately:

- in `eval`, as a golden — `mode = "open"` and `mode = "allowlist"` render different, snapshot-stable guest configuration, with the allowlist case using two names on different addresses. This catches a policy regression with no VM and no network.
- in `boot`, end to end, where the policy is actually enforced.

The duplication is the point: the first is fast and always runs, the second is authoritative and runs where a guest can start.

## Two rules that keep the suite honest

A disk-heavy lane asks where its bytes go before it writes any: [`../../tests/host/disk-preflight`](../../tests/host/disk-preflight), the rule in [`../../AGENTS.md`](../../AGENTS.md), and [`microvm-verification-harness.md`](./microvm-verification-harness.md) for how the drive is configured and what happens when it is not there. This is a rule about honesty rather than about capacity — a lane that fills the disk halfway through reports a failure that belongs to the disk, in the vocabulary of whatever change was under test.

At the commit and push stages the same rule is machinery rather than convention: every hook that compiles, evaluates a flake, or boots runs through [`../../tests/host/heavy-run`](../../tests/host/heavy-run), which binds the resolved drive into the hook's environment and refuses when there is none ([`../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md`](../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md)). The gate's own contract is a lane in this register, not a note about one: `tests/local_heavy_gate.rs` asserts what a gated run's child is handed and that a refused run's command never starts.

Every golden file names the spec line it encodes, in a header comment. Without that, accepting a diff is a keystroke rather than a decision, and a snapshot nobody traced back to the spec quietly becomes the contract — which inverts the resolution rule in [`../../AGENTS.md`](../../AGENTS.md).

Mutation testing runs on a schedule against the parser, resolver, and precedence modules — never per-change. It guards the specific risk of a suite that is mostly snapshots: assertions that would not notice if the code changed underneath them.
