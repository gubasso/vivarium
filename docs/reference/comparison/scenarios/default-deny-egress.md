# Default-deny egress

Record what the tool can reach on the network with no destination named. The row asks whether a default-deny posture is reachable at all; whether a destination can then be readmitted by name is [the next row](./allowlist.md), and is not re-asked here.

1. Configure the tool for its most restrictive documented network posture.
2. Inside, attempt a TCP connection to an arbitrary public address.
3. Record the result and how long it took to answer.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes, and open is the default: the [egress-defaults-open rule](../../spec/08-invariants-and-guarantees.md) makes unrestricted egress the shipped posture and requires exactly one declarative knob to switch to a default-deny allowlist, which is `sandbox.egress.mode`. Open egress is stated not to weaken the boundary — the boundary is the microVM, and the cost of open egress is exfiltration exposure, a workload policy choice rather than a containment property.

The deny posture is enforced host-side, in the VM's own network namespace, because a guest holding root could tear down any ruleset it can see; denials are rejected rather than dropped, so a blocked attempt fails in milliseconds instead of hanging. Both modes run today: a denied name answered `REFUSED` in 9 ms and a denied literal connect reset in 15 ms, measured 2026-08-14 and recorded in [`implementation-status.md`](../../implementation-status.md). Details in [`spec/05-networking-and-egress.md`](../../spec/05-networking-and-egress.md).

## flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Yes, by absence rather than by policy, and the absence is the shipped state. Upstream states that firecracker "supports networking only through TUN/TAP devices" and that "it is the user's responsibility to set up the routing on the host from the TUN/TAP device to the outside world", then walks a static-IP NAT setup: `ip_forward`, a MASQUERADE rule, a `tap-<app>` device per registration, and `boot_args` edited from `ip=dhcp` to a static triple. Until an operator does that work a microVM reaches nothing, and `flake-ctl firecracker register --no-net` keeps it that way deliberately, which is the documented restrictive posture this row asks for.

## podman

podman 5.x, read 2026-08-19. Yes: `--network none` is genuinely default-deny, and the network posture is podman's rather than the OCI runtime's, so it applies at the krun setup too.
