# 12 — Exec and shell

This page specifies how `viv exec` and `viv shell` connect to a project VM, how they start it if
needed, and how they map streams and failures. The control and exit-status rationale is in
[`../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md);
the workspace path rationale is in
[`../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md`](../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md);
the shared stream and failure rules are in
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md);
and startup reuses the ensure-running routine in [`10-vm-lifecycle.md`](10-vm-lifecycle.md).

## Command grammar

```text
viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <cmd> [args...]
viv shell
```

`-t` and `-T` are mutually exclusive. `--env KEY` copies that named variable from the host only when
it exists; `--env KEY=VAL` supplies the literal value. `viv exec` with no `--`, or with `--` but no
`<cmd>`, fails closed with EX_USAGE (64). `viv shell` is the command for "give me a shell"; empty
`exec` is never a shell shortcut.

## Argv boundary

The first `--` is the vivarium/guest boundary. Everything after it is guest argv byte-for-byte: no
vivarium flag parsing, no shell wrapping, no shell splitting, no glob expansion, and no environment
expansion. A second `--` after the boundary is an ordinary guest argument.

```console
$ viv exec -- sh -lc 'echo "$PWD"'
```

That form is the explicit way to request shell semantics.

## Streams and terminal allocation

Guest stdout maps raw to host stdout, guest stderr maps raw to host stderr, and guest stdin remains
attached to host stdin. Vivarium's own progress, status, warnings, and errors go to host stderr only,
and progress appears only when stderr is a TTY, preserving pipelines and redirection.

`exec` defaults to no guest PTY. `-t`/`--tty` allocates a guest PTY and forwards terminal resize;
vivarium refuses `-t` with EX_USAGE (64) when host stdin is not a terminal. `-T`/`--no-tty` forces no
PTY. `shell` always allocates a PTY and returns the shell session's exit status when known.

## Guest process environment

`exec` runs the argv directly with `execve`; `shell` opens the configured user shell as a
login-interactive shell. Both start in the workspace cwd: the primary workspace mount
`/workspaces/<repo>`, or the corresponding guest subdirectory when invoked from a subdirectory of the
host workspace.

The default guest user is non-root `vivarium`; root is allowed only when an image or piece explicitly
opts in. The workspace mount is writable for that user.

Host environment passthrough is deny-by-default. The default allowlist is `TERM`, `COLORTERM`,
`NO_COLOR`, `FORCE_COLOR`, `LANG`, and `LC_*`. No other host variables are forwarded unless named by
`--env`; vivarium does not auto-forward `SSH_AUTH_SOCK`, cloud tokens, or other credential-bearing
variables. Credential and agent forwarding is deferred to a later runtime-injection design; see
[`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).

## Workspace mounts

Mount semantics are owned by [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md).
The primary workspace is mounted read-write at `/workspaces/<repo>`, and additional mounts may be
declared for other guest paths. All host paths for primary and extra mounts are launch-time inputs or
personal/machine-local config and must never enter the Nix build output or shared manifest facts.

## Ensure running and control socket

1. Take the per-project `flock` under `$XDG_RUNTIME_DIR/vivarium/<project-id>/lock`.
2. If `control.sock` exists, send the guest agent a cheap authenticated `Ping`.
3. If `Ping` succeeds and `boot.json` matches the project identity, running generation/store path,
   backend, and workspace host path expected for this invocation, reuse the running VM and skip
   preflight/build/boot.
4. If the socket exists but ping fails, check `vm.pid` only as diagnostic/staleness evidence: dead
   process means remove stale runtime files; live process with unreachable agent means wait within
   the boot timeout or fail EX_UNAVAILABLE (69).
5. If no live VM is found, run the same hard preflight subset used by `viv start`, build or select the
   requested generation as needed, launch the VM, inject mounts, and wait for the guest agent
   readiness ping before releasing the lock.
6. After startup, concurrent `exec` and `shell` sessions do not hold the startup lock; they multiplex
   over the control socket.

```text
$XDG_RUNTIME_DIR/vivarium/<project-id>/
  lock
  vm.pid
  control.sock
  boot.json
  console.log
```

`console.log` is optional. `<project-id>` is the still-open project-identity key; this page does not
define its exact form. The control transport is vsock-class, host-local, and network-independent,
bridged to a host Unix socket; the concrete device/backend is below this contract per N2. SSH is not
the primary control plane, though it may exist as a debug fallback.

## Exit status and failures

Exit codes follow the program-wide taxonomy and per-command matrix in
[`14-exit-codes.md`](14-exit-codes.md); the `exec` / `shell` row there names the categories these
commands can return. What is specific here is the **guest-process-start boundary**:

- **Before** the guest process starts, vivarium-origin failures use the sysexits categories: usage
  (`64`, incl. missing `--`, empty argv, `-t` when stdin is not a terminal), no/invalid manifest or
  incompatible generation metadata (`78`), Nix eval/build fault before boot (`70`), backend/agent
  unavailable or boot timeout (`69`), host or agent-reported permission failure (`77`), transient
  lock/startup race (`75`), and control-socket I/O (`74`).
- **After** it starts, return the guest status verbatim for `0..255`; a guest killed by signal `S`
  yields `128+S`. If the transport dies after guest start before the status is known, return `74`
  (EX_IOERR) with a stderr diagnostic and do not guess a guest code. A guest may itself exit a value
  such as `69`; that is still the guest's result, because the boundary is guest-process start.

`127` (not found) and `126` (not executable) stay reserved for a future refinement of the
before-start not-found/not-executable cases; v1 uses the categories above.

## Deferred details

- Exact `<project-id>` definition.
- Exact control-socket wire framing/auth/multiplex protocol.
- Credential/agent forwarding.
- Implementation backend/device.
