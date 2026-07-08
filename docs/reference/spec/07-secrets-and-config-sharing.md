# 07 — Secrets and config sharing

How a project's sandbox configuration is shared across a team without leaking personal or secret
data. Two rules govern this: secrets never enter the build (see
[`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)),
and machine-specific paths are injected at launch, not built in (see
[`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md)).

## The shared / personal split

Configuration divides into two classes:

- **Shared** — portable, safe to commit: images, pieces, the committed manifest pointer, and the
  manifest itself. These contain no personal paths and no secrets.
- **Personal / machine-local** — never committed: a user's resource overrides, machine identity, and
  anything host-specific. These live in the gitignored personal-override file (see
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)) or the per-user config.

The guiding principle: **committed files describe the project; the machine and the user supply their
own context at the edges.** A committed file that contains an absolute home path or an inline
credential has crossed the line and belongs in the personal class instead.

## Eliminating machine-specific paths

The most common leak is an absolute working-directory path. It is eliminated by convention: the
working directory is always mounted to the same fixed in-guest location and injected at launch, so no
host path appears in any configuration file. Other machine-local paths follow the same rule — supplied
at launch, not written into shared config.

## Secrets

A secret embedded during the build lands in the world-readable store and ships with the VM's closure,
so build-time secrets are prohibited. Two safe channels replace them:

- **Encrypted-at-rest, for shared secrets.** Commit encrypted secret files that decrypt only at
  activation into a runtime-only location, never into the store. Only the public recipient
  identities are committed in clear.
- **Runtime injection, for personal secrets.** Forward an authentication-agent socket into the guest,
  bind-mount credential directories read-only, and pass tokens as runtime environment when the VM
  launches. None of these involve the build.

The rule is: **build-time means in the store, which is wrong for secrets; secrets are runtime or
encrypted-at-rest only.**

## Enforcing the split

Because the working-directory path is never written to config and secrets are never built in, the
shared class stays genuinely shareable. Tooling can guard the boundary by rejecting a committed file
that contains an absolute home path or an inline secret, and by scaffolding the gitignored
personal-override file at initialization so the personal class has a home from the start.
