# ADR-0066: Shares translate a fixed guest identity to the invoking host user

## Context and Problem Statement

Every other choice in the filesystem-sharing model is settled; the last one is ownership. Files the guest writes into the read-write workspace must land owned by the host user who ran `viv`, and files the host wrote must be writable by the guest user. Either the guest's numeric identity matches the host's, or something translates.

## Considered Options

- Match — build the guest user with the host's numeric UID/GID.
- Translate with subordinate-ID maps — the sandbox's own user-namespace mapping.
- Translate in the sharing daemon — its software ID-mapping options.

## Decision Outcome

Chosen option: translate in the sharing daemon — it is the only option that keeps the guest build host-independent while needing no host administration.

- The guest user has a fixed UID/GID, and each per-share daemon maps that pair bidirectionally to the invoking host user's. Guest identities outside the map are forbidden, so a guest `chown` to an unmapped ID fails cleanly rather than silently landing as the host user.
- Match is rejected on purity grounds. A guest user's UID is guest system configuration and therefore build-channel; equating it to the host's would put a launch-time value in the build output, break N19, and make cached guest builds host-specific.
- Subordinate-ID maps are rejected on posture grounds. They require host-administered `subuid`/`subgid` ranges and setuid helpers — host state vivarium would have to add as a doctor check and a setup step, against its no-root, no-daemon posture.

## Consequences

- Good: the last open choice in the sharing model closes on paper; the guest image stays host-independent.
- Good: no host administration, no new prerequisite, and it composes with the namespace sandbox N20 mandates.
- Bad: shares cannot support POSIX ACLs — translation and ACLs are mutually exclusive upstream — so the list of things a share is not gains a third entry.
- Bad: it pins a minimum daemon version, which becomes an assertion the closure must satisfy.

## Status

Accepted

Discharges the consequence [`ADR-0025-default-hypervisor-cloud-hypervisor.md`](./ADR-0025-default-hypervisor-cloud-hypervisor.md) left open — "virtio-fs fidelity for the RW workspace (UID/GID mapping)" — which is otherwise unchanged.

Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), with the launch-profile flags in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).

The translation options are absent from the daemon's 1.11 and 1.12 documentation and present in 1.13, so the floor is 1.13.0. Per [`ADR-0049-backend-is-a-closure-member.md`](./ADR-0049-backend-is-a-closure-member.md) the backend is a pinned closure member, so this lands as an evaluation-time assertion on the closure's version rather than a number in the spec — the same treatment given to balloon free-page reporting and block discard.
