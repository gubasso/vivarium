# Backend capabilities

This page owns exact upstream capability facts that are pinned or publicly attributable, are not already owned by a specification, ADR, or the verification harness, and directly shape a planned slice.

The product contract remains in the [specification](./spec/README.md), rationale remains in [ADRs](../decisions/), host observations remain in the [verification harness](./microvm-verification-harness.md), and unresolved choices remain in [open questions](../plan/open-questions.md). Revalidate this page through [tracking](./tracking.yaml).

## Pinned inputs

`nix/flake.lock` is the local authority for the product input set. As checked on 2026-08-10 it pins:

| input     | locked revision                            | public source                                                                                                                  | slices that depend on it |
| --------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------ |
| `nixpkgs` | `f13ff45afd1bb73e640eaa08a7066dbed07e3238` | [NixOS/nixpkgs at the locked revision](https://github.com/NixOS/nixpkgs/tree/f13ff45afd1bb73e640eaa08a7066dbed07e3238)         | 001, 002, 007, 009       |
| `microvm` | `06544a68599a0e0721760af442000204e57e3937` | [microvm.nix at the locked revision](https://github.com/microvm-nix/microvm.nix/tree/06544a68599a0e0721760af442000204e57e3937) | 001, 002, 004, 007       |

Package versions are derived from the locked `nixpkgs` closure rather than duplicated as another pin here. The harness records versions observed in real-host runs.

## Cloud Hypervisor

At Cloud Hypervisor v53.0, `--net` accepts tap, macvtap, vhost-user, or file-descriptor-backed networking; it does not provide the user-mode mode that microvm.nix exposes for QEMU and kvmtool. This fact keeps [slice 004](../plan/slices/004-enforce-egress-allowlist/README.md) from treating an unavailable mode as a fallback. See the [v53.0 command definition](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/cloud-hypervisor/src/main.rs) and the locked [microvm.nix interface options](https://github.com/microvm-nix/microvm.nix/blob/06544a68599a0e0721760af442000204e57e3937/nixos-modules/microvm/options.nix).

At the same version the backend caps two concurrent-connection surfaces, and neither cap is stated by any specification page. The vsock muxer accepts `MAX_CONNECTIONS = 1023`; past that cap it accepts a connection and immediately drops it, so the failure presents as a connection that opens and dies rather than one that is refused. The API socket caps concurrent connections at ten, and the eleventh receives HTTP 503 with `{ "error": "Too many open connections" }`. That cap is a connection cap rather than a request cap, and it belongs to the embedded [micro-http server](https://github.com/firecracker-microvm/micro-http/blob/main/src/server.rs) (`MAX_CONNECTIONS = 10`, returning `ServerError::ServerFull`) rather than to Cloud Hypervisor's own API documentation, which does not state it. Re-checked on 2026-08-10, when the earlier attribution to `docs/api.md` was found not to carry the fact. The first bounds [slice 003](../plan/slices/003-guest-agent-and-credential-relay/README.md), whose control transport must survive the several concurrent sessions [exec and shell](./spec/12-exec-and-shell.md) calls the ordinary case without naming an upper bound; the second bounds any supervisor in [slice 002](../plan/slices/002-secure-launch-and-supervision/README.md) that drives the API socket concurrently. Source at v53.0 for the vsock cap: [`virtio-devices/src/vsock/unix/mod.rs`](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/virtio-devices/src/vsock/unix/mod.rs).

The same v53.0 hybrid-vsock host endpoint is an ordinary Unix stream. A host-opened connection writes `CONNECT <guest-port>\n`; the VMM replies `OK
<ephemeral-host-port>\n` when the guest connection is established. The host consumes that bounded acknowledgement before exchanging application bytes, and the numeric acknowledgement port is not the requested guest port. With four parked connections for each of at most two credential ids, the idle pool uses at most eight connections under the 1,023-connection cap. Sources: [v53.0 vsock documentation](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/docs/vsock.md) and [v53.0 Unix muxer](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/virtio-devices/src/vsock/unix/muxer.rs). The `CONNECT` and `OK` shape is re-proved on every run of `tests/host/guest-agent-check`, which cannot boot a guest without it.

From v53.0 the same endpoint propagates a half-close: a guest `VSOCK_OP_SHUTDOWN` carrying `VSOCK_FLAGS_SHUTDOWN_SEND` reaches the backing host Unix socket as `shutdown(SHUT_WR)`, so the host peer reads end of file. Any host design that waits for a guest to finish speaking depends on this, and it was absent through v52.0 — see [KI-0002](./known-issues/resolved/KI-0002.md), which is why the pin is at or above v53.0 rather than merely current. Source: [v53.0 `csm/connection.rs`](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/virtio-devices/src/vsock/csm/connection.rs), whose `shutdown_host_write_side` is the call that was missing.

The v53.0 API-only sequence is `cloud-hypervisor --api-socket path=<path> --seccomp true`, followed by `ch-remote --api-socket <path> create <VmConfig JSON>` and a separate `ch-remote --api-socket <path> boot`. `vm.create` stores configuration and creates the console devices; vCPUs begin only in `vm.boot`. The JSON uses the pinned Rust `VmConfig` field spellings, including `boot_vcpus`, `landlock_enable`, and serial `{ "mode": "Socket", "socket": ... }`. This is the causal boundary used by the slice 002 supervisor. Sources: [v53.0 command entry point](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/cloud-hypervisor/src/main.rs), [v53.0 VM configuration](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/vmm/src/vm_config.rs), and [v53.0 request handler](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/v53.0/vmm/src/lib.rs).

A user-space vhost-user uplink can drive this backend directly — the pinned passt negotiated with the pinned VMM and the guest booted past device setup — and the shipped topology deliberately does not use it, because that path crosses no netfilter hook for the allowlist to attach to; the [slice 004 revisions](../plan/slices/004-enforce-egress-allowlist/README.md) carry Q-006's experiment record. Serial no-replay behavior, accepted Landlock spelling, shared-memory requirements, page reporting, and sparse/discard observations already have owners in the [logging specification](./spec/16-logging-and-diagnostics.md), [resource specification](./spec/17-resources-and-capacity.md), and [verification harness](./microvm-verification-harness.md), so they are not copied here.

## microvm.nix module

At the locked revision, `microvm.shares[*].proto` accepts `9p` or `virtiofs` and defaults to `9p`. It is the one default on this options page whose upstream value is unusable on the shipped backend, so every share vivarium declares sets it explicitly; `nix/guest.nix` does so at each share. `microvm.shares[*].cache` accepts `auto`, `always`, `metadata`, or `never` and defaults to `auto`. `microvm.virtiofsd.threadPoolSize` accepts a string or unsigned integer and defaults to `` `nproc` ``; the module always emits `--thread-pool-size`. These upstream defaults are not vivarium's policy. They are the baseline [slice 001](../plan/slices/001-close-host-measurement-gaps/README.md) and the launch constructor in [slice 002](../plan/slices/002-secure-launch-and-supervision/README.md) must override deliberately. Sources: [locked module options](https://github.com/microvm-nix/microvm.nix/blob/06544a68599a0e0721760af442000204e57e3937/nixos-modules/microvm/options.nix) and [locked virtiofsd module](https://github.com/microvm-nix/microvm.nix/blob/06544a68599a0e0721760af442000204e57e3937/nixos-modules/microvm/virtiofsd/default.nix).

The locked module passes `--rlimit-nofile 1048576` only when its wrapper runs as root and otherwise omits the option. vivarium runs the daemon unprivileged, so [ADR-0093](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) and slice 002 require an explicit limit rather than inherited behavior.

## virtiofsd

The pinned closure used by the latest host evidence contains virtiofsd 1.14.0; the evidence owner is the [verification harness](./microvm-verification-harness.md). At upstream tag v1.14.0:

- `--sandbox` accepts `namespace`, `chroot`, or `none` and defaults to `namespace`.
- `--seccomp` accepts `none`, `kill`, `log`, or `trap` and defaults to `kill`.
- `--cache` accepts `auto`, `always`, `metadata`, or `never` and defaults to `auto`; `cache=none` is not accepted by the Rust daemon.
- `--thread-pool-size` defaults to `0`, which disables the pool.
- `--inode-file-handles` defaults to `prefer`.
- `--rlimit-nofile` declares the maximum descriptor count; when omitted the daemon derives a limit from the process and kernel ceiling.
- UID/GID translation flags are present and cannot be combined with POSIX ACLs.

These shapes are used directly by slices 001 and 002. The source is the [v1.14.0 README](https://gitlab.com/virtio-fs/virtiofsd/-/blob/v1.14.0/README.md) and [v1.14.0 command-line definition](https://gitlab.com/virtio-fs/virtiofsd/-/blob/v1.14.0/src/main.rs). The accepted vivarium policies belong to [ADR-0050](../decisions/ADR-0050-share-cache-policy-named-by-mechanism.md), [ADR-0096](../decisions/ADR-0096-the-share-worker-pool-takes-the-daemon-default.md), [ADR-0066](../decisions/ADR-0066-share-uid-gid-translation.md), and [ADR-0093](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md).

## Guest kernel and NixOS

The guest kernel, NixOS modules, and Nix package manager are closure members selected by the locked `nixpkgs` revision above. Their evaluated versions are not independent constants and MUST be re-read from the closure after a pin move. The [verification harness](./microvm-verification-harness.md) owns the versions and behavior observed on real hosts; the [resource specification](./spec/17-resources-and-capacity.md) owns required guest features. This page adds no second version roster.

## Nix store and collector interfaces

The `local-overlay` store and collector facts retained from the draft are already fully owned by the [workspace specification](./spec/06-workspace-and-project-environment.md), [resource specification](./spec/17-resources-and-capacity.md), storage ADRs [ADR-0087](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) through [ADR-0092](../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md), and the [verification harness](./microvm-verification-harness.md). They are intentionally not repeated.

The collector behavior that was unresolved here is measured and is a defect in the pinned Nix, tracked as [KI-0001](./known-issues/KI-0001/README.md). A pin move MUST re-read it: the case closes when a collection attempts more than one path.
