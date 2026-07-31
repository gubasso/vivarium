# 05 — Networking and egress

The sandbox's network model and its egress policy. The default and its rationale are in [`../../decisions/ADR-0007-default-open-egress.md`](../../decisions/ADR-0007-default-open-egress.md).

## Egress policy: open by default

Egress is controlled by one declarative knob, `sandbox.egress.mode`, with two values:

- **`open`** (default) — unrestricted outbound network. The guest can reach anything the host's network can reach. No filtering is applied.
- **`allowlist`** — default-deny outbound, permitting only named hosts. Enforcement is **host-side**, in the VM's own host-side network namespace (below), configured from the allowlist that layers contribute (the list concatenates across pieces, per [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). It is host-side by design: the guest is treated as adversarial and may hold guest-root, so a rule set _inside_ the guest could be torn down — the wall must sit outside it.

The default is `open` so that package installs, repository clones, and agent research work with no setup. Switching to `allowlist` is a single knob, typically supplied by a restriction piece.

## Why open egress is safe

The isolation boundary is the microVM, not the network (see [`../../decisions/ADR-0001-microvm-isolation-boundary.md`](../../decisions/ADR-0001-microvm-isolation-boundary.md)). Open egress does **not** weaken VM isolation: the guest still has only its own kernel and the devices, mounts, and network path it was given, and cannot escape to the host through the network. The cost of open egress is **data-exfiltration exposure** — a compromised process can send data outward. That is a workload policy choice, not a containment property. When egress is open, the controls that matter are credential and mount scoping (see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)), not the network.

## How the guest gets connectivity

Every VM's network lives in its **own host-side network namespace**, created by the same launch wrapper that enacts N20 ([`../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)). Inside that namespace the wrapper owns a tap interface for the guest and an unprivileged uplink process reaching the host's network; nothing about the setup requires host root, a host bridge, or any persistent host network configuration. This costs no new prerequisite: unprivileged user namespaces are already a **hard** doctor check, because the per-share filesystem daemon's sandbox cannot be built without them ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)).

Cloud Hypervisor offers **no user-mode networking** — its network device is a tap or a vhost-user backend — so a per-VM namespace is not one option among several on the shipped backend; it is the shape connectivity takes. The **wiring inside the namespace is below this contract** and may change: what is guaranteed is one unprivileged uplink process per VM and a guest that cannot observe or alter the arrangement, not a particular device topology.

The namespace matters for policy, not just for tidiness. It is what makes host-side enforcement true in the strong sense ADR-0044 intends: the rules live in a namespace the guest has no handle on, so a guest holding root can tear down everything it can see and change nothing about what it can reach.

## Address, DNS, and firewall

In `open` mode the guest obtains its address and DNS through its namespace's uplink and applies no outbound filter.

In `allowlist` mode the same connectivity is present, and the namespace additionally holds two things: a **default-deny packet filter** (nftables) and **vivarium's own resolver**, which is the only DNS server the guest is given and the only DNS destination the filter permits — a guest that queries any other resolver, in plaintext or over TLS or HTTPS, is rejected like any other denied destination.

The resolver is the enforcement point, and the **ordering is the contract**:

1. A query for a name that no allowlist entry matches is answered `REFUSED` immediately. It is never forwarded, so a denied name is not even leaked to the upstream resolver.
2. A query for a matching name is forwarded to the host's configured resolver. Every address in the answer is **installed into the filter's allowed set before the reply is written to the guest**, with the record's TTL as the element's lifetime.

Programming the filter _after_ releasing the reply would be a race — the guest can connect before the rule exists — and a failure of exactly the kind this page forbids elsewhere. It is specified as an ordering, not as an optimization.

Two consequences follow, and both are deliberate. Names are resolved **lazily, at the moment the guest asks**, never eagerly at boot, so an allowlisted host whose addresses rotate stays reachable and one that is temporarily unresolvable does not block a launch — an allowlisted name that fails to resolve is a health finding, owned by `egress-allowlist-dns` in [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md), not a boot failure. And an upstream failure is passed through as itself: `NXDOMAIN` for a name that does not exist, `SERVFAIL` for a resolver that could not answer. `REFUSED` therefore means _policy_, unambiguously, and never has to be distinguished from a genuine lookup failure.

**IPv4 and IPv6 are enforced identically.** Both families default-deny and both are programmed from the same answers. Permitting one family while dropping the other is specifically forbidden: a client that races A against AAAA stalls on the silent family, which is the unexplained hang this page exists to prevent. Where the host has no working IPv6 path, the resolver withholds AAAA records rather than letting the guest attempt a connection that cannot complete.

### What the allowlist matches

`egress.allow` is an array of strings ([`03-artifact-model.md`](./03-artifact-model.md)); each entry is one of:

| Form                            | Matches                                                                                        |
| ------------------------------- | ---------------------------------------------------------------------------------------------- |
| `example.com`                   | that name exactly                                                                              |
| `*.example.com`                 | exactly one additional label — `api.example.com`, but not `example.com` nor `a.b.example.com`  |
| `**.example.com`                | one or more additional labels — `api.example.com` and `a.b.example.com`, but not `example.com` |
| `192.0.2.10`, `2001:db8::1`     | that address, for destinations reached without DNS                                             |
| `192.0.2.0/24`, `2001:db8::/32` | any address in that block                                                                      |

An entry carries no scheme, no path, and no port. **The allowlist is a list of destinations, not of services**: every TCP port on an allowlisted host's addresses is permitted, and UDP is denied outright except DNS to vivarium's resolver. A port axis was considered and rejected — the exposure the policy exists to limit is _where_ data can go, not which port it leaves by, and a port dimension would break ordinary traffic (repository access over SSH, registries on non-standard ports) with exactly the unexplained failure ADR-0044 forbids.

The two wildcard forms are separate on purpose. A single `*` that silently spanned several labels would grant more than it reads as; a single `*` that could not span them at all would make a common case — a content-delivery subdomain several levels deep — inexpressible, and the resulting denial would be correct but baffling. Two tokens let the manifest say which it means.

**Denials are rejected, not dropped.** A blocked connection **must** fail the guest's `connect()` promptly with an error, and the DNS lookup of a denied name **must** fail the same way; neither may be silently discarded. A dropped packet turns a policy decision into an unexplained hang, which reads as a broken sandbox rather than an enforced rule ([`../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md`](../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md)). Concretely, in each of the three places a denial can happen:

- **A denied name** → DNS `REFUSED`, so the stub resolver fails immediately. Not `NXDOMAIN`, which asserts the name does not exist — a claim about the world rather than about policy, and one that gets cached as a negative answer.
- **A denied TCP connection** → a reset, so `connect()` returns `ECONNREFUSED` in milliseconds.
- **Anything else denied** → an ICMP administratively-prohibited response, one rule covering both address families.

The specific mechanism above is the shipped one; what a future mechanism may not do is fail slowly or silently.

No vivarium exit code describes a denial: enforcement is host-side but the failure is observed by a guest process, and after guest-process start `exec` returns that process's own status verbatim ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). What the contract guarantees is that the guest's attempt fails, and fails quickly. This is distinct from the `egress-allowlist-dns` health check ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)), which covers **allowlisted** names that fail to resolve.

## What a conforming implementation must be able to prove

Enforcement is only demonstrated by a trial in which the **allowlist is the only variable**. That requires a controlled endpoint the guest can actually reach, so the shape is specified here rather than left to whoever writes the test:

- Two names under the `.test` special-use domain — reserved for exactly this and guaranteed never to resolve publicly — resolving to **two different addresses**, both served by a listener the harness starts **inside the VM's own network namespace**. Two names sharing one address cannot distinguish a name-level decision from an address-level one, so the addresses must differ.
- Only the first name appears in `egress.allow`. Nothing else differs between the arms.
- The allowed arm must return the endpoint's bytes and exit `0`; the denied arm must fail non-zero, promptly, on `REFUSED`.

The endpoint lives in the namespace so the trial needs no external network and no public delegation. Until such a fixture exists, a trial using names that resolve on **neither** arm pins only the _shape_ of the contract — non-zero and fail-fast — and cannot falsify enforcement, because the allowed arm has nothing to reach either.
