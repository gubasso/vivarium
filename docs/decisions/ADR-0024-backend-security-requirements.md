# ADR-0024: Backend security requirements and VMM sandbox

## Context and Problem Statement

vivarium exists to run untrusted coding agents. The isolation contract is a capability class, not a named VMM (N1/N2, ADR-0001), but the class was under-specified: any KVM backend qualified, however large its device surface or however exposed its VMM process. The properties that make the boundary trustworthy must be normative.

## Considered Options

- Leave the class at "hardware virtualization + separate guest kernel" only.
- Name one hypervisor in the contract.
- Specify a security property class; keep the concrete default hypervisor an implementation detail.

## Decision Outcome

Chosen option: **specify the security property class**.

Threat model: guest code is assumed adversarial — arbitrary execution, kernel-exploit attempts, data exfiltration. A shared host kernel is a single-bug boundary; the microVM boundary forces an attacker to chain a guest-kernel escape _and_ a VMM/hardware break. To keep that second wall real, every admissible backend must provide:

- the hardware-virtualization boundary with its own guest kernel (N1);
- a minimal, virtio-only device model — no legacy device emulation;
- a shared-filesystem device sufficient for the read-write workspace mount (N16);
- a VMM process that vivarium always launches under a host-side sandbox — seccomp syscall filter plus capability drop — so even a compromised VMM is contained: new invariant **N20**;
- no host-filesystem access beyond the declared mounts (ADR-0020).

Guest-kernel separation and the VMM sandbox are guarantees by construction, not runtime probes; `viv doctor` checks only the host-visible prerequisites (spec/13).

The **default hypervisor is deliberately deferred.** The microvm.nix-supported candidates that satisfy the class (for example QEMU's microvm machine type or Cloud Hypervisor) are admissible; picking the shipped default is an implementation choice under N2, recorded in a follow-up decision.

## Consequences

- Good: the isolation bar becomes checkable properties, not a brand name.
- Good: the spec stays backend-agnostic (N2) while excluding weak backends.
- Bad: the VMM sandbox must be built and maintained for every supported backend.

## Status

Accepted

Adds invariant **N20** to [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md). The default-hypervisor selection deferred here is recorded in [`ADR-0025-default-hypervisor-cloud-hypervisor.md`](./ADR-0025-default-hypervisor-cloud-hypervisor.md).

Amended by [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — N20 is extended to cover host-side filesystem daemons (virtiofsd), and its confinement standard is enacted as a concrete by-construction launch profile.
