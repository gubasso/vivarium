# vivarium

Declarative, deterministic, composable microVM sandboxes for projects and AI coding agents.

vivarium boots each project inside its own lightweight virtual machine: a separate guest kernel behind a hardware-virtualization boundary rather than a shared-kernel container. Nix describes the sandbox, and reusable images and config pieces compose through a single manifest.

The command-line tool resolves the manifest, builds the VM with Nix, mounts the working directory, and wires the network. Established Nix and virtualization components provide the guest kernel, isolation boundary, and merge semantics.

## Highlights

- Strong isolation: a real guest kernel behind a hardware-virtualization boundary.
- Declarative and deterministic: a pinned lockfile makes the sandbox reproducible across machines and over time.
- Composable: small images and config pieces combine through one project manifest.
- Your development environment stays untouched: the project's own `flake.nix` and direnv setup run inside the sandbox.
- Cheap concurrency: guests share the host's Nix store read-only while guest writes land in a guest-local overlay.
- Disposable: project work remains in the user's version-controlled host directory, so rebuilding is the recovery path.
- User-based: state lives under standard per-user XDG directories.

## Never put a secret in the store

A secret must never enter a Nix build. The store is world-readable by construction, and every guest reads the shared store. The manifest is compiled into a generated flake, so its text also reaches the store.

Use vivarium's runtime credential channels or a user-owned encrypted-at-rest workflow that decrypts only during activation. vivarium provides injection channels and performs no cryptography. See [Keep secrets out of the store](docs/guides/keep-secrets-out-of-the-store.md).

## Status

See [implementation status](docs/reference/implementation-status.md) for the sole current account of what runs today and what remains designed.

## Security

Report vulnerabilities privately as described in [the security policy](./SECURITY.md).

## Documentation

Start at the [documentation index](docs/README.md), which covers the five zones: plan, reference, explanation, guides, and decisions.

## Development shell

vivarium provides a pinned Nix development shell.

```bash
# Interactive: allow direnv to load the shell on cd
direnv allow

# Ad hoc: enter the development shell directly
nix develop
```

## License

vivarium is licensed under the [MIT license](LICENSE).
