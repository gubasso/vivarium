# Instances

Record how a user finds every sandbox that is running right now, including ones started from directories they have forgotten.[^read]

1. Start sandboxes for three different projects and leave them running.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed, what is missing, and what would stop them all.

## vivarium

Specified, not built: `viv status` reports the current project; `viv status -g`, a live-session count, and `viv stop --all` are specified and do not run — the `*`. The machine-wide view is the one thing all three alternatives provide today and vivarium does not.

Per project the surface works today; the machine-wide half is the `*`:

```bash
cd ~/projects/a && viv start
cd ~/projects/b && viv start
viv status              # this project
viv status -g           # every project on the machine - specified, not built
```

## flake-pilot

No at both routes: `flake-ctl list` reports registrations rather than instances, so the tool's own listing answers a different question at either engine. Firecracker instances are processes with TAP devices that nothing enumerates. Podman instances, which is what the `krun` route creates, live in a separate storage root that needs `CONTAINERS_STORAGE_CONF` to be visible at all — so even the engine's own `podman ps` does not find them by default.

## glaipnir

Yes: instances are podman containers under the user's ordinary storage, so `status` and podman's own listing both find them, and the numbered-sibling naming makes a forgotten one legible rather than anonymous.

## bunkerbox

No: the CLI has no enumeration verb and no way to stop something it did not start in this terminal. Containers are visible where containerd keeps them, through `sudo ctr containers ls`, which is the engine answering rather than the tool — and the names there are bunkerbox's own, so the information exists and the surface does not.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
