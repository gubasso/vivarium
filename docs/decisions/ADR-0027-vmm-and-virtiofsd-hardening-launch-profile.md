# ADR-0027: VMM and virtiofsd hardening launch profile

## Context and Problem Statement

ADR-0024 made N20 normative — the VMM must run under a host-side seccomp plus capability-drop sandbox — and ADR-0025 said every virtiofsd instance is "sandboxed to the same standard," but that standard was never written down and N20's wording covered only the VMM. A documented virtiofsd escape (CVE-2026-47243) shows the shared-filesystem daemon is itself an escape vector: an unsandboxed virtiofsd running as root let a guest-root process create symlinks to absolute host paths via raw FUSE, yielding host-root. The enactment must be concrete: what confinement, applied where, over which processes.

## Considered Options

- Leave N20 as VMM-only prose and treat confinement as an implementation detail.
- Verify confinement with a runtime `viv doctor` probe.
- Enact N20 as a by-construction launch profile owned by the launch wrapper, covering the VMM and every virtiofsd, implemented for Cloud Hypervisor only.

## Decision Outcome

Chosen option: a by-construction launch profile.

- The launch wrapper is the enforcement layer (not the guest, not a probe): unprivileged execution, `PR_SET_NO_NEW_PRIVS`, capability drop toward zero, a seccomp filter, and cgroup placement, applied before `exec`.
- Cloud Hypervisor runs with built-in seccomp enabled and a Landlock path allowlist; the concrete flags live in spec/13.
- N20 extends to virtiofsd: each per-share daemon runs unprivileged, `--sandbox=namespace`, seccomp on, one process per share, with only that share writable. CVE-2026-47243 is the negative spec. (Cache mode was listed here originally; it is coherency, not confinement — see [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md).)
- QEMU is documentation-only: admissible under N2 but unhardened; Cloud Hypervisor is the one shipped hardened path.

## Consequences

- Good: N20 becomes a checkable by-construction property and the virtiofsd escape class is closed at the source.
- Good: the spec stays backend-agnostic while shipping one hardened path.
- Bad: the profile must be built and maintained; QEMU is unfit for untrusted workloads until hardened.

## Status

Accepted

Amended by ADR-0036 — the profile's "cgroup limits" are a per-VM systemd scope used for accounting, weighting, and teardown, with no limit on a VM or on the fleet slice.

Amended by ADR-0039 — filesystem-share cache mode is removed from this profile; it is a coherency setting, not a confinement mechanism, and the mode originally named is not a value the shipped daemon accepts. Every other element of the profile stands unchanged.

Amended by ADR-0048 — the wrapper this profile makes the enforcement layer is vivarium's own. The upstream runner package supervises the share daemons itself, unsandboxed, so consuming it would leave this profile unenforceable.

Amended by ADR-0049 — "capability drop toward zero" is scoped to the initial user namespace. The wrapper execs unprivileged with an empty bounding and ambient set there, while each filesystem daemon keeps the small working set it needs inside its own user namespace to honour guest-set ownership and modes; read literally against a daemon's own namespace, the original wording would ship a daemon that cannot serve a share. The capability that permits resolving a file by handle is never granted: a handle decoded on that privileged path resolves against the whole filesystem rather than the pivoted subtree, reopening the escape class this profile closes. Inode-file-handle mode is therefore deliberately forgone, and the wrapper instead raises the soft descriptor limit to the hard limit before `exec`.

Amended by ADR-0093 — the descriptor limit is passed to each daemon explicitly rather than inherited from the session's hard limit, so the budget does not vary by host. The forgoing of inode file handles above is unchanged; it is what makes the budget matter.

Amended by ADR-0099 — emptying the bounding set needs `CAP_SETPCAP`, which an unprivileged launcher does not hold, and `setpriv` refuses to exec the child rather than degrading. The drop is therefore rendered only where the capability is present. The rest of the wrapper, including the `no-new-privileges` bit this profile's unprivileged-execution clause depends on, is unchanged and unconditional.

Extends N20 ([`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md)) to host-side filesystem daemons and enacts ADR-0024 / ADR-0025's confinement standard. The concrete recipe and prerequisite probes live in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md); QEMU stays admissible under N2.
