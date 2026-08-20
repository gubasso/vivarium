# Session sockets

Record what happens when a mount names one of the host's session directories — `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` — as its source. These hold live session state: the session bus, the display socket, the authentication-agent socket.[^read]

1. Name `/tmp`, `${XDG_RUNTIME_DIR}`, or an ancestor of either as a mount source.
2. Start the tool.
3. Record the exit status, the message, and whether anything booted.

## vivarium

Yes: the [session-directories-never-cross rule](../../spec/08-invariants-and-guarantees.md) refuses a mount whose `source` resolves to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`, or any ancestor, in either layer, before boot. A share conveys an inode, not a listener, so mounting a socket directory grants the exposure without the capability that motivated it.

## flake-pilot

n/a: there is no bind-mount mechanism at the firecracker boundary, so there is nothing to refuse. Under the container backend a session socket is an ordinary `--opt "\-v ..."` and nothing objects — the shared-kernel answer, not this one.

[^read]: Read at `vivarium` `ceb0027` and `flake-pilot` `main` on 2026-08-18.
