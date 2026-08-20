# Build steps

Record what executes while the environment is being produced, and as whom.[^read]

1. Read the tool's build path for anything executing a user-supplied command.
2. Record whether it runs as root, and whether its effect is recorded anywhere.

## glaipnir

No: build hooks execute as root during the image build, and the base image installs agents from npm and from vendor scripts piped to `bash`. The generality is the point; the cost is that the image is not reproducible from the declaration alone.

[^read]: Read at `glaipnir` `21ef389` on 2026-08-18.
