# ADR-0083: Inner-layer store registration is one closure, on demand, on the launch channel

## Context and Problem Statement

[`ADR-0038`](./ADR-0038-guest-store-sharing.md) shares the host store read-only, but a real-host measurement showed only the guest's own boot closure is registered in its Nix database. Every other shared path is readable but formally unknown, so the inner layer cannot reuse bytes it can see. ADR-0038 left the fix as a fork: which channel registers, over what scope.

## Considered Options

- **Build channel** — register the project's dev environment into the image closure.
- **Launch channel, whole host database** — dump it on the host, load it in the guest.
- **Launch channel, one closure on demand** — validity is materialized for a shared path when something asks.

## Decision Outcome

Chosen option: **launch channel, one closure on demand** — the only option agnostic about how the project builds itself. **This settles policy; the mechanism is deferred to a host spike.**

- The build channel breaks **N3/N5/N19** — computing that closure means evaluating the working tree, so the image stops being a function of the manifest — and [`ADR-0008`](./ADR-0008-two-layer-separation.md), having a root path only when the inner layer is Nix-shaped. Nothing here evaluates the project, so any other tool leaves it inert.
- The whole database is refused: stale at once, and it couples guest correctness, GC, and trust to the host's.
- **Registration is paired with retention.** `--load-db` marks paths valid without checking they exist, so registration alone yields a silently corrupt guest database.
- **The presumptive mechanism is upstream, not bespoke.** Nix's experimental `local-overlay-store` implements these semantics, and the guest's overlay mount satisfies its `check-mount` shape. It needs a lower _metadata_ source: an immutable per-boot snapshot of the host store database, shared read-only, with no guest-originated request. Fallbacks: an allowlisted store-protocol proxy over [`ADR-0071`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md)'s parked connections; then filtered `--dump-db`.

## Consequences

- Good: the inner layer reuses host bytes with no egress, and forces no image rebuild.
- Good: vivarium never reads the project's build system.
- Good: the snapshot form needs no guest-originated request, leaving [`ADR-0065`](./ADR-0065-control-socket-wire-protocol.md)'s third authorization leg untouched.
- Bad: the mechanism stays open, so `spec/06` still states the measured limit.

## Status

Accepted — **as policy. The mechanism is deliberately open.**

Amended 2026-08-01, before implementation, by a greenfield review. The decision outcome survived intact; the original mechanism — a bespoke guest-originated `--dump-db` request — did not, because Nix's experimental `local-overlay-store` already implements the same semantics upstream and vivarium's guest is already in the filesystem layout it requires. Its experimental status ([NixOS/nix milestone 50](https://github.com/NixOS/nix/milestone/50) is open) is the reason it is presumptive rather than chosen.

Three competing spikes decide it, and none can be settled on paper: `local-overlay-store` against an immutable metadata snapshot; a restricted store-protocol proxy; filtered registration. Two facts the spike must carry: `nix-store --dump-db` **does** accept store paths and subset the dump, but the caller owns closure completeness — a partial dump fails `registerValidPaths`. And whichever mechanism wins, the host must not garbage-collect under a running guest; that constraint predates this decision and belongs to [`ADR-0038`](./ADR-0038-guest-store-sharing.md).

Fallback (b) is not a protocol to invent. `nix-store --serve` is upstream's documented primitive for exactly this shape — a store-protocol server speaking over stdin and stdout, meant for a restricted peer, read-only unless `--write` is passed ([Nix manual](https://nix.dev/manual/nix/2.31/command-ref/nix-store/serve.html)). The spike models that arm on it rather than on a bespoke allowlist, so what gets confined under N20 is a known program with a known surface.

Discharges the fork left open in [`ADR-0038`](./ADR-0038-guest-store-sharing.md).
