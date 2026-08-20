# Build steps

Record what executes while the environment is being produced, and as whom.[^read]

1. Read the tool's build path for anything executing a user-supplied command.
2. Record whether it runs as root, and whether its effect is recorded anywhere.

## vivarium

Yes, nothing does: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) admits no step that executes a user-supplied command, so there is no root shell in the build to audit. What a user contributes is declaration, evaluated rather than run — the refusal [the setup row](./own-setup-at-build.md#vivarium) reads from the other side, and the reason its verdict there is a partial rather than a yes.

## flake-pilot

No: the guest is built by KIWI from a description the user maintains, and its `config.sh` is an arbitrary root shell script — upstream's own example VM pipes a vendor installer into `bash` there. flake-pilot does not run that build, which is exactly the problem this row records: the commands ran as root, and the registration that results names neither them nor the file they came from.

## glaipnir

No: build hooks execute as root during the image build, and the base image installs agents from npm and from vendor scripts piped to `bash`. The generality is the point; the cost is that the image is not reproducible from the declaration alone.

## podman

No: a `RUN` line is a root shell command by construction, and a `Containerfile` is a sequence of them. The effect is recorded — `podman history` keeps the commands — but recorded is not restrained, and what any of them fetched from the network is not part of that record.

[^read]: Read at `glaipnir` `21ef389` on 2026-08-18; `vivarium` `ceb0027`, `flake-pilot` `920f41e`, and `podman` 5.x on 2026-08-20.
