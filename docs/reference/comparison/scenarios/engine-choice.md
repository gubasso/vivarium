# Engine choice

Record whether the user can select which virtualization or container engine runs the workload.[^read]

1. Read the registration or manifest schema for an engine key.
2. Change it to a second engine and re-run.
3. Record whether the workload starts and what reports the change.

## vivarium

Nobody chooses: vivarium's [class-not-tool rule](../../spec/08-invariants-and-guarantees.md) fixes the boundary by capability class, so any backend satisfying the class is admissible and none is part of the contract. There is no manifest key to set. Not planned — the question does not mean anything for this design.

## flake-pilot

The user chooses, at registration: `podman-pilot` drives podman, whose OCI runtime is selected by `--opt "\--runtime=krun"` or by `runtime = "krun"` in `containers.conf`, so the reachable set is whatever podman accepts; `firecracker-pilot` drives firecracker. Upstream states the consequence directly — the `krun` runtime "gives isolation based on KVM and should be preferred for AI workloads" — so the rungs are not equivalent and the registration is where the difference is fixed.

Both rungs are read in these tables rather than one standing for the other, because a choice upstream leaves to the user at registration, and publishes a `claude` registration for either way of making, is not a choice this comparison gets to make on their behalf.

## glaipnir

The host probe chooses. There is no engine key: a passing `_check_microvm` selects libkrun, `--no-microvm` opts down to the container, and nothing selects among engines.

## bunkerbox

Nobody chooses, and the reason is different from vivarium's. There is no engine key in any of the four config surfaces because the runtime is a literal in the source, `io.containerd.kata.v2`. What that name resolves to is then Kata's own business: the hypervisor comes from the `configuration.toml` that `bunkerbox setup` symlinks into `/etc/kata-containers`, so the engine underneath is decided by a file the tool installs and never reads. vivarium reaches the same place by naming a capability class on purpose; bunkerbox reaches it by having one runtime and delegating what sits below it.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `flake-pilot` `main` on 2026-08-18, re-read 2026-08-20; `glaipnir` `21ef389` on 2026-08-19; `bunkerbox` `b7f14f3` on 2026-08-25.
