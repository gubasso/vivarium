# Shipping a secret

Record whether a secret the environment needs can travel with the project's own definition, rather than being installed on each machine by hand.[^read]

1. Put a secret the environment needs into the project's own files, in a form safe to commit.
2. Hand the project to a second person who holds a decryption identity.
3. Record what reaches the guest, what reaches the build, and what the tool itself had to do.

## vivarium

Reachable, nothing arranges it: encrypted-at-rest is one of the two shapes the [specification](../../spec/07-secrets-and-config-sharing.md) names for a secret, and it is the one meant for sharing — commit files that decrypt at activation into a runtime-only location, never into the store, with only the public recipient identities in clear. What vivarium supplies is the seam, not the scheme. A piece is a NixOS module, so a team that wants decrypt-at-activation imports one the way it imports anything else, and the identity that scheme needs arrives over the GPG relay of the row above rather than as a mounted host path. What it will not do is a closed list of seven, not a gap: no decryptor, no provider command executed and relayed, no decryption identity held, no plaintext written to a host path, no credential in any root, no verb whose subject is a credential value, and no reasoning about a credential's lifetime ([`ADR-0072`](../../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md)). The fourth and fifth are the load-bearing pair: a provider hook looks like the smallest possible integration and is the opposite of one, because it would put plaintext in vivarium's own address space, which is the condition its redaction guarantee is free of today. Available rather than provided, and deliberately so.

## flake-pilot

No: the repository has no secrets mechanism of any kind, encrypted or otherwise — the only matches for the word are a CI workflow's own credentials. Material a registration needs reaches the guest as image content or as an `include.tar` / `include.path` payload, and it travels in clear either way — but neither is a way of shipping it beside the definition, because an include is a host path read at provisioning on the machine that registers, and the image is pulled rather than committed to. The second person registers with their own payload, and [the row above](./secrets-in-the-build.md#flake-pilot) records where that payload lands.

This is a property of the repository rather than of an engine, so it holds identically at both routes.

## glaipnir

No: the stance is that the image holds no secret at all, stated as a documented guarantee, and nothing ships one beside the definition either. Authentication happens at runtime inside the container and the result persists to a host cache directory the user owns, which is a per-machine step by construction: the second person authenticates again rather than receiving anything.

## podman

Reachable, nothing arranges it: `podman secret create` takes a `pass` driver, where the secret "resides in a GPG-encrypted file", and a `shell` driver that hands storage to scripts of the user's choosing; `--secret` then mounts it at runtime rather than baking it in. Two things keep the arrangement the user's own. The default `file` driver is a read-protected file and not an encrypted one, so the safe answer is the one you have to ask for. And the store is machine-local podman state that a `Containerfile` or Quadlet unit refers to by name — a `pass` store can itself be shared, but the binding to the project is a name that must already resolve, so the second person runs a command before anything works.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `flake-pilot` `44e3ab2` on 2026-08-20; `glaipnir` `8c7420e`, read 2026-08-20 — a later revision than the `21ef389` the rest of this subject is pinned to, read fresh for this row rather than inferred from the earlier one; `podman` 5.x on 2026-08-20.
