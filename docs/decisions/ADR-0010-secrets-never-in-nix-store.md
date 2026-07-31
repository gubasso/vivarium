# ADR-0010: Keep secrets out of the Nix store

## Context and Problem Statement

Sandboxes need credentials — ssh keys, tokens, cloud credentials — to be useful. Anything embedded during the Nix build lands in the store, which is world-readable and travels with the VM's closure. A secret baked into the build is therefore exposed to every user of the host and to anyone who obtains the image. The design must guarantee secrets never enter the store.

## Considered Options

- **Build-time secrets** — read credentials during the build and embed them in the VM definition.
- **Encrypted-at-rest secrets** — commit encrypted secret files decrypted only at activation into a runtime-only location (for example an age- or KMS-based Nix secrets scheme).
- **Runtime injection** — never involve the build; forward an agent socket, bind-mount credential directories read-only, and pass tokens as runtime environment when the VM launches.

## Decision Outcome

Chosen option: **encrypted-at-rest for shared secrets, runtime injection for personal secrets**; **build-time secrets are prohibited**. Team secrets that must be committed use an encrypted scheme that decrypts to a runtime-only path, never to the store. Personal secrets are injected at launch — agent-socket forwarding, read-only credential mounts, and runtime environment variables.

The rule is simple: **build-time means in the store, which is wrong for secrets; secrets are runtime or encrypted-at-rest only.** See [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).

## Consequences

- Good: no credential can leak through a world-readable store path or a shipped closure.
- Good: two clear channels — encrypted-at-rest for shared, runtime-injected for personal.
- Good: committed configuration stays shareable because it contains no plaintext secrets.
- Bad: contributors must set up an encryption scheme or runtime injection rather than inlining a value.
- Bad: runtime injection depends on host state (a running agent, present credential files) that the build cannot guarantee.

## Status

Accepted

Amended by [`ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md`](./ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md) — the Decision Outcome above ranges over **where a secret may live**, not over what vivarium builds. Both shapes remain exactly as legal as they are stated here; the encrypted-at-rest one is performed entirely by the user, and vivarium supplies no decryptor, executes no provider, and holds no identity (N25). The option list above named "an age- or KMS-based Nix secrets scheme" as an example, and it stays an example rather than becoming an integration.

Amended by [`ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — "forward an agent socket", named as a runtime-injection mechanism above, is a **relay over the control transport's second port**, not a share or a mount; a filesystem share cannot carry a live socket at all.

Amended by [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — compiling a manifest into the generated flake copies **the manifest's own text into the store**, so the prohibition above demonstrably reaches a value written in a manifest's `[env]` table, not only one read during the build — being launch-channel keeps that value out of every build output (N19), never out of the store. The rule is unchanged; its reach is now explicit, and stated in [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).
