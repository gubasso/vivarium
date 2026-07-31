# vivarium

Declarative, deterministic, composable **microVM sandboxes** for projects and AI coding agents.

vivarium boots each project inside its own lightweight virtual machine — a separate guest kernel behind a hardware-virtualization boundary (VT-x/AMD-V) — instead of a shared-kernel container. The sandbox is described entirely in Nix: you compose reusable **images** and **config pieces** through a single **manifest**, and the same manifest produces the same VM on any machine.

The tool is a thin command-line wrapper. It does the boring parts — resolving the manifest, building the VM with Nix, mounting your working directory, wiring the network — and leaves the guest kernel, the isolation boundary, and the merge semantics to well-established building blocks.

## Highlights

- **Strong isolation** — a real guest kernel behind a hardware-virtualization boundary, not a shared-kernel namespace.
- **Declarative & deterministic** — the sandbox is a Nix build; a pinned lockfile makes it reproducible across machines and over time.
- **Composable** — small images and config pieces combine through one manifest, the single source of truth for a project's sandbox.
- **Your dev environment, untouched** — your project's own `flake.nix` / direnv setup runs _inside_ the sandbox, independent of vivarium.
- **User-based** — all state lives under standard per-user XDG directories.

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
