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
