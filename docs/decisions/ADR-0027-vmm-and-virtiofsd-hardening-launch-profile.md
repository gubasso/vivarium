# ADR-0027: VMM and virtiofsd hardening launch profile

## Context and Problem Statement

ADR-0024 made N20 normative — the VMM must run under a host-side seccomp plus capability-drop
sandbox — and ADR-0025 said every virtiofsd instance is "sandboxed to the same standard," but that
standard was never written down and N20's wording covered only the VMM. A documented virtiofsd
escape (CVE-2026-47243) shows the shared-filesystem daemon is itself an escape vector: an
unsandboxed virtiofsd running as root let a guest-root process create symlinks to absolute host
paths via raw FUSE, yielding host-root. The enactment must be concrete: what confinement, applied
where, over which processes.

## Considered Options

- Leave N20 as VMM-only prose and treat confinement as an implementation detail.
- Verify confinement with a runtime `viv doctor` probe.
- Enact N20 as a by-construction launch profile owned by the launch wrapper, covering the VMM and
  every virtiofsd, implemented for Cloud Hypervisor only.

## Decision Outcome

Chosen option: **a by-construction launch profile**.

- The **launch wrapper is the enforcement layer** (not the guest, not a probe): unprivileged
  execution, `PR_SET_NO_NEW_PRIVS`, capability drop toward zero, a seccomp filter, and cgroup limits,
  applied before `exec`.
- **Cloud Hypervisor** runs with built-in seccomp enabled and a Landlock path allowlist; the concrete
  flags live in spec/13.
- **N20 extends to virtiofsd:** each per-share daemon runs unprivileged, `--sandbox=namespace`,
  seccomp on, `cache=none`, one process per share, with only that share writable. CVE-2026-47243 is
  the negative spec.
- **QEMU is documentation-only:** admissible under N2 but unhardened; Cloud Hypervisor is the one
  shipped hardened path.

## Consequences

- Good: N20 becomes a checkable by-construction property and the virtiofsd escape class is closed at
  the source.
- Good: the spec stays backend-agnostic while shipping one hardened path.
- Bad: the profile must be built and maintained; QEMU is unfit for untrusted workloads until hardened.

## Status

Accepted

Extends **N20** ([`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md))
to host-side filesystem daemons and enacts ADR-0024 / ADR-0025's confinement standard. The concrete
recipe and prerequisite probes live in
[`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md);
QEMU stays admissible under N2.
