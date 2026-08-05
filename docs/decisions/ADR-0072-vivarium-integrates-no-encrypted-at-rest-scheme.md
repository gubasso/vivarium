# ADR-0072: vivarium integrates no encrypted-at-rest scheme

## Context and Problem Statement

[`ADR-0010-secrets-never-in-nix-store.md`](./ADR-0010-secrets-never-in-nix-store.md) named encrypted-at-rest one of two legal shapes a secret may take, but never said what vivarium does about it. [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md) meanwhile enumerated "anything read through the encrypted-at-rest channel" as secret-class, which presumes a channel vivarium reads through. Either vivarium builds one, or it states plainly that it does not.

## Considered Options

- A provider hook — a user-named command vivarium executes, piping plaintext into the guest.
- A first-party secrets schema carrying recipients, identities, and decryption.
- Integrate nothing — the encrypted shape stays legal and entirely user-owned.

## Decision Outcome

Chosen option: integrate nothing. A piece is a NixOS module, so a team wanting decrypt-at-activation imports one; vivarium needs no feature for it and gains no plaintext.

The distinction is between enforcing and performing. vivarium keeps refusing secrets in the build (N10), keeps rejecting a shared artifact that carries a literal personal path (N11), and keeps the runtime-injection channels it owns — the agent channel ([`ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md)) and read-only credential mounts. It supplies no decryptor, executes no provider, defines no secrets schema, and never holds a plaintext secret.

A provider hook is not a smaller version of this decision; it is its opposite. It would put plaintext in vivarium's own address space, turning [`ADR-0069-redaction-is-by-construction.md`](./ADR-0069-redaction-is-by-construction.md)'s guarantee from a structural fact into a claim to defend.

## Consequences

- Good: redaction stays trivially true, because there is no plaintext to leak.
- Good: any scheme that is a NixOS module works with no vivarium code, and there is no provider parity to maintain forever.
- Bad: onboarding a teammate to a team secret is an out-of-band workflow vivarium neither smooths nor obstructs.
- Bad: a guest-side scheme's decryption identity must arrive through a channel vivarium does own, which is a constraint users have to understand rather than one the tool manages.

## Status

Accepted

Amends [`ADR-0010-secrets-never-in-nix-store.md`](./ADR-0010-secrets-never-in-nix-store.md) — its Decision Outcome ranges over where a secret may live, not over what vivarium builds. The two shapes stay exactly as legal as they were; only the question of who performs them is now answered.

Specified in [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), [`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md), and [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md).
