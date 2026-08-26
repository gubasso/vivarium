# Rollback

Record whether a build from a past date can be booted again as it was.[^read]

1. Build the environment and record what identifies that build.
2. Change the definition and rebuild.
3. Ask the tool for the earlier environment by that identifier and record what starts.

## vivarium

Yes: every successful build appends a numbered generation to a per-project profile, pinned by a GC root so an ordinary store collection cannot eat it. `viv generations list` names what is retained, `rollback` and `activate` move the `current` pointer, `viv start --generation <n>` boots a retained build without evaluating, and `prune` unlinks exactly what its retention argument selects while refusing the running VM's own generation. What no alternative has is the past kept on purpose: below, the closest thing to an answer is an old artifact left lying around until something collects it.

## flake-pilot

No at both routes.

At the firecracker route, `pull --force` writes the new rootfs and kernel over the old ones in `/var/lib/firecracker/images/<name>/`, and the registration names those paths rather than a version. Nothing keeps the bytes that were there, so the earlier environment stops existing at the moment the new one arrives.

At the `krun` route the earlier bytes usually do survive — a re-pulled tag leaves the previous image on the machine, untagged — but nothing a registration can name reaches them. The registration holds a tag, the old image now answers only to an id nobody wrote down, and re-registering the command against that id is building a second registration rather than booting a previous build. The bytes the engine leaves lying around are exactly what a user at the bare engine would reach for, and they are out of reach here precisely because the registration is the thing that made the tool convenient.

## glaipnir

No: a rebuild moves the agent's tag and glaipnir's own surface — `run`, `status`, `clean` — offers no way to name an earlier build, so there is nothing to ask for. The untagged image the rebuild left behind is reachable through podman directly, which is a different tool answering, not this one.

## bunkerbox

No: nothing keeps a history and nothing asks for an earlier one. What identifies a build is the tag in the runtime config, and `overwrite: true` in an image config replaces the archive that tag was built from. An old archive that happens to survive as a file can be imported again and pointed at by editing the config, which is reinstalling a package rather than a verb the tool offers.

[^read]: Read at `vivarium` `e1b3c73` on 2026-08-26; `flake-pilot` `920f41e` and `glaipnir` `21ef389` on 2026-08-20; `bunkerbox` `b7f14f3` on 2026-08-25.
