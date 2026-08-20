# Shared kernel

Record whether the tool offers a mode where the workload is separated by namespaces and cgroups on the host kernel rather than by a kernel of its own.

1. Read the tool's engine or runtime list for an ordinary OCI runtime.
2. Start the workload through it.
3. Inside, run `uname -r` and compare it with the host value.
4. Record what the tool itself says about the isolation this mode gives.

## vivarium

vivarium `ceb0027`, 2026-08-18. No, by rule: vivarium's [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) fixes a hardware-virtualization boundary with no shared-kernel mode. Not planned — a second, weaker mode would turn the guarantee into a default.

## flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Yes: `podman-pilot` at podman's default runtime is the upstream README's first `claude` registration, and its cost is legible in the registration itself — a shared kernel, `--net host`, and `~/ai` as the one shared path. It is also the rung with the best ergonomics, and upstream says why: the `krun` handler does not support `exec`, so a `krun` registration cannot use `--resume` either.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: rootless podman is the baseline and the microVM is an upgrade on top. `--no-microvm` selects the baseline outright; a failed probe lands on it with a warning; on macOS it is the only mode.
