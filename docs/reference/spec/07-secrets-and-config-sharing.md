# 07 — Secrets and config sharing

How a project's sandbox configuration is shared across a team without leaking personal or secret
data. Two rules govern this: secrets never enter the build (see
[`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)),
and machine-specific paths are injected at launch, not built in (see
[`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md)).

## The shared / personal split

Configuration divides into two classes:

- **Shared** — portable, safe to share in a config library: images, pieces, and the manifest itself.
  These contain no personal paths and no secrets, so a team can track and distribute them together.
- **Personal / machine-local** — never shared: a user's resource overrides, machine identity, and
  anything host-specific. These live in the user's own per-user config and state (see
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)) and are injected at launch, never
  written into a shared artifact.

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

## Mirroring host configuration

Personal config files and directories are mirrored into the guest through the declarative
`[[mounts]]` schema
([`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md),
[`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md)): the host
`source` expands host-side variables at launch, the guest `target` expands `~` to the guest home,
and `readonly = true` marks identity files that must not be written. Because each side expands its
own home, identity mounts (`~/.config/foo` → `~/.config/foo`) need no path translation even though
the guest username differs from the host's.

Mount declarations resolve at launch and are never build inputs (N5): an unset variable or missing
host path fails before boot with a legible error, and no expanded path is ever recorded in a shared
artifact (N11). One caveat: mirroring is transparent at the *path* layer only — a mirrored file
whose contents embed a host-absolute path is not rewritten. Runtime environment values pass through
the `[env]` table, subject to the deny-by-default rule (N17).

## Enforcing the split

Because the working-directory path is never written to config and secrets are never built in, the
shared class stays genuinely shareable. Tooling guards the boundary with **read-only checks** —
rejecting a shared artifact that contains an absolute home path or an inline secret — never by writing
into the user's config (N13). The personal class already has a home in the per-user config and state
roots; vivarium does not scaffold it.
