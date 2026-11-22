# ADR-0116: The binary ships the first pin

## Context and Problem Statement

vivarium keeps two pins. A released `viv` renders branch references, so a target's first lock resolves them live. Development redirects them through an environment seam to the store paths of [`nix/flake.lock`](../../nix/flake.lock), found by a `nix flake archive` that fetches every input. On 2026-11-20 spectrum-os.org stopped answering, and that archive hung the suite. [`Q-009`](../plan/open-questions.md) asked whether the shipped tool carries a pin.

## Considered Options

- The binary carries the pin, and a pre-flight writes it.
- The branch references resolve live.
- A data file beside the binary.
- A fetch at first run.

## Decision Outcome

Chosen option: `the binary carries the pin`, because the first lock then follows from the binary's version.

`nix/flake.lock` is the one pin, and the host binary embeds it. When a target has no lock in force, a pre-flight runs Nix's lock step on the generated flake with the embedded bytes as the reference lock. It installs the result as the user's lock before anything evaluates. The generated flake keeps its branch references, and `viv` reads each from the `original` field of the embedded lock, so [`nix/flake.nix`](../../nix/flake.nix) alone names a branch. `nixpkgs` stays on `nixos-unstable`, which microvm.nix follows. Nothing reads the embedded copy afterward, and only `viv update` moves the user's lock. The suite takes a new user's path, with no redirect.

Every move of `nix/flake.lock` is a releasing `fix`, as Renovate marks a shipped dependency move: `fix(deps)`. A move of the root `flake.lock` ships nothing and stays `chore(deps)`.

A data file can drift from its binary, and a fetch is live resolution again.

## Consequences

- Good: the first lock is reproducible from the binary's version, and the suite needs no forge request, archive, or deadline.
- Neutral: the user gets no staleness warning and moves the lock with `viv update`. A developer sees the `.envrc` warning of a generic exobrain staleness helper.
- Bad: the shipped pin ages between releases, so moving it is a release obligation.
- Bad: a target whose first build fails still keeps a lock.

## Status

Accepted

Chooses the first exit of [`Q-009`](../plan/open-questions.md), a rendered baseline pin with a named update path, and awaits enactment.

Amends [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — the first lock comes from the pre-flight and the shipped pin, not from the first successful evaluation. The lock's home, its per-target scope, and that only `viv update` moves it survive.

Amends [`ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — the released pin move of an advisory is a move of `nix/flake.lock`, which the binary then ships as the first pin. No periodic job moves the pins. The routine clock becomes the 30-day warning of `exobrain outdated` in `.envrc` on the `nixpkgs` node of `nix/flake.lock`, and a person moves the lock. The advisory clock, the owner role, and the boundary trigger are unchanged, and both clocks end in the same `fix(deps)` pin move.

Leaves [`ADR-0112-the-selected-image-carries-the-base-flake.md`](./ADR-0112-the-selected-image-carries-the-base-flake.md) unchanged: a base's own `flake.lock` still seeds its subtree, because the reference lock holds no node under the base.

Extends [`ADR-0102-the-installation-supplies-vivarium.md`](./ADR-0102-the-installation-supplies-vivarium.md): the binary already supplies the product tree, and now it supplies the first pin too.
