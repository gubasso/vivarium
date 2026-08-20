# Instances

Record how a user finds every sandbox that is running right now, including ones started from directories they have forgotten.

1. Start sandboxes for three different projects and leave them running.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed, what is missing, and what would stop them all.

## vivarium

vivarium `ceb0027`, 2026-08-18. Specified, not built: `viv status` reports the current project; `viv status -g`, a live-session count, and `viv stop --all` are specified and do not run — the `*`. The machine-wide view is the one thing all three alternatives provide today and vivarium does not.

Per project the surface works today; the machine-wide half is the `*`:

```bash
cd ~/projects/a && viv start
cd ~/projects/b && viv start
viv status              # this project
viv status -g           # every project on the machine - specified, not built
```

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: `flake-ctl list` reports registrations rather than instances. Firecracker instances are processes with TAP devices that nothing enumerates, and podman instances live in a separate storage root that needs `CONTAINERS_STORAGE_CONF` to be visible at all — so even the engine's own listing does not find them by default.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: instances are podman containers under the user's ordinary storage, so `status` and podman's own listing both find them, and the numbered-sibling naming makes a forgotten one legible rather than anonymous.

## podman

podman 5.x, read 2026-08-19. Yes: `podman ps -a` lists everything on the machine and `podman stop -a` stops it, at any runtime, including krun.

Enumeration and mass control are free, and neither needs the runtime named:

```bash
podman ps -a
podman stop -a
```
