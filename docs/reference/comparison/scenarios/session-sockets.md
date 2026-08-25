# Session sockets

Record what happens when a mount names one of the host's session directories — `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` — as its source. These hold live session state: the session bus, the display socket, the authentication-agent socket.[^read]

1. Name `/tmp`, `${XDG_RUNTIME_DIR}`, or an ancestor of either as a mount source.
2. Start the tool.
3. Record the exit status, the message, and whether anything booted.

## vivarium

Yes: the [session-directories-never-cross rule](../../spec/08-invariants-and-guarantees.md) refuses a mount whose `source` resolves to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`, or any ancestor, in either layer, before boot. A share conveys an inode, not a listener, so mounting a socket directory grants the exposure without the capability that motivated it.

## flake-pilot

At the firecracker route, n/a: there is no bind-mount mechanism, so there is nothing to refuse.

At the `krun` route there is something to refuse and nothing refuses it: a session directory is an ordinary `--opt "\-v /tmp:/tmp"` line in the registration, and the only inspection `podman-pilot` performs on a `--volume` argument is the existence check behind `%ignore_missing_volume_path`. What the boundary does instead of refusing is make the mount useless in the usual case: the guest runs its own kernel, so a carried socket path arrives as a name with no listener behind it — a failure at use rather than a refusal at start, and not the same thing as declining to carry it. libkrun states the limit on its own passthrough in the same terms, warning that it "does not provide any protection against the guest attempting to access other directories in the same filesystem, or even other filesystems in the host", and directing users to arrange isolation host-side. A directory that is not a socket — `/var/tmp`, or an ancestor of a session directory — crosses and is readable.

## glaipnir

No: the mount list is fixed in the script and names no session directory, but the workspace is the invocation's own directory and crosses without inspection, so starting a run from inside `/tmp` mounts it. The one source check that exists refuses a workspace equal to `$HOME` and falls back; nothing looks at `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`.

## bunkerbox

Not applicable: there is no mount source to name, for the reason [the mount-choice row](./choosing-mounts.md#bunkerbox) records — the set is fixed and no configuration key adds to it. The same `➖` the firecracker route carries, and for the same reason: a refusal needs something to refuse.

[^read]: Read at `vivarium` `ceb0027` and `flake-pilot` `main` on 2026-08-18; `glaipnir` `21ef389` on 2026-08-20; `bunkerbox` `b7f14f3` on 2026-08-25.
