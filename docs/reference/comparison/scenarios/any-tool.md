# Any tool

Record whether the sandbox is general, or arrives knowing a fixed set of applications.[^read]

1. Choose a program the tool's documentation never mentions.
2. Run it inside by the tool's ordinary mechanism.
3. Record whether anything had to be taught its name, and where.

## vivarium

Yes: vivarium is application-agnostic and classifies nothing inside as trusted or untrusted. The manifest names mounts and the layers it composes, and a package is named in a layer rather than in the manifest at all ([the packages row](./programs-installed.md#vivarium)); nothing anywhere names an application the tool holds an opinion about. That generality is the same position that loses vivarium the [credential-scoping row](./per-tool-credentials.md#vivarium), where knowing which directory belongs to which application is exactly what would be needed, and it is logged as `Q-031` in [`open-questions.md`](../../../plan/open-questions.md).

## flake-pilot

Yes: a registration is a command name and an image, and nothing constrains which command. The pilot reads `argv[0]` from a symlink, so an unheard-of tool is one more registration. The image half is open the same way — at the firecracker boundary the user builds the VM, working from a description [upstream ships as an example](./guest-os.md#flake-pilot) — so nothing has to be taught a name inside flake-pilot. What the generality costs is [the packages row](./programs-installed.md#flake-pilot): the name is taught to a KIWI image description instead, and the registration records neither that file nor the build.

## glaipnir

No: the roster is five trusted agents and one untrusted one, hardcoded, with per-agent images, per-agent credential directories, and per-agent authentication predicates. A tool outside the roster has no image, no mounts, and no entry in `_bind_agent_mounts`, so running it means editing `glaipnir.sh`. This is the cost side of the same built-in opinion that wins glaipnir [the credential-scoping row](./per-tool-credentials.md#glaipnir) — the two rows are one design decision, read from its two ends.

## podman

Yes: podman runs an image and an image runs anything. It knows nothing about applications, which is why it neither helps nor hinders here.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19; `flake-pilot` `920f41e` on 2026-08-20.
