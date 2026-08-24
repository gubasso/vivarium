# Own setup at start

The companion to [the build-time row](./own-setup-at-build.md). Some setup cannot happen at build: it needs the host as it is now, or the credential that arrived since. Record whether the tool has a place for it, and whether a second such step can be added without editing the first.[^read]

1. Write a setup step that must run each time the sandbox starts, before the user's work does.
2. Attach it by the tool's documented mechanism.
3. Attach a second one, and record what it took to add.

## vivarium

Yes, with no limit worth naming: a piece is a NixOS module, so a systemd service, a timer, or an activation step is declared the way it would be on any NixOS host, and it composes with every other layer through the same merge. A second one is a second module rather than an edit to the first, which is [the composition row](./composition.md#vivarium) paying out. What it is not is a shell hook — the unit is a declaration the module system can see, which is what lets [a collision be reported](./collision.md#vivarium).

Refreshing a credential before the agent runs, as a unit the merge can see — and a second such step is a second piece in the manifest's list, not an edit to this one:

```nix
# pieces/agent-warmup/default.nix
{ ... }:
{
  systemd.services.agent-warmup = {
    wantedBy = [ "multi-user.target" ];
    serviceConfig.Type = "oneshot";
    script = "/run/current-system/sw/bin/refresh-token";
  };
}
```

## flake-pilot

Reachable, nothing arranges it: `sci` runs the one `run=` command and then reboots, so there is exactly one execution slot — but that command is a path inside the image the user built, so a script that does the setup and then execs the application reaches the same place podman's `ENTRYPOINT` does. What is absent is a place to add to: a second step edits the first, or rebuilds. The empty guest is also what loses flake-pilot [a second session](./concurrent-sessions.md#flake-pilot).

The setup goes in a wrapper the user bakes into their own image, and the registration points its one slot at the wrapper instead of at the application:

```bash
# /usr/bin/claude-wrapped, installed by the image description
#!/bin/bash
/usr/bin/refresh-token || exit 1
exec /usr/bin/claude "$@"
```

```bash
flake-ctl firecracker register --vm myvm \
    --app /usr/bin/claude --target /usr/bin/claude-wrapped
```

The `krun` route has the same one slot and reaches it through podman's entry point rather than through `sci`: the registration's `--target` is the path run inside the image, so a wrapper baked into the image and named there does the setup and then execs the application. Same arrangement, same absence of a place to add to.

## glaipnir

Yes: `--run-hook` stages scripts into a mounted directory, and the entrypoint finds every `*.sh` there, sorts them, and runs each one on every start. The ordered `NN-*.sh` convention is what composes them, and each is validated with `shellcheck` before use. The cost is the same as the mechanism: they compose the way shell does, one after another, so nothing can report a disagreement between two of them — which is where [the collision row](./collision.md#glaipnir) reads `n/a`.

Each step is a file, and the number prefix is the whole ordering mechanism:

```bash
# 20-refresh-token.sh — staged with --run-hook, run on every start
refresh-token --quiet
```

## podman

Reachable, nothing arranges it: the start side is one command — `ENTRYPOINT` baked into the image, or `--entrypoint` replacing it for a single run — so setup means writing a script that does the work and then execs the real one. The mechanism is podman's and the arrangement is the user's: there is no directory of start steps to add to, so a second step edits the first, or rebuilds.

The same wrapper shape as flake-pilot's, either baked in as `ENTRYPOINT` or named per run:

```bash
# entrypoint.sh, baked into the image
#!/bin/sh
refresh-token
exec claude "$@"
```

```bash
podman run --runtime krun --rm -it --entrypoint /usr/local/bin/entrypoint.sh dev:latest
```

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `44e3ab2`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-20.
