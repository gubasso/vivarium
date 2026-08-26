# Instances

Record how a user finds every sandbox that is running right now, including ones started from directories they have forgotten.[^read]

1. Start sandboxes for three different projects and leave them running.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed, what is missing, and what would stop them all.

## vivarium

Yes: `viv status -g` runs from any directory and reports a row per sandbox — its state, its uptime, the agent's own count of attached sessions, and its measured memory beside the declared ceiling — with the host's own reading beside the fleet, so a forgotten VM is found with the evidence that it is running rather than merely defined. The third step has its own verb: `viv stop --all` walks the same stop ladder over every running sandbox, from anywhere.

```bash
cd ~/projects/a && viv start
cd ~/projects/b && viv start
viv status              # this project
viv status -g           # every sandbox on the machine, with state and sessions
viv stop --all          # stop them all, from anywhere
```

## flake-pilot

No at both routes: `flake-ctl list` reports registrations rather than instances, so the tool's own listing answers a different question at either engine. Firecracker instances are processes with TAP devices that nothing enumerates. Podman instances, which is what the `krun` route creates, live in a separate storage root that needs `CONTAINERS_STORAGE_CONF` to be visible at all — so even the engine's own `podman ps` does not find them by default.

## glaipnir

Yes: instances are podman containers under the user's ordinary storage, so `status` and podman's own listing both find them, and the numbered-sibling naming makes a forgotten one legible rather than anonymous.

## bunkerbox

No: the CLI has no enumeration verb and no way to stop something it did not start in this terminal. Containers are visible where containerd keeps them, through `sudo ctr containers ls`, which is the engine answering rather than the tool — and the names there are bunkerbox's own, so the information exists and the surface does not.

[^read]: Read at `vivarium` `e1b3c73` on 2026-08-26; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
