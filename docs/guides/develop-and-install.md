# Develop on vivarium and install it locally

How to get a `viv` you can run while working on vivarium itself. This is a contributor runbook; for what the command surface is meant to do, start at [`getting-started.md`](./getting-started.md).

There are two entry points and they answer different questions. Inside this repository the development shell already provides one. Outside it, `just install` provides the other.

## Inside the repository: nothing to install

Entering the development shell — `nix develop`, or `direnv allow` once and then simply `cd` — puts `viv` on `PATH`:

```console
$ viv status
```

That `viv` is a shim. It builds the crate from the working tree on each invocation and then executes the binary, so it never lags an edit, and it shadows any `viv` installed elsewhere on the machine for as long as you are in the shell. It also pins its baseline flake inputs to this tree, for the reason in [Why the pin matters](#why-the-pin-matters) below. The shim body is [`../../scripts/viv-shim.sh`](../../scripts/viv-shim.sh), which carries the rationale for its shape beside the code.

The cost is a `cargo build` before every run. On an unchanged tree that is close to free; after an edit it is the compile you were going to pay anyway.

## Outside the repository: `just install`

A sandbox is bound to a project directory, so exercising one means running `viv` somewhere other than here. Install it:

```console
$ just install
```

This runs `cargo install --path . --locked --force`, which puts `viv` and `vivarium-supervisor` into cargo's binary directory — `~/.cargo/bin` unless `CARGO_HOME` says otherwise. The recipe prints where they landed and warns on standard error if that directory is not on your `PATH`, which is the only way this step fails quietly.

What you get is what a user gets: a `viv` whose baseline flake inputs resolve live. That is the right thing to run when you are checking the command surface, diagnostics, or state handling. It is the wrong thing to run when you are checking a guest.

## Why the pin matters

`viv` ships branch references for the three flake inputs a generated project flake carries, so a released tool resolves today's upstream and pins it in the lock the first evaluation writes. The default for the third of them is `github:gubasso/vivarium?dir=nix`; see `BASELINE_VIVARIUM_VARIABLE` and its neighbours in [`../../src/config/flake.rs`](../../src/config/flake.rs) for the shipped values and the environment variables that override them.

For a developer that default is a trap. A `viv` you built from your own tree still builds its guest from the published branch, so a change to the guest module or the launch seam does not appear in the sandbox you just booted, and the run reports on upstream rather than on your work. Nothing announces this. The evaluation succeeds.

So development pins and the shipped product resolves live, which is the rule [`../../AGENTS.md`](../../AGENTS.md) states and the test lanes already follow.

## The pinned entry point: `viv-dev`

```console
$ just install-pinned
```

This installs everything `just install` does, and adds a `viv-dev` beside it. `viv-dev` sets the three baseline variables to this repository — the two upstream inputs as local store paths, this tree as itself — and then executes the installed `viv`. Use it from any project directory:

```console
$ cd ~/src/some-project
$ viv-dev start
```

It is a separate name rather than a replacement for `viv`, so there is never a question about which of the two an observation came from.

The store paths are recomputed at each invocation rather than baked in at install time, because they move whenever [`../../nix/flake.lock`](../../nix/flake.lock) does. Only the repository path is fixed, so moving the repository means running `just install-pinned` again. The pin itself is computed by [`../../scripts/baseline-pins`](../../scripts/baseline-pins), which you can also read directly:

```console
$ scripts/baseline-pins
export VIVARIUM_BASELINE_NIXPKGS=path:/nix/store/…-source
export VIVARIUM_BASELINE_MICROVM=path:/nix/store/…-source
export VIVARIUM_BASELINE_VIVARIUM=path:/…/vivarium?dir=nix
```

It fails open: on a host that cannot produce the pins it prints the reason on standard error, emits nothing, and exits `0`, so the caller resolves live exactly as a user would. That is deliberate, and it means an empty output is a state to notice rather than an error to debug.

## The same pins, ambient in this repository

[`../../.envrc`](../../.envrc) evaluates the same script, so a shell entered here carries the three variables whether or not you went through `viv-dev`. That covers what neither entry point above does: an installed `viv` run from this directory, a hand-run `nix build`, a lane invoked outside its usual harness.

It lives in `.envrc` and not in the untracked `.envrc.local` for two reasons. The store paths move whenever [`../../nix/flake.lock`](../../nix/flake.lock) does, so a literal copy would go stale without announcing it, and `.envrc.local` is restricted to plain `export` lines because [`../../tests/host/disk-preflight`](../../tests/host/disk-preflight) sources it directly, with no direnv to evaluate anything. `.envrc.local` keeps its one job, which is the drive a disk-heavy run should absorb.

## Removing it

```console
$ just uninstall
```

Removes `viv`, `vivarium-supervisor`, and `viv-dev`. The development shell's `viv` is unaffected — it is not installed anywhere, so there is nothing to remove.

## What this does not touch

The root [`../../flake.nix`](../../flake.nix) is the development environment and declares no build outputs; the shim is a shell package inside `devShells`, not a `packages` attribute. [`../../scripts/check-flake-boundary`](../../scripts/check-flake-boundary) enforces that distinction against the flake's evaluated attribute names, so run it after any change here.
