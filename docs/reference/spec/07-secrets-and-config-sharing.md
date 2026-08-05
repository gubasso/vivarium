# 07 — Secrets and config sharing

How a project's sandbox configuration is shared across a team without leaking personal or secret data. Two rules govern this: secrets never enter the build (see [`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)), and machine-specific paths are injected at launch, not built in (see [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md)).

## The shared / personal split

Configuration divides into two classes:

- Shared — portable, safe to distribute in a config library: images and pieces. These contain no literal personal paths and no secrets — host context appears only as portable variables resolved at launch (see below) — so a team can track and distribute them together. Anything that must hold for everyone belongs here, because a piece can set it with `mkForce` and no individual can then override it (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)).
- Personal / machine-local — the user's own: the manifest, resource overrides, machine identity, and anything host-specific. These live in the user's own per-user config and state (see [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). The manifest names which image and pieces to adopt and carries the values only that user needs; host-specific values are injected at launch, never written into a shared artifact.

The manifest sits in the personal class because there is exactly one per project (N7) and no overlay: were it shared, every per-user adjustment would mean editing the file the team distributes. A user may publish their manifest for others to read, but nothing imports another user's manifest — the shared surface is images and pieces. See [`../../decisions/ADR-0040-manifest-is-the-personal-layer.md`](../../decisions/ADR-0040-manifest-is-the-personal-layer.md).

The guiding principle: shared artifacts describe the concern; the user's own manifest and machine supply the rest. A shared image or piece that contains an absolute home path or an inline credential has crossed the line and belongs in the personal class instead.

## Eliminating machine-specific paths

The most common leak is an absolute working-directory path. It is eliminated by convention: the working directory is always mounted to the same fixed in-guest location and injected at launch, so no host path appears in any configuration file. Other machine-local paths follow the same rule — supplied at launch, not written into shared config.

## Secrets

A secret embedded during the build lands in the world-readable store and ships with the VM's closure, so build-time secrets are prohibited. Two safe shapes replace them, and vivarium's relationship to each is different:

Note who the audience for a store path actually is, because vivarium widens it. The store is shared read-only into every guest ([`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md), [`../../decisions/ADR-0038-guest-store-sharing.md`](../../decisions/ADR-0038-guest-store-sharing.md)) — the same mechanism that makes an additional running project nearly free. So the reach of a store secret is not "other users of this host" but every sandbox on the machine, including those deliberately running untrusted code. This does not change N10, which admits no exception either way; it is why the rule earns no exception. The walkthrough for each safe channel is [`../../guides/keep-secrets-out-of-the-store.md`](../../guides/keep-secrets-out-of-the-store.md).

- Encrypted-at-rest, for shared secrets. Commit encrypted secret files that decrypt only at activation into a runtime-only location, never into the store; only the public recipient identities are committed in clear. vivarium performs no part of this — it supplies no decryptor, executes no provider, and defines no secrets schema ([`../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md`](../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md)). A piece is a NixOS module, so a team that wants decrypt-at-activation imports one; see "What vivarium does not do" below.
- Runtime injection, for personal secrets. This vivarium does own. The primary channel is authentication-agent socket forwarding (SSH/GPG): the guest receives a socket, so a compromised guest can use a key for the session but the private key material never crosses the boundary and cannot be exfiltrated. Where a token is unavoidable, pass a scoped, short-lived one as runtime environment — never a long-lived credential. Credential directories may be bind-mounted read-only (`ro,nodev,nosuid,noexec`) for tools that read config files.

The rule is: build-time means in the store, which is wrong for secrets; secrets are runtime or encrypted-at-rest only. Neither shape involves the build, and vivarium enforces that rule without performing either shape's cryptography.

### The agent channel

Settled in [`../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md). A host authentication-agent socket cannot be delivered through a filesystem share: a Unix socket's endpoint is an object in the kernel that owns the listener, and a share conveys the inode, not the listener — so a guest with its own kernel (N1) finds a name with nothing behind it. Forwarding is therefore a relay, carried on a dedicated credential port of the same vsock-class transport as the control plane ([`12-exec-and-shell.md`](./12-exec-and-shell.md)), and never a mount.

What may be forwarded is a closed allowlist of two, declared through the typed `vivarium.credentials.agents` option and off unless a layer opts in:

| Id    | Host source                                                  | Guest path                     |
| ----- | ------------------------------------------------------------ | ------------------------------ |
| `ssh` | `$SSH_AUTH_SOCK`                                             | `/run/vivarium/ssh-agent.sock` |
| `gpg` | the agent's restricted extra socket — never the ordinary one | `/run/vivarium/gpg-agent.sock` |

An arbitrary host path is never forwardable: the option's value is a member of the enum above, carries no host path, and so a shared piece may declare it without violating N11 — which is what makes a distributable `ssh-agent` piece honest ([`03-artifact-model.md`](./03-artifact-model.md)). The guest's `SSH_AUTH_SOCK` is set by the guest agent to the fixed guest path above; it is a tool-generated value, not a forwarded host one, so N17's deny-by-default rule is untouched. Refusing the unrestricted GPG socket is deliberate: the extra socket exists to sign and decrypt for a remote consumer without exposing the key, and the ordinary one does not.

Two limits are stated rather than engineered away. A compromised guest can use a forwarded key for as long as the session lasts — the channel protects the key material, not its authority — so a user who wants use-time confirmation or destination constraints configures them on their own agent, which is the only place they can live. And a byte relay does not carry the signal an agent uses to recognize a forwarded connection, so agent-side restrictions that depend on it do not apply inside the guest.

### What vivarium does not do

The non-goal at [`00-goals-and-non-goals.md`](./00-goals-and-non-goals.md) — not a secrets manager — is enforceable rather than aspirational because this list is exhaustive. vivarium will not:

1. store a credential in any root — config, state, cache, data, or the store;
2. hold, generate, derive, or rotate a decryption identity or key;
3. write decrypted material to any host filesystem path;
4. implement or depend on any decryption itself;
5. execute a user-named provider command and relay its output;
6. offer any verb whose subject is a credential value;
7. expire, renew, or reason about a credential's lifetime — that is what makes "scoped, short-lived" the user's choice above and not a promise here.

Items 4 and 5 are the load-bearing pair. A provider hook looks like the smallest possible integration and is in fact the opposite of this decision: it would place plaintext in vivarium's own address space, which is exactly the condition [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)'s redaction guarantee is free of today. A user remains free to compose any encrypted-at-rest scheme into their own image or piece; vivarium neither ships one nor stands in the way, and the identity such a scheme needs arrives through the channels above.

### Your manifest is compiled, so it is in the store too

This reaches further than "do not read a credential during the build". The manifest is not just a name-list the tool consults — it is compiled into a module in the generated flake, and Nix realizes that flake ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md), [`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md)). Every value written in a manifest therefore lands in the world-readable store, including the launch-channel tables: `[env]`, `[[mounts]]`, and `[resources]` are read by pure evaluation before they are applied at launch, so being launch-channel keeps a value out of every build output (N19), never out of the store the compiled text is copied into.

So `[env] TOKEN = "…"` is a build-time secret, whatever the channel classification suggests. Being the personal layer buys nothing here either — N11 permits literal personal paths in your own manifest, and N10 still forbids a secret. A token reaches the guest as a runtime environment value injected at launch (`--env`, [`12-exec-and-shell.md`](./12-exec-and-shell.md)) or through one of the two channels above; it is never written into any file vivarium compiles.

## Mirroring host configuration

Host config files and directories are mirrored into the guest through the declarative mount schema ([`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md), [`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md), [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md)): the host `source` expands host-side variables at launch, the guest `target` expands `~` to the guest home, and `readonly = true` marks identity files that must not be written. Because each side expands its own home, identity mounts (`~/.config/foo` → `~/.config/foo`) need no path translation even though the guest username differs from the host's.

Mount declarations travel the launch channel and are never build inputs (N5, N19, [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)): an unset variable or missing host path fails before boot with a legible error, and no expanded path is ever recorded in a shared artifact (N11). One caveat: mirroring is transparent at the path layer only — a mirrored file whose contents embed a host-absolute path is not rewritten. Runtime environment values pass through `vivarium.env` (pieces) or the manifest `[env]` table, subject to the deny-by-default rule (N17).

### Portable variables — the sharing rule

A shared layer references the host only through portable variables: `${HOME}` and the four durable XDG directories (`${XDG_CONFIG_HOME}`, `${XDG_DATA_HOME}`, `${XDG_STATE_HOME}`, `${XDG_CACHE_HOME}`). These are machine-independent names — every host resolves them — so a piece declaring `source = "${HOME}/.config/foo"` stays committable: nothing personal appears until launch expands it, and the expanded value never lands in an artifact. A literal personal path (`/home/alice/…`) in a shared image or piece fails validation before the build; literal paths are legal only in the personal layer — the user's own manifest. The check runs during evaluation, so `viv config eval`, `viv start`, and a cold-starting `exec`/`shell` reject it with `65`, while `viv config sources` renders it as a defect and still exits `0` ([`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)). `viv doctor` carries a cheaper textual lint over the same rule, `shared-layer-paths-portable`, which flags a literal without evaluating ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)).

What vivarium's own diagnostics may say about any of this — which values never reach a log, why a generated command line is structured rather than pasteable, and why personal paths are normalized rather than blanked — is the redaction contract in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md).

`${XDG_RUNTIME_DIR}` is deliberately not in that set, and neither layer may mount it (N24). The four above name durable user data; the runtime directory names live session state — the session bus, the display socket, the authentication-agent socket — which is the one class of host data a sandbox must not receive wholesale. Two independent reasons close this off. It would not work: mounting a directory of sockets delivers inodes with no listener behind them, for the reason "The agent channel" gives above, so a session socket has never been obtainable this way by anyone. And it must not be tried: a directory of live session endpoints is precisely what deny-by-default (N17) exists to keep out, and the one legitimate use case — reaching an authentication agent — is now served by a named channel that carries no host path at all. Excluding it costs nothing else, because a shared layer has no other use for a directory that is empty at every fresh login.

This rule is what keeps a piece whole and still shareable: an application piece carries the packages, guest config, runtime env, and host-config mounts its application needs, and adopting the piece brings all of it (see [`03-artifact-model.md`](./03-artifact-model.md)).

## Enforcing the split

Because the working-directory path is never written to config and secrets are never built in, the shared class stays genuinely shareable. Tooling guards the boundary with read-only checks — never by writing into the user's config (N13). The personal class already has a home in the per-user config and state roots, the manifest chief among them; vivarium does not scaffold it (see [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md)).

The two halves of N11 are not equally enforceable, and the difference is worth stating plainly rather than leaving a reader to assume the tool sees more than it does:

- A literal personal path is decidable, because it is a property of text vivarium already parses. It is a genuine refusal: evaluation rejects it with `65`, as above.
- A plaintext secret is not decidable. Nothing distinguishes a credential from a fixture by inspection. `manifest-no-inline-secret` ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)) is a heuristic that warns, and a value it does not flag is not a promise that the value is safe.

So N11's secret clause is a rule the user upholds and the tool assists with; its path clause is a rule the tool upholds. What refusal means here is narrow and worth being precise about: vivarium declines to lend its own machinery to a defect it can see. It cannot stop a user from writing anything they like into their own files, and it does not try — the checks are read-only, the config root is never written (N13), and the guardrails exist to make the right thing obvious rather than to make the wrong thing impossible.
