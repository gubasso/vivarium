# Engine choice

Record whether the user can select which virtualization or container engine runs the workload.

1. Read the registration or manifest schema for an engine key.
2. Change it to a second engine and re-run.
3. Record whether the workload starts and what reports the change.

## vivarium

vivarium `ceb0027`, 2026-08-18. Nobody chooses: vivarium's [class-not-tool rule](../../spec/08-invariants-and-guarantees.md) fixes the boundary by capability class, so any backend satisfying the class is admissible and none is part of the contract. There is no manifest key to set. Not planned — the question does not mean anything for this design.

## flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. The user chooses, at registration: `podman-pilot` drives podman, whose OCI runtime is selected by `--opt "\--runtime=krun"` or by `runtime = "krun"` in `containers.conf`, so the reachable set is whatever podman accepts; `firecracker-pilot` drives firecracker. Upstream states the consequence directly — the `krun` runtime "gives isolation based on KVM and should be preferred for AI workloads" — so the rungs are not equivalent and the registration is where the difference is fixed.

## glaipnir

glaipnir `21ef389`, read 2026-08-19. The host probe chooses. There is no engine key: a passing `_check_microvm` selects libkrun, `--no-microvm` opts down to the container, and nothing selects among engines.
