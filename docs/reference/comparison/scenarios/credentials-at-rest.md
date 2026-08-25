# Credentials at rest

An agent that logs in leaves a token behind. Record what that token looks like on the host between runs. Whether a secret can travel with the project is [a different row](./shipping-a-secret.md); this one asks about state the tool itself produced.[^read]

1. Authenticate the tool inside the sandbox so it writes its own credential file.
2. Stop the sandbox.
3. Read that file on the host, and record whether its contents are legible.

## vivarium

No, by design: the [no-decryption-identity rule](../../spec/08-invariants-and-guarantees.md) states that vivarium decrypts nothing and holds no identity, so it has no key with which to seal a home volume and no place to keep one. What it offers instead is upstream of this row — credentials arrive through runtime injection channels and never enter the store ([`07-secrets-and-config-sharing.md`](../../spec/07-secrets-and-config-sharing.md)) — and what the guest then writes into its home volume is an ordinary file in an ext4 image under the state root. Encrypting that is a workflow the user owns, which is the same position [the shipping row](./shipping-a-secret.md) records.

## bunkerbox

Yes: an image config lists credential paths under `encrypt`, relative to the persisted home. Before the container starts, bunkerbox prompts for a passphrase and decrypts every `.enc-cipher` file in place; when the container exits, it encrypts each matching file back and removes the plaintext. AES-256-GCM, with the key derived per file by PBKDF2-HMAC-SHA256 at 100,000 iterations over a random salt. `BUNKERBOX_ENCRYPT_KEY` supplies the passphrase without a prompt, and a wrong one prompts per file to delete the undecryptable copy or abort.

```yaml
encrypt:
  - ".local/share/opencode/auth.json"
  - ".local/share/opencode/account.json"
```

Two scopings keep the verdict honest. The protection is between runs and not during one: while the container is up the plaintext is in the persisted home, which is exactly where the agent works. And the list belongs to the packager — `encrypt` is one of three keys a project config may not override, alongside `network` and `oci`.

[^read]: Read at `bunkerbox` `b7f14f3` on 2026-08-25, and at `vivarium` `162f230`. The unlinked `flake-pilot` and `glaipnir` cells are absence claims derived from the inventories in [`../feature-sweep.md`](../feature-sweep.md), not from a fresh read: neither tool encrypts anything.
