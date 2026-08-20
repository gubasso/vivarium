# Rollback

Record whether a build from a past date can be booted again as it was.[^read]

1. Build the environment and record what identifies that build.
2. Change the definition and rebuild.
3. Ask the tool for the earlier environment by that identifier and record what starts.

## vivarium

Specified, not built: `spec/11` fixes per-project generations, each pinned by a GC root, with `viv generations list`/`activate`/`rollback`/`prune` and `viv start --generation <n>`. None of it runs yet — the `*`. What no alternative has is the same thing kept on purpose: below, the one that answers does it by leaving the old artifact lying around until something collects it.

## flake-pilot

No: `pull --force` writes the new rootfs and kernel over the old ones in `/var/lib/firecracker/images/<name>/`, and the registration names those paths rather than a version. Nothing keeps the bytes that were there, so the earlier environment stops existing at the moment the new one arrives.

## glaipnir

No: a rebuild moves the agent's tag and glaipnir's own surface — `run`, `status`, `clean` — offers no way to name an earlier build, so there is nothing to ask for. The untagged image the rebuild left behind is reachable through podman directly, which is a different tool answering, not this one.

## podman

Reachable, nothing arranges it: rebuilding a tag does not delete what it pointed at, so the previous image is still there, untagged, and `podman run <image-id>` boots it exactly as it was. Two things keep it an arrangement rather than an answer. The identifier is one the user had to write down before the rebuild, because afterwards the old image shows as `<none>` among every other dangling layer; and `podman image prune`, the ordinary way to reclaim space, is also the way the history is destroyed.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `flake-pilot` `920f41e`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-20.
