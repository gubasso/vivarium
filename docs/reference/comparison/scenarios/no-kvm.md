# No KVM

Record what happens on a host with no `/dev/kvm`.[^read]

1. Confirm `/dev/kvm` is absent or unreadable.
2. Run the tool's default invocation.
3. Record the exit status and the message.

## vivarium

Refuses: the [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) admits no shared-kernel mode, so a host without KVM is a host vivarium does not run on. `viv doctor` reports it as a hard failure and `viv start`'s preflight consumes the same probe. Not planned — a fallback would make the boundary a default rather than a guarantee.

## flake-pilot

The microVM registrations do not start: firecracker and `krun` both require `/dev/kvm`. Only `crun` container registrations run. Nothing falls back — a registration names one engine, and its failure is a failure.

## glaipnir

Falls back: a failed `_check_microvm` proceeds in a plain container with a warning. The stance is stated — a weaker sandbox beats no sandbox.

## podman

`--runtime krun` cannot create the VM without `/dev/kvm`; the default `crun` runtime runs anywhere. Nothing falls back on its own — the failing flag is the user's to remove.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `flake-pilot` `main`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-19.
