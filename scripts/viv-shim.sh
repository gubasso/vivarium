# shellcheck shell=bash
# The `viv` the development shell puts on PATH. Loaded by the root flake into a
# `writeShellApplication`; not a standalone program, which is why it has no
# shebang and no `set` line of its own.
#
# It serves the working tree, never a store snapshot. Listing a built package in
# the shell instead would pin the command to whatever the flake evaluated at
# shell entry, so it would silently lag every edit made since and a hand-run
# check would then report behaviour the tree no longer has.
#
# Inside this repository it shadows an installed `viv`, which is the intent.

root="$(dirname "$(cargo locate-project --workspace --message-format plain)")"
cargo build --quiet --manifest-path "$root/Cargo.toml" --bin viv

# Pinned, because a `viv` run from this tree that built its guest from the
# published branch would be measuring something other than the change under
# test. `scripts/baseline-pins` owns the reasoning and fails open.
eval "$("$root/scripts/baseline-pins")"

# `exec`, so no process sits between the caller and the binary. `viv shell` puts
# the local terminal in raw mode and hands it to a guest shell, so a supervising
# parent — which is what `cargo run` would leave here — would change the signals,
# streams and exit status an interactive session observes.
exec "${CARGO_TARGET_DIR:-$root/target}/debug/viv" "$@"
