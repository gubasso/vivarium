# ADR-0106: Gated runs put their bytes on the heavy drive

## Context and Problem Statement

[`../../tests/host/disk-preflight`](../../tests/host/disk-preflight) decides where a disk-heavy run's bytes go, and every hand-run lane calls it. Nothing at the gates did, so `git push` ran a suite that compiles the workspace, realises guest closures and boots microVMs onto the boot disk, which reached 100% partway through. The drive was configured throughout; `VIVARIUM_HEAVY_DRIVE` is absent from a hook's environment when git starts outside a direnv shell. A gate here cannot ask either: pre-commit runs hooks with stdin on `/dev/null`.

## Considered Options

- A wrapper that resolves the drive, binds it, and refuses when there is none
- Keep the advisory rule and rely on the developer
- Move the bytes that dominate — the Nix store — onto the drive instead

## Decision Outcome

Chosen option: `a wrapper that resolves the drive`. [`../../tests/host/heavy-run`](../../tests/host/heavy-run) calls `disk-preflight --require-drive`, binds `VIVARIUM_HEAVY_DRIVE`, `TMPDIR`, `CARGO_TARGET_DIR` and the state, data and cache roots under the answer, and execs. The runtime root stays put against the 108-byte socket limit, the config root because the tool only reads it. No usable drive means exit `69` and nothing ran; `--on-host`, or `VIVARIUM_HEAVY_ON_HOST` for a run git starts, waives the requirement and not the capacity check.

`VIVARIUM_HEAVY_DRIVE` names a directory under the mount, never the mountpoint: a subdirectory goes away with its filesystem, and its absence is how an unplugged drive is told from a mounted one. Deciding filesystem identity at run time has no spelling free of false refusals.

Moving the store is refused on a measurement, 2026-08-19. A chroot store keeps logical `/nix/store` paths, so substituters serve it, but shares nothing with the host store: realising `hello` fetched five paths, two already held, for 82M. A guest closure duplicates in gigabytes, and a booting guest cannot read one — the launch contract names host store paths.

## Consequences

- Good: the drive answer reaches the process that writes, not the shell that started it, and one compile cache is filled rather than two.
- Bad: that cache lives on removable media, so builds are slower and unplugging one mid-run fails in cargo's vocabulary.

## Status

Implemented

Enacted in `tests/host/heavy-run`, `disk-preflight --require-drive`, `.pre-commit-config.yaml`, `justfile`, `flake.nix` and `ci.yml`; asserted by `tests/heavy_gate.rs`.
