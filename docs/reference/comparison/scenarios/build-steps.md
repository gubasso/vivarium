# Build steps

Record what executes while the environment is being produced, and as whom.[^read]

1. Read the tool's build path for anything executing a user-supplied command.
2. Record whether it runs as root, and whether its effect is recorded anywhere.

## vivarium

Yes, nothing does: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) admits no step that executes a user-supplied command, so there is no root shell in the build to audit. What a user contributes is declaration, evaluated rather than run — the refusal [the setup row](./own-setup-at-build.md#vivarium) reads from the other side, and the reason its verdict there is a partial rather than a yes.

## flake-pilot

No at both routes, and the dagger is the whole point of the cell: flake-pilot runs no build for this row to audit. The entire `flake-ctl` surface is `pull`, `load`, `register`, `show`, `remove`, `init`, and `list`, and no `build` or `commit` call appears in either pilot. Upstream states the delegation rather than leaving it implicit — building a container or VM image "can be done in different ways", with the Open Build Service and KIWI named as one option — and anchors trust at the source instead, fetching an image over https and documenting the variable that relaxes that as safe only where "the connection to the image source can be trusted".

The no is still the honest verdict, because this row asks what produced the artifact a user receives rather than what the tool executes. At the firecracker route the guest is built by KIWI from a description the user maintains, whose `config.sh` is an arbitrary root shell script — upstream's own example VM pipes a vendor installer into `bash` there. The `krun` route is the same shape at a different builder, an OCI image whose `RUN` steps are the image author's and run as root. Either way the commands ran as root, and the registration that results names neither them nor the file they came from — what [the same-definition row](./same-definition.md#flake-pilot) charges separately.

What `†` records is that the guarantee is one flake-pilot has placed outside itself, not one it failed to make: the builder is deliberately the user's choice, so a reader who wants this answer looks at the builder they picked. The credit runs the other way at [the setup row](./own-setup-at-build.md#flake-pilot), where the same `config.sh` is what earns a yes — one artifact, credited once and charged once.

## glaipnir

No: build hooks execute as root during the image build, and the base image installs agents from npm and from vendor scripts piped to `bash`. The generality is the point; the cost is that the image is not reproducible from the declaration alone.

## podman

No: a `RUN` line is a root shell command by construction, and a `Containerfile` is a sequence of them. The effect is recorded — `podman history` keeps the commands — but recorded is not restrained, and what any of them fetched from the network is not part of that record.

[^read]: Read at `glaipnir` `21ef389` on 2026-08-18; `vivarium` `ceb0027`, `flake-pilot` `920f41e`, and `podman` 5.x on 2026-08-20; `flake-pilot` re-read at `920f41e` on 2026-08-22 for the complete `flake-ctl` command surface and for where upstream states who builds an image.
