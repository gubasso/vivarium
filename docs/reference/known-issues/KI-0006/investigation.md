# KI-0006 investigation

## Symptom

On 2026-11-20 the `ci` workflow failed on the trunk commit that passed on 2026-11-12. Run 44750215556 shows two failures:

- `test-local`: 42 trials, 29 passed and 13 timed out at the 120 s `slow-timeout`. The 13 are every trial of `local_workflows` that runs a command through `run_viv`, `harness_self_check` included.
- `test-eval`: 7 trials, 4 passed slow and 3 timed out at 360 s: `workflow_04_inspect_before_run`, `workflow_05_restrict_egress_config_surface`, and `workflow_23_agent_channel_config_surface`.

## Root cause

`run_viv` pinned the baseline inputs through `nix flake archive` over the product flake. That command fetches every input of `nix/flake.lock` recursively, `spectrum` among them, and `spectrum` lives on `https://spectrum-os.org/git/spectrum`. Measured from a developer host on 2026-11-20:

- `curl -4`, `curl -6`, and `http://` to `spectrum-os.org` time out. `github.com` answers in 0.2 s.
- `git ls-remote https://spectrum-os.org/git/spectrum` still waits at 60 s.
- On an empty chroot store, `nix flake prefetch` of the locked `nixpkgs` and `microvm` takes 12 s and 1 s and leaves no `spectrum` path. `nix flake archive` on the same store fetches `flake-utils` and `systems` and then blocks until `timeout 120` kills it.

In microvm.nix at the locked revision, `spectrum` feeds one overlay attribute, `cloud-hypervisor-graphics`. Nix fetches an input when an evaluation first reads it, so the `product-flake-check` lane passed on the same run.

The last green run on 2026-11-12 possibly took the `spectrum` source from the Actions cache of the Nix store. That cache drops an entry after seven days with no use. The evidence does not show it.

## The guard

- `baseline_inputs` in [`tests/support/mod.rs`](../../../../tests/support/mod.rs) prefetches only `nixpkgs` and `microvm`, each at the `narHash` that the lock records, and it gives up after 90 s. A failure fails the trial with the stderr of `nix`. It no longer falls back to a live evaluation.
- `output_within` kills the process group of a command at its deadline, so the `git` that `nix` started dies with it.

Two cases stay exposed. A trial whose evaluation reads the overlay still fetches `spectrum`. A user evaluation that reads it on a cold store blocks the same way. The second case is inferred and not measured.

## Recheck condition

Run `git ls-remote https://spectrum-os.org/git/spectrum`, as [`../../tracking.yaml`](../../tracking.yaml) schedules. When the host answers and `nix flake archive` of the product flake completes on a cold store, collapse this dossier under `resolved/`. If the host stays down, the decision about the `spectrum` input belongs to the product and not to this case.
