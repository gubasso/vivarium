# 12 — Exec and shell

This page specifies how `viv exec` and `viv shell` connect to a project VM, how they start it if needed, and how they map streams and failures. The control and exit-status rationale is in [`../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md); the workspace path rationale is in [`../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md`](../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md); the shared stream and failure rules are in [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md); and startup reuses the ensure-running routine in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md).

## Command grammar

```text
viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <cmd> [args...]
viv shell
```

`-t` and `-T` are mutually exclusive. `--env KEY` copies that named variable from the host only when it exists; `--env KEY=VAL` supplies the literal value. `viv exec` with no `--`, or with `--` but no `<cmd>`, fails closed with EX_USAGE (64). `viv shell` is the command for "give me a shell"; empty `exec` is never a shell shortcut.

## Argv boundary

The first `--` is the vivarium/guest boundary. Everything after it is guest argv byte-for-byte: no vivarium flag parsing, no shell wrapping, no shell splitting, no glob expansion, and no environment expansion. A second `--` after the boundary is an ordinary guest argument.

```console
$ viv exec -- sh -lc 'echo "$PWD"'
```

That form is the explicit way to request shell semantics.

## Streams and terminal allocation

Guest stdout maps raw to host stdout, guest stderr maps raw to host stderr, and guest stdin remains attached to host stdin. Vivarium's own progress, status, warnings, and errors go to host stderr only, and progress appears only when stderr is a TTY, preserving pipelines and redirection.

`exec` defaults to no guest PTY. `-t`/`--tty` allocates a guest PTY and forwards terminal resize; vivarium refuses `-t` with EX_USAGE (64) when host stdin is not a terminal. `-T`/`--no-tty` forces no PTY. `shell` always allocates a PTY and returns the shell session's exit status when known.

## Guest process environment

`exec` runs the argv directly with `execve`; `shell` opens the configured user shell as a login-interactive shell. Both start in the exact directory from which `viv` was invoked after that directory has been proved to lie inside one declared workspace. Every workspace is host-symmetric (N16), so the correspondence is identity — the guest cwd is the host cwd, including its subdirectory, spelled the same way. No first declaration is privileged.

The default guest user is non-root `vivarium`; root is allowed only when an image or piece explicitly opts in. The workspace mount is writable for that user.

Host environment passthrough is deny-by-default. The default allowlist is `TERM`, `COLORTERM`, `NO_COLOR`, `FORCE_COLOR`, `LANG`, and `LC_*`. No other host variables are forwarded unless named by `--env`; vivarium does not auto-forward `SSH_AUTH_SOCK`, cloud tokens, or other credential-bearing variables.

Agent forwarding is not an exception to that rule, and naming `--env SSH_AUTH_SOCK` is not a substitute for it. The variable holds a host filesystem path; forwarding it hands the guest a name, while the socket it names is an object in the host kernel that a guest `connect()` cannot reach. When the agent channel is declared, the guest's `SSH_AUTH_SOCK` is set by the guest agent to a fixed guest path served by the relay below — a tool-generated value, not a forwarded host one. The channel itself, and the closed allowlist of what may cross it, are owned by [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md).

## Workspace mounts

Mount semantics are owned by [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md). Every declared workspace is mounted read-write at the absolute path it occupies on the host, and additional mounts may be declared for chosen guest paths. An extra mount's host source is a launch-time input and must never enter the Nix build output or a shared image or piece. A declared workspace's path is the exception and the reason is the symmetry above: it is the guest path as well as the host one, so the build depends on it ([`../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md`](../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md)). Neither may appear in a shared image or piece, which only a portable variable may reference.

## Ensure running and control socket

1. Take the per-target `flock` under `$XDG_RUNTIME_DIR/vivarium/<manifest>/<target>/lock`.
2. If `control.sock` exists, send the guest agent a cheap `Ping` over an authorized connection (below).
3. If `Ping` succeeds and `boot.json` matches the manifest sandbox key, running generation/store path, backend, and complete declared workspace set expected for this invocation, reuse the running VM and skip preflight/build/boot.
4. If the socket exists but ping fails, check `vm.pid` only as diagnostic/staleness evidence: dead process means remove stale runtime files; live process with unreachable agent means wait within the boot timeout or fail EX_UNAVAILABLE (69).
5. If no live VM is found, run the same hard preflight subset used by `viv start`, build or select the requested generation as needed, launch the VM, inject mounts, and wait for the guest agent readiness ping before releasing the lock.
6. After startup, concurrent `exec` and `shell` sessions do not hold the startup lock; they multiplex over the control socket.

```text
$XDG_RUNTIME_DIR/vivarium/<manifest>/<target>/
  lock
  vm.pid
  control.sock
  boot.json
  console.log
```

`console.log` is present whenever a VM is running, unless `--no-console-log` is set; it holds the guest's raw serial output under the capture contract in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md). `<manifest>` is the sandbox key and `<target>` the VM instance within it; the runtime layout mirrors the state layout in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md) component for component. `$XDG_RUNTIME_DIR` is required and never synthesized — how it resolves and what a missing or unusable one costs are owned by that page, and none of the files above outlive the session that root belongs to ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)). The control transport is vsock-class, host-local, and network-independent, bridged to a host Unix socket; the concrete device/backend is below this contract per N2. SSH is not the primary control plane, though it may exist as a debug fallback.

### One connection per session

The multiplexing in step 6 is the transport's, not vivarium's. `control.sock` is a listening socket: each `viv exec` and each `viv shell` opens its own connection to it and performs the transport's per-connection session handshake, and the transport carries the resulting streams independently. Session state — the PTY, the argv, the environment, the exit status — is per connection.

Three things therefore do not exist, and adding any of them would be a defect rather than an enhancement:

- No framing protocol layering many sessions over one stream.
- No socket created per session, and no port allocated per session.
- No session registry the host must keep in sync with the guest.

Sessions are counted, not tracked: `viv status` reports the number of live sessions ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)). The count is the agent's own — one integer that rises when a connection's `Start` is accepted and falls when that session ends — and it is read over the `Sessions` query pair in the tag table above. A connection becomes a session at its accepted `Start`, so a `Ping` connection and the query connection itself are never in the number, and nothing about it is a registry: no ids, no per-session state, nothing the host could fall out of sync with. A host asking an agent that predates the query treats the resulting protocol error as "unavailable", never as a fault. Several sessions attached to one VM is the ordinary case — one project, many terminals — and it is unrelated to `<target>`, which names VM instances, not sessions.

## Exit status and failures

Exit codes follow the program-wide taxonomy and per-command matrix in [`14-exit-codes.md`](./14-exit-codes.md); the `exec` / `shell` row there names the categories these commands can return. What is specific here is the guest-process-start boundary:

- Before the guest process starts, vivarium-origin failures use the sysexits categories: usage (`64`, incl. missing `--`, empty argv, `-t` when stdin is not a terminal), no/invalid manifest or incompatible generation metadata (`78`), a merged-configuration content defect on a cold start (`65`, see [`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)), Nix eval/build fault before boot (`70`), backend/agent unavailable or boot timeout (`69`), host or agent-reported permission failure (`77`), transient lock/startup race (`75`), and control-socket I/O (`74`).
- After it starts, return the guest status verbatim for `0..255`; a guest killed by signal `S` yields `128+S`. If the transport dies after guest start before the status is known, return `74` (EX_IOERR) with a stderr diagnostic and do not guess a guest code. A guest may itself exit a value such as `69`; that is still the guest's result, because the boundary is guest-process start.

`127` (not found) and `126` (not executable) stay reserved for a future refinement of the before-start not-found/not-executable cases; v1 uses the categories above.

## The wire protocol

Settled in [`../../decisions/ADR-0065-control-socket-wire-protocol.md`](../../decisions/ADR-0065-control-socket-wire-protocol.md). What follows is the contract for one connection; there is nothing above it, because there is no multiplexer.

### Establishing a connection

The vsock-class transport is a hybrid one: the backend listens on `control.sock`, and a host connection is completed to a guest port by the transport's own preamble before any vivarium byte is exchanged. Two properties fall out of that and are load-bearing:

- The host end is an ordinary Unix stream. Nothing on the host side needs a vsock-aware transport; the vsock dependency exists only in the guest agent.
- The guest cannot originate a control connection. A guest-initiated connection would need a host process listening on a per-port socket beside `control.sock`, and vivarium creates none — ever. The control plane is host-initiated by construction, not by policy.

The host consumes and validates the backend acknowledgement before reading a vivarium frame. The pinned backend's exact preamble and acknowledgement belong to [backend capabilities](../backend-capabilities.md#cloud-hypervisor); this contract requires the same complete transport establishment for any backend.

A connection that closes before the transport completes it means the agent is not yet listening: wait within the boot timeout, then `69` (step 4 above).

### The credential port

When a composition declares the agent channel ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md), [`../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md)), the transport carries a second guest port beside the control port. It is a separate port and a separate, deliberately minimal protocol — an opaque byte relay with no control tags — because sharing the control connection would demand the multiplexer this page says does not exist, and would put guest-reachable bytes on the same stream as `Exec` and `Signal` frames.

Both properties above survive unchanged, and the design is shaped around keeping them:

- Vivarium holds four idle connections per declared credential id that it opened on the credential port. A one-byte setup prelude selects `ssh` (`0x01`) or `gpg` (`0x02`), and the guest acknowledges admission to that id's bounded queue with `0x00`. The guest-side proxy accepts a local connection at the fixed guest socket path and consumes one parked connection; vivarium refills the pool. So the guest still originates nothing, and vivarium still creates no host listener beside `control.sock`.
- The host end of each parked connection remains an ordinary Unix stream, and vivarium relays its bytes to the host agent socket the channel names.

Vivarium never interprets a byte of that stream. The relay's payload is secret-class in full ([`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)); what may be recorded is that a channel exists and which id it carries, never its traffic.

### Framing

Each message begins with a four-byte unsigned big-endian length. The length counts the one-byte tag plus the payload. Standard-I/O payloads are raw bytes; every control payload is JSON encoded through serde. A standard-stream payload is at most 64 KiB and any other control payload is at most 1 MiB. A zero length, an oversized frame, truncated content, malformed JSON, an unknown tag, or a tag from the wrong direction closes the connection.

| Tag    | Sender | Message    |
| ------ | ------ | ---------- |
| `0x01` | client | `Hello`    |
| `0x02` | agent  | `Hello`    |
| `0x03` | client | `Ping`     |
| `0x04` | agent  | `Pong`     |
| `0x05` | client | `Start`    |
| `0x06` | client | `Stdin`    |
| `0x07` | client | `StdinEnd` |
| `0x08` | client | `Resize`   |
| `0x09` | client | `Signal`   |
| `0x0a` | agent  | `Stdout`   |
| `0x0b` | agent  | `Stderr`   |
| `0x0c` | agent  | `Exit`     |
| `0x0d` | agent  | `Error`    |
| `0x0e` | client | `Sessions` |
| `0x0f` | agent  | `Sessions` |
| `0x10` | client | `Shutdown` |
| `0x11` | agent  | `Shutdown` |
| `0x12` | client | `Memory`   |
| `0x13` | agent  | `Memory`   |
| `0x14` | client | `Trim`     |
| `0x15` | agent  | `Trim`     |

An unknown tag is a protocol error, not a message to skip — silently ignoring one would let two versions believe they agreed. Direction is part of the contract and is enforced, not merely documented: only the client sends standard input, resize, and signal frames; only the agent sends standard output, standard error, and the exit frame.

The message set is exactly what a session needs and no more: a handshake pair, `Ping`/`Pong`, a request to start the process, the three standard streams with an explicit end-of-input, `Resize`, `Signal`, `Exit`, `Error`, the session-count query pair the reporting below rests on, the shutdown request pair the stop ladder opens with ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)), and the two reclaim pairs `viv memory trim` and `viv volume trim` ride ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)).

A client first sends `Hello` with schema version 1 and the boot identity. After the matching agent `Hello`, the connection carries either `Ping`/`Pong`, one `Sessions` query and its answer, one `Shutdown` request and its acknowledgement, one `Memory` query and its answer, one `Trim` request and its acknowledgement, or a single session. `Resize` may precede `Start`. A pre-spawn failure produces `Error`; successful spawn produces streams followed by exactly one `Exit`. There is no `Started` frame, session id, multiplexer, or registry.

The memory query is a query like `Sessions`: the agent answers with the guest kernel's own `MemTotal` and `MemAvailable`, in bytes, and the connection ends on the answer. It exists because the trim target derivation needs the one figure the kernel publishes with exactly the right meaning — what can be reclaimed without swapping — and the host cannot derive it: the scope charge it reads includes the very page cache the trim exists to drop ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)).

The trim request carries the guest mountpoints to trim and is the shutdown request's disk counterpart with the opposite promise: the acknowledgement claims completion, not motion, because the host's next act is reading the image's allocated blocks. The agent hands the mountpoint list to a root-owned path unit through a trigger file in its own runtime directory, waits — bounded — for the done marker that unit writes only after `fstrim` finished, and only then acknowledges; a failure or a timed-out wait answers `Error` instead. Like the queries, the connection ends on the answer and can never become a session. A host asking an agent that predates either pair treats the resulting protocol error as "the agent cannot be reached" (`69`), never a fault.

The shutdown request is one request with one meaning: `viv stop` asks the agent to begin an orderly guest shutdown, which is the first rung of [`10-vm-lifecycle.md`](./10-vm-lifecycle.md)'s ladder — running anything else in the guest is what a session is. The agent makes the shutdown true before claiming it: it hands the request to the guest's own service manager through a root-owned trigger it can reach without privilege, then answers with the acknowledgement, so the acknowledgement promises motion, never completion — whether the shutdown finishes is observed from outside, as the VM exiting. Like a `Ping`, the request's connection ends on the answer and can never become a session. A host asking an agent that predates the request treats the resulting protocol error as "the agent cannot be reached" and falls through to the power signal, never a fault. The trigger is writable by the agent's own guest user, which every session process also runs as, so the widest thing the mechanism admits is a workload powering off its own sandbox — the same outcome that workload could already reach by crashing its guest, and one the host reads as a clean guest exit.

### Terminal size and signals

A resize carries the new dimensions and the agent applies them to the session's PTY; the host re-sends whenever its own terminal changes size. Sent before the process starts, it sets the initial dimensions instead — so a session never briefly renders at the wrong size.

With `-t` the host puts the local terminal in raw mode and forwards the interrupt as a byte, letting the guest PTY's line discipline raise the signal against the guest's own foreground process group. This is the only correct behaviour when the guest runs a job-control shell: a synthesized signal would go to the wrong process. Explicit signal frames exist for the non-TTY path, where there is no line discipline to do the work; the tag table does not qualify them by session kind, and a terminal session is still a process group, so they remain valid there too.

A signal frame carries the raw number, and the agent delivers any signal the guest kernel names — `1` through `31` on Linux — to the session's process group. The range a guest C library reserves for real-time signals is not a session's to send, so a number outside the named set closes the connection like the framing faults above.

A session whose client disconnects is not left running. The agent signals the session's process group to terminate and waits a bounded grace period; a group that has not exited by then is killed with the signal that cannot be caught, ignored, or held off by a stopped process. The wait is bounded rather than open-ended because the sandbox is disposable ([`../../decisions/ADR-0080-the-sandbox-is-disposable.md`](../../decisions/ADR-0080-the-sandbox-is-disposable.md)); an abandoned session must not outlive the client that owned it, and stopping a group therefore cannot strand one.

For a PTY, standard input, output, and error share the terminal as required by the operating system, and all PTY output is sent as `Stdout`. Without a PTY, stdout and stderr remain distinct.

End-of-input follows that same split. Without a PTY, `StdinEnd` closes the process's standard input and the guest reads EOF. Under a PTY there is no separate input channel to close: `StdinEnd` stops the host from writing, and what ends the guest's read is the terminal's own end-of-file character, carried in the client's ordinary byte stream. The agent does not synthesize that byte, for the same reason it does not synthesize an interrupt — inventing input is how a session ends up acting on the wrong process.

### Authorization

`control.sock` is not protected by a shared secret, and that is a decision rather than an omission. Three facts already settle who may talk to it:

- The runtime directory is session-scoped and `0700`, so no other host user can reach the socket at all ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`../../decisions/ADR-0055-runtime-directory-is-required.md`](../../decisions/ADR-0055-runtime-directory-is-required.md)).
- The guest cannot originate connections, per above.
- A secret would have to be delivered into the guest, where guest root — the adversary the sandbox is drawn against — reads it anyway. It would add a handling path and defend against nobody.

What is left to establish is which agent answered, and the handshake does exactly that: the agent's reply carries the boot identity, and vivarium compares it against `boot.json` before proceeding — the same comparison step 3 above already requires, against a stale socket, a re-created VM, or a crossed project. `boot.json` is host-written metadata and never authentication material.
