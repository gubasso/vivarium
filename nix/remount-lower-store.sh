# shellcheck shell=bash
set -eu
# util-linux's new mount API cannot remount overlayfs (#2528, #2576), and
# upstream Nix's own test harness sets this globally for the same reason.
# Load-bearing here, not vestigial: overlayfs runs in the guest, whose kernel is
# below the 6.19 at which upstream stops reproducing the stale handle.
export LIBMOUNT_FORCE_MOUNT2=always
exec mount -o remount "$1"
