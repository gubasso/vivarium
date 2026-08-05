# Keep secrets out of the store

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when a sandbox needs a credential — a git signing key, a registry token, a cloud profile. The rule it walks you through is one sentence: a secret never goes anywhere Nix can see it. The normative form of that rule is N10 in [invariants and guarantees](../reference/spec/08-invariants-and-guarantees.md); the contract it belongs to is [secrets and config sharing](../reference/spec/07-secrets-and-config-sharing.md), which owns every fact this guide only puts in order.

## Why the rule is sharper here than elsewhere

Two properties compose into one consequence, and it is worth seeing the composition rather than memorizing the conclusion.

1. The Nix store is world-readable by construction. It always was; that is not vivarium's doing.
2. Every guest reads the host's store, shared read-only — the decision that makes each additional running project nearly free ([guest store sharing](../decisions/ADR-0038-guest-store-sharing.md), contract in [the store inside the guest](../reference/spec/06-workspace-and-project-environment.md)).

So the audience for a store path is not "other users on this laptop". It is every sandbox on this machine, including the ones you deliberately filled with untrusted code. The saving and the caveat are the same mechanism seen from two sides, which is why they are documented together.

The non-obvious half is what counts as "in the store". Your manifest is not a name-list the tool consults — it is compiled into a flake and realized, so its own text is copied into the store, launch-channel tables included ([why the manifest's text lands there](../reference/spec/07-secrets-and-config-sharing.md)). This is a build-time secret:

```toml
# WRONG — this string is now a world-readable store path
[env]
GITHUB_TOKEN = "ghp_…"
```

Being launch-channel keeps the value out of every build output (N19). It does not keep the text that declares it out of the store.

## Step 1 — Prefer the agent channel

The best credential is one that never crosses the boundary at all. For SSH and GPG, forward the agent rather than the key: the guest gets a socket it can ask to sign, and the private key material stays on the host. A compromised guest can use the key for the session; it cannot walk away with it.

Because the option names an id and not a host path, a shared piece may declare it without violating N11:

```nix
{ vivarium.credentials.agents = [ "ssh" ]; }
```

```console
$ viv start
$ viv exec -- git push
```

The guest's `SSH_AUTH_SOCK` is set by the guest agent to a fixed guest path — it is not your host variable forwarded, and `--env SSH_AUTH_SOCK` is not a substitute for the channel. The closed allowlist of what may cross (`ssh`, `gpg`, and nothing else), and why the ordinary GPG socket is refused in favor of the restricted extra one, are in [the agent channel](../reference/spec/07-secrets-and-config-sharing.md); the transport is [ADR-0071](../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md).

## Step 2 — Pass a token at launch, never in a file

When a token is unavoidable, hand it over per-invocation so it lives in the guest's process environment and nowhere else:

```console
# reads the value from your host environment; nothing is written down
$ viv exec --env GITHUB_TOKEN -- gh pr list

# or supply a literal for one command
$ viv exec --env GITHUB_TOKEN=ghp_… -- gh pr list
```

Make it scoped and short-lived. vivarium never expires, renews, or reasons about a credential's lifetime, so "short-lived" is your choice and not a promise the tool keeps for you. Host environment passthrough is deny-by-default — the allowlist is `TERM`, `COLORTERM`, `NO_COLOR`, `FORCE_COLOR`, `LANG`, `LC_*`, and nothing else crosses unless you name it (N17, [exec and shell](../reference/spec/12-exec-and-shell.md)).

## Step 3 — Mount credential files read-only

For tools that insist on reading a config file, mirror the directory instead of copying its contents into configuration:

```toml
[[mounts]]
source = "${HOME}/.config/gh"
target = "~/.config/gh"
readonly = true
```

Each side expands its own home, so no path translation is needed even though the guest username differs. `readonly = true` marks identity files that must not be written; the mount schema is owned by [ADR-0020](../decisions/ADR-0020-mount-and-config-mirroring-schema.md) and specified in [secrets and config sharing](../reference/spec/07-secrets-and-config-sharing.md).

Two limits worth knowing before you reach for this. A portable variable (`${HOME}`, `${XDG_*_HOME}`) keeps the declaration shareable; a literal `/home/alice/…` in an image or piece is refused at evaluation with `65`. And a mount `source` may never resolve to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` (N24) — mounting a directory of live sockets grants exposure without granting the capability that motivated it, because a share conveys the inode and not the listener.

## Step 4 — Team secrets: encrypted at rest, and entirely yours

A secret several people need cannot travel by any of the above. The legal shape is encrypted at rest: commit ciphertext, decrypt at activation into a runtime-only path, never into the store.

vivarium performs no part of this. It supplies no decryptor, executes no provider command, defines no secrets schema, and holds no decryption identity (N25, [ADR-0072](../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md)). It is not standing in your way either — a piece is a NixOS module, so any decrypt-at-activation module composes with no vivarium feature at all.

The two established modules for this shape are:

- [sops-nix](https://github.com/Mic92/sops-nix) — a NixOS module around [SOPS](https://github.com/getsops/sops), which encrypts structured files (YAML, JSON, `.env`, INI) value by value so the keys stay readable in a diff. Recommended when a team edits secrets often or wants one file to hold many, and when you may later want a KMS (AWS/GCP/Azure) or PGP recipient alongside age.
- [agenix](https://github.com/ryantm/agenix) — a thinner module over [age](https://github.com/FiloSottile/age), one encrypted file per secret, no structured-format layer. Recommended when you want the smallest thing that works.

Both do the same job: keep ciphertext in git, decrypt at activation to a runtime path, keep plaintext out of the store. The walkthrough below uses sops-nix with an age key; agenix differs in the option names and not in the shape, and either substitutes cleanly.

1. Create an identity, on the host, and keep it there.

```console
$ age-keygen -o ~/.config/vivarium-secrets/identity.txt
Public key: age1ql3z…
```

1. Commit only the public recipients. `.sops.yaml` says who may decrypt what. Nothing in it is secret — these are public keys, and a teammate joining means appending theirs and re-keying.

```yaml
# .sops.yaml
keys:
  - &alice age1ql3z…
  - &bob age1f7k2…
creation_rules:
  - path_regex: secrets/.*\.yaml$
    key_groups:
      - age: [*alice, *bob]
```

1. Encrypt the secret and commit the ciphertext. `sops` opens your editor and writes back a file whose keys are readable and whose values are encrypted — which is what makes it reviewable in a diff.

```console
$ sops secrets/registry.yaml     # edit in cleartext; saved encrypted
$ git add secrets/registry.yaml  # ciphertext is safe in the repo
```

1. Decrypt at activation, into a runtime-only path. Import sops-nix into your piece — a piece is a NixOS module, so this is an ordinary import — and declare the secret:

```nix
{
  sops.defaultSopsFile = ./secrets/registry.yaml;
  sops.age.keyFile = "/run/vivarium-keys/identity.txt"; # step 5 puts it here

  sops.secrets."registry_token" = {
    path = "/run/secrets/registry-token"; # tmpfs — never /nix/store
    mode = "0400";
    owner = "vivarium"; # the default guest user
  };
}
```

The one thing to get right is the destination. A module that writes plaintext to a store path has defeated the exercise; `/run` is correct because it is memory-backed and gone at shutdown, which the [disposable sandbox](../decisions/ADR-0080-the-sandbox-is-disposable.md) makes a feature rather than a loss. Note that the secret arrives as a file, not an environment variable — read it at the point of use (`$(cat /run/secrets/registry-token)`) rather than exporting it, so it does not spread into every child process's environment.

1. Get the identity into the guest — through a channel vivarium does own. This is the step that is easy to get wrong, because the age identity is itself a secret and so cannot ride in the manifest, an image, or a piece. Mount it read-only, using the credential mount from Step 3 of this guide:

```toml
[[mounts]]
source = "${HOME}/.config/vivarium-secrets"
target = "/run/vivarium-keys"
readonly = true
```

Alternatively, sops-nix can derive an age identity from an SSH host key, which lets step 1's agent channel carry the trust instead — the same substitution, one less file to mount. Either way the identity never goes through the store.

That handoff is a constraint you have to understand rather than one the tool manages — stated plainly as a consequence in [ADR-0072](../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md).

## What the tool will and will not catch

Be precise about this, because the two halves of N11 are not equally enforceable and the difference is part of the invariant rather than a gap in it:

- A literal personal path in a shared image or piece is decidable from text. Evaluation refuses it with `65`, and `viv doctor` carries a cheaper textual lint (`shared-layer-paths-portable`) that flags it without evaluating.
- A plaintext secret is not decidable. Nothing distinguishes a credential from a fixture by inspection. `manifest-no-inline-secret` ([doctor and health checks](../reference/spec/13-doctor-and-health-checks.md)) is a heuristic that warns, and a value it does not flag is not a promise that the value is safe.

So this rule is one you uphold and the tool assists with. Its guardrails exist to make the right thing obvious, not to make the wrong thing impossible — the checks are read-only and vivarium never writes to your config root (N13).

## If a secret did reach the store

Treat it as disclosed and rotate it. Do not reason about who might have read it: by the time it is a store path it is readable by every local user and by every running sandbox, and a store path can persist in generations and GC roots long after you edited the file that produced it ([generations and build history](../reference/spec/11-generations-and-build-history.md)). Rotation is the only step that ends the exposure; removing the file is not.

## Acceptance coverage

None yet. Unlike the other per-task guides, this one pairs with no gated trial in [`user_workflows.rs`](../../tests/user_workflows.rs) — the decidable half of the rule is already covered by `workflow_03_literal_path`, and the undecidable half is a heuristic by design, so there is no assertion that could encode "no secret is present". Stating that is more honest than inventing a trial that would prove less than its name suggests.
