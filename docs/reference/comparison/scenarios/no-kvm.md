# No KVM

Record what happens on a host with no `/dev/kvm`.

1. Confirm `/dev/kvm` is absent or unreadable.
2. Run the tool's default invocation.
3. Record the exit status and the message.

## vivarium

vivarium `ceb0027`, 2026-08-18. Refuses: the [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) admits no shared-kernel mode, so a host without KVM is a host vivarium does not run on. `viv doctor` reports it as a hard failure and `viv start`'s preflight consumes the same probe. Not planned — a fallback would make the boundary a default rather than a guarantee.

## flake-pilot

flake-pilot `main`, read 2026-08-19. The microVM registrations do not start: firecracker and `krun` both require `/dev/kvm`. Only `crun` container registrations run. Nothing falls back — a registration names one engine, and its failure is a failure.

## glaipnir

glaipnir `21ef389`, read 2026-08-19. Falls back: a failed `_check_microvm` proceeds in a plain container with a warning. The stance is stated — a weaker sandbox beats no sandbox.

## podman

podman 5.x, read 2026-08-19. `--runtime krun` cannot create the VM without `/dev/kvm`; the default `crun` runtime runs anywhere. Nothing falls back on its own — the failing flag is the user's to remove.
