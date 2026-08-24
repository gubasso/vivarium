# ADR-0009: Inject the workspace path at launch time, not into the build

## Context and Problem Statement

The sandbox must mount the user's working directory, whose absolute host path differs per user and per machine. If that path becomes an input to the Nix build, the built VM is no longer a pure function of the manifest: the same manifest yields different store outputs on different machines, committed config leaks personal paths, and reproducibility is lost.

## Considered Options

- Bake the path into the build — pass the working-directory path as a build input (for example through an impure environment read), so the VM definition contains it.
- Inject the path at launch time — keep the build path-free and pure; the tool adds the working-directory share when it launches the VM, as a runtime concern.

## Decision Outcome

Chosen option: inject the path at launch time. The build takes no host path; by convention the working directory is always mounted to a fixed in-guest location, so no host path appears in any configuration file. The tool supplies the actual host directory only when starting the VM.

This preserves the core determinism guarantee: identical manifest and lockfile produce an identical VM on every machine and in CI. See [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and the determinism invariant in [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).

## Consequences

- Good: the built VM is reproducible and path-free; committed config never leaks personal paths.
- Good: the same manifest works for every user and in CI without edits.
- Bad: the mount is a runtime step the launcher must perform, not visible in the build output.
- Bad: additional machine-local paths (extra data dirs) must follow the same runtime-injection rule rather than being written into config.

## Status

Superseded

Superseded by [`ADR-0110-the-workspace-is-an-ordinary-mount.md`](./ADR-0110-the-workspace-is-an-ordinary-mount.md) — the path this record kept out of the build is now a build input. A workspace is declared in the manifest rather than taken from the invoking directory, so it is known before evaluation, and compiling it to an ordinary mount deletes the launch channel, the percent-encoder, and the guest unit that carried it. The determinism bought here is retracted with it: one manifest now evaluates to different store outputs on two machines.

Amended by ADR-0048 — states how the injection is achieved. vivarium generates the launch itself rather than consuming the upstream runner package, because that package writes each share's host source path into the build output, which is exactly what this decision forbids.
