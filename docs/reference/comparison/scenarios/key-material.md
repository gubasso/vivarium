# Key material

Record what has to cross the boundary before a tool inside can authenticate with a key the user already has.[^read]

1. Have a key the host's `ssh-agent` or `gpg-agent` already holds.
2. Make the tool inside the sandbox use it.
3. Record what is in the guest afterwards: the key, a copy of it, or neither.

## vivarium

Yes: neither. The host agent's socket is relayed on a dedicated credential port of the same vsock-class transport the control plane uses, and the guest talks to `/run/vivarium/ssh-agent.sock`. What may be forwarded is a closed allowlist of two, `ssh` and `gpg`, declared through a typed option that names no host path — which is what lets a shareable piece declare it — and the GPG side takes the agent's restricted extra socket rather than the ordinary one. A mount could not do this at all: a socket's endpoint is an object in the kernel that owns the listener, so a guest with its own kernel finds a name with nothing behind it. Two limits are stated rather than engineered away: a compromised guest can use the key for as long as the session lasts, and a byte relay does not carry the signal an agent uses to recognise a forwarded connection.

The declaration names a member of the closed enum and carries no host path, which is what lets a distributable piece hold it:

```nix
# pieces/ssh-agent/default.nix
{ ... }:
{ vivarium.credentials.agents = [ "ssh" ]; }
```

## flake-pilot

No: the firecracker boundary has neither a share nor a relay. A key reaches the guest only by being written into the image or into an `include.tar` / `include.path` payload, which is the material itself rather than its use.

The `krun` route has a share and still has no relay. Mounting the agent socket — `--opt "\-v $SSH_AUTH_SOCK ..."` — is the shared-kernel answer, and the guest here runs its own kernel, so the carried path arrives with no listener behind it. Nothing forwards an agent by any other route at either engine.

## glaipnir

No, by a different route: nothing is forwarded, and the design instead authenticates inside the sandbox and persists the result to a host cache directory the user owns. What ends up in the guest is a token rather than a private key, which is better than copying one — but an existing host key still cannot be used from inside.

## bunkerbox

No: there is no agent relay and no mount to carry a socket with, so a key the host holds is unreachable from the guest. The channel that does cross — [passthrough](./host-toolchain.md#bunkerbox) — reaches the other way, and with `profiles` empty it hands a command the host's real `HOME`, so an unsandboxed passthrough command reaches `~/.ssh` from the host side while the guest still cannot. Naming `ssh` in the whitelist is what would make that an answer to this row, and upstream says not to.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
