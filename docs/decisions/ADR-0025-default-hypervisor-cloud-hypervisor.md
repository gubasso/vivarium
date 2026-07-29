# ADR-0025: Default hypervisor — Cloud Hypervisor

## Context and Problem Statement

ADR-0024 fixed the security property class every backend must satisfy and deliberately deferred the shipped default. Two microvm.nix-supported candidates satisfy the class: Cloud Hypervisor and QEMU's microvm machine type. The default must be safe, usable, battle-tested, and closest to N20/N16 out of the box.

## Considered Options

- QEMU (microvm machine type) — microvm.nix's own default, most-trodden path.
- Cloud Hypervisor — Rust VMM, virtio-only device model.

## Decision Outcome

Chosen option: **Cloud Hypervisor**, weighed on the criteria in priority order:

- **Security surface.** A memory-safe Rust VMM, roughly twenty times smaller than QEMU, exposing only virtio devices. QEMU's microvm profile is deliberately minimal but remains a C codebase carrying QEMU's CVE stream.
- **N20 fit.** Cloud Hypervisor enables per-thread seccomp filtering by default and supports Landlock; QEMU's `-sandbox` seccomp is off by default. Both still require vivarium's launch wrapper (capability drop, unprivileged execution); Cloud Hypervisor simply starts closer to N20.
- **N16 parity.** Both share the workspace identically: virtio-fs served by a per-share virtiofsd process. No differentiator.
- **microvm.nix maturity.** QEMU is better-trodden — the one criterion it wins — but Cloud Hypervisor is first-class there and avoids QEMU's host-side vsock port conflicts across concurrent VMs.
- **Boot feel.** Sub-second direct-kernel boot versus multiple seconds; material to `viv start`/`viv exec`.
- **Upstream health.** Foundation-hosted, corporate-backed, disciplined release cadence.

The default is a **hardened launch profile**, not a bare hypervisor selection: seccomp kept enabled, capability drop, unprivileged execution, Landlock where available, and every virtiofsd instance sandboxed to the same standard (N20).

QEMU's microvm machine type **remains admissible** under N2 as the fallback; the contract stays class-based (ADR-0024).

## Consequences

- Good: the smallest admissible attack surface plus the fastest boot become the default.
- Good: seccomp-on-by-default shortens the distance to N20.
- Bad: leaves microvm.nix's most-tested path; integration sharp edges are more likely ours to hit first.
- Bad: virtio-fs fidelity for the RW workspace (UID/GID mapping, watch semantics, throughput) must be verified during implementation.

## Status

Accepted

Amended by **ADR-0035** — the launch profile additionally requires the backend arguments that make guest memory elastic: shared guest memory (a prerequisite of filesystem sharing), a zero-size balloon with free page reporting and deflate-on-OOM, and a per-VM control socket so `viv trim` can act on a running VM.

Amended by **ADR-0049** — the backend is a member of the built runner's closure, not a host binary, so its version is settled by the lockfile rather than probed. The feature floors this default must meet are therefore evaluation-time assertions: free page reporting on the balloon device (ADR-0035), Landlock in the hardened profile (ADR-0027), and block-device discard with sparse images ([`ADR-0037-volume-disk-format-and-reclamation.md`](./ADR-0037-volume-disk-format-and-reclamation.md)). The recommended version named in spec/13 was always a tested baseline, never the first release carrying any of them.

Amended by **ADR-0048** — vivarium generates this backend's launch arguments itself rather than consuming the upstream runner package.

Discharges the deferral in ADR-0024. The binaries this default resolves to are named in [`ADR-0049-backend-is-a-closure-member.md`](./ADR-0049-backend-is-a-closure-member.md). The hardened launch profile it names (VMM + virtiofsd confinement) is enacted by [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md).
