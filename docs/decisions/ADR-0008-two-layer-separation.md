# ADR-0008: Separate the sandbox layer from the project's own dev environment

## Context and Problem Statement

Many projects already define their own development environment — a `flake.nix` with direnv and nix-direnv, or an equivalent — that must keep working on bare metal, in CI, and unchanged inside a sandbox. If vivarium entangles its sandbox configuration with the project's environment, it would either rewrite the project's files or force every project to adopt vivarium to build at all.

## Considered Options

- One combined environment — vivarium owns both the VM and the in-VM developer environment, generating or absorbing the project's dev config.
- Two independent layers — vivarium owns only the outer sandbox; the project's own dev environment runs inside it, untouched.

## Decision Outcome

Chosen option: two independent layers. The outer layer (vivarium) boots the VM, mounts, network, and security. The inner layer is the project's own environment, which evaluates inside the guest at shell time and is never modified by vivarium. The project's dev config keeps working identically with or without the sandbox.

To make the inner layer work, the guest ships a working Nix toolchain plus direnv, and the project's directory is bind-mounted read-write so its files — including its own `flake.nix` and direnv config — are present and load normally. See [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).

## Consequences

- Good: a project's environment is portable — identical in or out of the sandbox.
- Good: vivarium never edits repository files, so adoption is non-invasive.
- Good: clean mental model — the sandbox is the box; the project defines what runs inside it.
- Bad: the guest must include Nix and direnv, enlarging the base image.
- Bad: two Nix evaluations exist (outer build-time, inner shell-time), which can confuse newcomers.

## Status

Accepted

Amended by [`ADR-0029-project-identity-and-marker.md`](./ADR-0029-project-identity-and-marker.md) — vivarium may manage a self-ignored `.vivarium/` runtime marker in the project tree; it carries project identity only and is inert to the inner dev environment, so the two-layer separation stands.
