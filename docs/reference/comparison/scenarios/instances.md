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

No at both routes: `flake-ctl list` reports registrations rather than instances, so the tool's own listing answers a different question at either engine. Firecracker instances are processes with TAP devices that nothing enumerates. Podman instances, which is what the `krun` route creates, live in a separate storage root that needs `CONTAINERS_STORAGE_CONF` to be visible at all — so even the engine's own `podman ps`, which [answers this row for podman](#podman), does not find them by default.

## glaipnir

Yes: instances are podman containers under the user's ordinary storage, so `status` and podman's own listing both find them, and the numbered-sibling naming makes a forgotten one legible rather than anonymous.

## podman

Yes: `podman ps -a` lists everything on the machine and `podman stop -a` stops it, at any runtime, including krun.

Enumeration and mass control are free, and neither needs the runtime named:

```bash
podman ps -a
podman stop -a
```

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
