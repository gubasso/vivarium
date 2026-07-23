# ADR-0001: Isolate workspaces behind a microVM hardware-virtualization boundary

## Context and Problem Statement

vivarium runs untrusted or semi-trusted code — including autonomous AI coding agents — against a
user's projects. The isolation boundary must resist a hostile process inside the sandbox escaping to
the host. Shared-kernel isolation (namespaces, seccomp) exposes the entire host kernel as attack
surface; a single kernel bug breaks containment.

## Considered Options

- **Shared-kernel containers** — namespaces + cgroups + seccomp on the host kernel.
- **Language/OS sandboxing** — per-process seccomp/AppArmor confinement without a VM.
- **microVM with a separate guest kernel** — hardware virtualization (VT-x/AMD-V), the guest runs its
  own kernel; the host is reached only through a narrow, well-audited hypervisor interface.

## Decision Outcome

Chosen option: **microVM with a separate guest kernel** — a hardware-virtualization boundary gives a
far smaller, better-hardened host attack surface than a shared kernel, which is the level of
containment agent workloads require.

The boundary is specified by **capability class** ("separate guest kernel behind hardware
virtualization"), not by a hardcoded virtual machine monitor. Any backend that satisfies the class
(for example QEMU, Cloud Hypervisor, or Firecracker) is admissible, so a specific VMM is an
implementation choice, not part of the contract. See
[`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md)
for the boundary invariant.

## Consequences

- Good: strong isolation; a guest kernel bug does not by itself reach the host.
- Good: the backend can change without changing the product contract.
- Bad: requires hardware virtualization on the host (KVM or equivalent); no shared-kernel fallback.
- Bad: heavier startup and memory cost than a container, and a weaker Windows story.

## Status

Accepted
