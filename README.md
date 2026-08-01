# vivarium

Declarative, deterministic, composable **microVM sandboxes** for projects and AI coding agents.

vivarium boots each project inside its own lightweight virtual machine — a separate guest kernel behind a hardware-virtualization boundary (VT-x/AMD-V) — instead of a shared-kernel container. The sandbox is described entirely in Nix: you compose reusable **images** and **config pieces** through a single **manifest**, and the same manifest produces the same VM on any machine.

The tool is a thin command-line wrapper. It does the boring parts — resolving the manifest, building the VM with Nix, mounting your working directory, wiring the network — and leaves the guest kernel, the isolation boundary, and the merge semantics to well-established building blocks.

## Highlights

- **Strong isolation** — a real guest kernel behind a hardware-virtualization boundary, not a shared-kernel namespace.
- **Declarative & deterministic** — the sandbox is a Nix build; a pinned lockfile makes it reproducible across machines and over time.
- **Composable** — small images and config pieces combine through one manifest, the single source of truth for a project's sandbox.
- **Your dev environment, untouched** — your project's own `flake.nix` / direnv setup runs _inside_ the sandbox, independent of vivarium.
- **Cheap to run several** — because the sandbox is built with Nix, guests read the **host's Nix store, shared read-only**: no per-VM store image to build, no duplicated store bytes, and one host page cache serving every guest. Running a fifth project costs close to nothing on disk. The share is read-only on both sides, served by its own confined daemon, and guest writes land in a guest-local overlay — so the host store is never modified from inside a sandbox.
- **Disposable** — destroy and rebuild at any time and lose nothing that matters: your work is in your own version-controlled directory on the host, and the environment is in the manifest. Let an agent wreck a sandbox; rebuilding one is the recovery path.
- **User-based** — all state lives under standard per-user XDG directories.

## One rule that comes with this: never put a secret in the store

The shared store is what makes extra sandboxes nearly free, and it is also why one rule is non-negotiable: **a secret must never enter a Nix build.** The store is world-readable by construction, and every guest reads it — so a credential that reaches a store path is exposed to every sandbox on the machine, including the ones you filled with untrusted code.

This reaches further than "don't read a credential during the build". Your manifest is compiled into a flake and realized, so its own text lands in the store: `[env] TOKEN = "…"` is a build-time secret whatever its channel classification suggests.

Use the channels vivarium does provide — a forwarded authentication-agent socket, a scoped short-lived token passed at launch, a read-only credential mount — or, for team secrets, an encrypted-at-rest scheme that decrypts at activation into a runtime-only path. vivarium enforces the rule; it performs none of the cryptography. Step-by-step, with a worked example for each channel: [`docs/guides/keep-secrets-out-of-the-store.md`](docs/guides/keep-secrets-out-of-the-store.md).

## Status

Design stage. The architecture is specified; the implementation has not started. See [`docs/reference/implementation-status.md`](docs/reference/implementation-status.md) for what runs today versus what is designed only, and for what a release may change before 1.0.

## Security

Report a vulnerability privately — see [`SECURITY.md`](./SECURITY.md), which also explains why a fix to the virtualization backend arrives as a moved lockfile pin rather than a host package update.

## Documentation

Start at [`docs/README.md`](docs/README.md) — an index into the decisions, product spec, guides, and architecture explanation.

## Development shell

vivarium ships a Nix flake devShell with its tooling pinned.

```bash
# Interactive: allow direnv to load the shell on cd
direnv allow

# Ad hoc: enter the devShell directly
nix develop
```

## License

vivarium is licensed under the terms described in the [LICENSE](LICENSE) file (MIT).
