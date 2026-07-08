# ADR-0008: Separate the sandbox layer from the project's own dev environment

## Context and Problem Statement

Many projects already define their own development environment — a `flake.nix` with direnv and
nix-direnv, or an equivalent — that must keep working on bare metal, in CI, and unchanged inside a
sandbox. If nixvault entangles its sandbox configuration with the project's environment, it would
either rewrite the project's files or force every project to adopt nixvault to build at all.

## Considered Options

- **One combined environment** — nixvault owns both the VM and the in-VM developer environment,
  generating or absorbing the project's dev config.
- **Two independent layers** — nixvault owns only the outer sandbox; the project's own dev
  environment runs inside it, untouched.

## Decision Outcome

Chosen option: **two independent layers**. The outer layer (nixvault) boots the VM, mounts, network,
and security. The inner layer is the project's own environment, which evaluates *inside* the guest at
shell time and is never modified by nixvault. The project's dev config keeps working identically with
or without the sandbox.

To make the inner layer work, the guest ships a working Nix toolchain plus direnv, and the project's
directory is bind-mounted read-write so its files — including its own `flake.nix` and direnv config —
are present and load normally. See
[`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).

## Consequences

- Good: a project's environment is portable — identical in or out of the sandbox.
- Good: nixvault never edits repository files, so adoption is non-invasive.
- Good: clean mental model — the sandbox is the box; the project defines what runs inside it.
- Bad: the guest must include Nix and direnv, enlarging the base image.
- Bad: two Nix evaluations exist (outer build-time, inner shell-time), which can confuse newcomers.

## Status

Accepted
