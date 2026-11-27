# Develop on vivarium and install it locally

How to get a `viv` you can run while working on vivarium itself. This is a contributor runbook; for what the command surface is meant to do, start at [`getting-started.md`](./getting-started.md).

There are two entry points and they answer different questions. Inside this repository the development shell already provides one. Outside it, `just install` provides the other.

## Inside the repository: nothing to install

Entering the development shell — `nix develop`, or `direnv allow` once and then simply `cd` — puts `viv` on `PATH`:

```console
$ viv status
```

That `viv` is a shim. It builds the crate from the working tree on each invocation and then executes the binary, so it never lags an edit, and it shadows any `viv` installed elsewhere on the machine for as long as you are in the shell. The binary it builds carries the pin of this tree, as [Where the pin comes from](#where-the-pin-comes-from) below explains. The shim body is [`../../scripts/viv-shim.sh`](../../scripts/viv-shim.sh), which carries the rationale for its shape beside the code.

The cost is a `cargo build` before every run. On an unchanged tree that is close to free; after an edit it is the compile you were going to pay anyway.

## Where your builds and checks write

Working on vivarium means compiling a workspace, realising guest closures, and booting microVMs, which together measure in gigabytes. Most of that goes to the drive named by `VIVARIUM_HEAVY_DRIVE` rather than to your boot disk: build scratch, the compile cache, sandbox images and volumes, and vivarium's own state, data and cache roots. The one exception is the Nix store, which stays at `/nix/store` because a booting guest has to read its closure there — the checks refuse to start a run that will not fit in it rather than moving it.

Create the directory on the drive, then name it once per machine in `$HOME/.config/project-env/vivarium.env`, outside the checkout, which [`../../.envrc`](../../.envrc) loads:

```bash
# $HOME/.config/project-env/vivarium.env — per machine, outside the checkout
export VIVARIUM_HEAVY_DRIVE=/run/media/you/external/vivarium
```

The directory has to exist, and nothing creates it for you. That is what tells a mounted drive apart from an unplugged one, whose mountpoint is otherwise an empty directory on your boot disk that the checks would happily fill. Point the variable at a directory under the mount, as above, rather than at the mount itself — a subdirectory goes away when the drive does, which is what makes its absence mean something, while a mountpoint from `/etc/fstab` stays present unmounted and would pass the check against your boot disk.

The development shell puts the build folder of the worktree there (`CARGO_TARGET_DIR`, at `targets/<worktree>-<hash>` under the drive), and so does every check that compiles, evaluates a flake, or boots — one folder per worktree rather than two, and never one that two worktrees share. Without a usable drive the shell says so and keeps the folder under `~/.cache/vivarium/targets`, but the checks refuse: `git push` exits with nothing run and names what it needed. To run them on this disk anyway:

```console
$ VIVARIUM_HEAVY_ON_HOST=1 git push
```

That waives the drive and not the capacity check: the store and the scratch filesystem are still measured, so a disk that is genuinely full still refuses rather than failing mid-write. Two things are worth knowing before you rely on it: unplugging the drive during a build surfaces as cargo errors that read like a corrupt cache, and the first build after pointing at a new drive is a full one. The reasoning is in [`../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md`](../decisions/ADR-0106-gated-runs-put-their-bytes-on-the-heavy-drive.md), and the operator-facing detail in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

Each `rustc` call goes through [kache](https://github.com/kunobi-ninja/kache), the compile cache of the host, because [`../../.envrc`](../../.envrc) sets `RUSTC_WRAPPER=kache`. A new worktree then takes the crates that another worktree compiled from that cache rather than compiling them again. kache must be installed on the host, as [`../../host-requirements.toml`](../../host-requirements.toml) lists. `KACHE_DISABLED=1` runs one command without it, and CI, which enters the shell without direnv, builds without it. kache shares disk blocks with a build folder only on one filesystem, so a build folder on a drive other than the one that holds `~/.cache/kache` receives full copies.

## Outside the repository: `just install`

A sandbox is selected through a manifest's declared workspaces, so exercising one means running `viv` from a directory declared by a test manifest rather than from this repository. Install it:

```console
$ just install
```

This runs `cargo install --path . --locked --force`, which puts `viv` and `vivarium-supervisor` into cargo's binary directory — `~/.cargo/bin` unless `CARGO_HOME` says otherwise. The recipe prints where they landed and warns on standard error if that directory is not on your `PATH`, which is the only way this step fails quietly.

What you get is what a user gets: a `viv` that takes the pin it ships. The guest itself always comes from the tree the binary embeds — the installation supplies vivarium ([`../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../decisions/ADR-0102-the-installation-supplies-vivarium.md)) — so an installed `viv` built from your tree boots your guest module.

## Where the pin comes from

A generated project flake carries two baseline flake inputs, `nixpkgs` and `microvm`, and names their branches. [`../../nix/flake.lock`](../../nix/flake.lock) is the one pin, and the binary compiles it in. When a target has no lock yet, `viv` writes that pin as the target's first lock, so a new target reaches the forge for nothing. From then on the lock belongs to the user, who moves it with `viv update`. [`../decisions/ADR-0116-the-binary-ships-the-first-pin.md`](../decisions/ADR-0116-the-binary-ships-the-first-pin.md) records the decision, and [`../../src/config/shipped_pin.rs`](../../src/config/shipped_pin.rs) reads the pin.

Every `viv` built from this tree, the shim and an installed one alike, ships the pin of the tree it was built from. No variable, wrapper, or script redirects a baseline input.

When the `nixpkgs` pin of a lock is over 30 days old, the `exobrain outdated` line of [`../../.envrc`](../../.envrc) prints a warning on each shell load that names the age and the command that moves the pin. A move of `nix/flake.lock` changes what ships, so commit it as `fix(deps)`, and that commit opens a release request.

## Removing it

```console
$ just uninstall
```

Removes `viv` and `vivarium-supervisor`. The development shell's `viv` is unaffected — it is not installed anywhere, so there is nothing to remove.

## What this does not touch

The root [`../../flake.nix`](../../flake.nix) is the development environment and declares no build outputs; the shim is a shell package inside `devShells`, not a `packages` attribute. [`../../scripts/check-flake-boundary`](../../scripts/check-flake-boundary) enforces that distinction against the flake's evaluated attribute names, so run it after any change here.
