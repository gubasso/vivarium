# 05 — Networking and egress

The sandbox's network model and its egress policy. The default and its rationale are in [`../../decisions/ADR-0007-default-open-egress.md`](../../decisions/ADR-0007-default-open-egress.md).

## Egress policy: open by default

Egress is controlled by one declarative knob, `sandbox.egress.mode`, with two values:

- **`open`** (default) — unrestricted outbound network. The guest can reach anything the host's network can reach. No filtering is applied.
- **`allowlist`** — default-deny outbound, permitting only named hosts. Enforcement is **host-side**, on the host end of the guest's network path (for example a NAT/tap firewall), configured from the allowlist that layers contribute (the list concatenates across pieces, per [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). It is host-side by design: the guest is treated as adversarial and may hold guest-root, so a rule set _inside_ the guest could be torn down — the wall must sit outside it.

The default is `open` so that package installs, repository clones, and agent research work with no setup. Switching to `allowlist` is a single knob, typically supplied by a restriction piece.

## Why open egress is safe

The isolation boundary is the microVM, not the network (see [`../../decisions/ADR-0001-microvm-isolation-boundary.md`](../../decisions/ADR-0001-microvm-isolation-boundary.md)). Open egress does **not** weaken VM isolation: the guest still has only its own kernel and the devices, mounts, and network path it was given, and cannot escape to the host through the network. The cost of open egress is **data-exfiltration exposure** — a compromised process can send data outward. That is a workload policy choice, not a containment property. When egress is open, the controls that matter are credential and mount scoping (see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)), not the network.

## How the guest gets connectivity

The network is provided by the virtualization backend. Two backend-level approaches exist:

- **User-mode networking** — the hypervisor provides outbound connectivity with no host network setup (no bridge, no NAT). This is the simplest path to "the guest just has internet" and the default expectation for the common single-VM workflow.
- **Host-side NAT with a tap interface** — a routed setup used with backends that do not offer user-mode networking, or when stable guest addressing and higher throughput are needed.

The choice of backend and networking mode is an implementation concern below the egress-policy knob: `sandbox.egress.mode` governs _policy_ regardless of which connectivity mechanism a backend uses.

## Address, DNS, and firewall

In `open` mode the guest obtains its address and DNS from the backend's networking and applies no outbound firewall. In `allowlist` mode the same connectivity is present, but a **host-side** firewall on the guest's network path blocks outbound traffic except to permitted hosts and the DNS needed to resolve them.

**Denials are rejected, not dropped.** A blocked connection **must** fail the guest's `connect()` promptly with an error, and the DNS lookup of a denied name **must** fail the same way; neither may be silently discarded. A dropped packet turns a policy decision into an unexplained hang, which reads as a broken sandbox rather than an enforced rule. The enforcement **mechanism** is left to implementation — any of the approaches above can satisfy this — but the fail-fast surface is not optional ([`../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md`](../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md)).

No vivarium exit code describes a denial: enforcement is host-side but the failure is observed by a guest process, and after guest-process start `exec` returns that process's own status verbatim ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). What the contract guarantees is that the guest's attempt fails, and fails quickly. This is distinct from the `egress-allowlist-dns` health check ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)), which covers **allowlisted** names that fail to resolve.
