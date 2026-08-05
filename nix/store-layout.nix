# The guest store's physical layout, shared by the base image and by every
# measurement leg that has to reach into it.
#
# A plain imported attrset rather than a NixOS options module, deliberately:
# these are ten in-repo constants with one consumer tree, and declaring them as
# `mkOption`s would create an options surface the project would then have to
# keep or deprecate. Where `config` already owns a value, read it from `config`
# instead of from here — `upperRoot` is `config.microvm.writableStoreOverlay`,
# which is the predicate `launch-arguments.nix` already uses to identify the
# store volume.
{ pkgs, lib }:

rec {
  # ADR-0087/ADR-0088. The layout mirrors the public prior art running this same
  # topology (shazow/agentspace), with one deliberate divergence recorded in
  # ADR-0088: that module forces `microvm.writableStoreOverlay` to null and
  # hand-writes `fileSystems."/nix/store"`. Here the option stays set, so
  # upstream's mounts.nix generates the identical overlay, marks the store volume
  # `neededForBoot` on its own, and leaves the `What=overlay` shutdown drop-in
  # uncontested — which forcing the option null would silently replace with
  # `What=store`.
  lowerStoreDir = "/nix/.ro-store";
  # LocalStore creates `<real>/.links` unconditionally, read-only or not, and the
  # lower layer is a read-only share. Give the metadata reader a private writable
  # view instead of making the immutable lowerdir writable.
  lowerStoreViewDir = "/nix/.local-overlay-lower-store";
  # The lower store's *state* is the tmpfs root's own `/nix/var/nix`, which
  # microvm.nix's `registerClosure` fills from `regInfo` in `boot.postBootCommands`
  # — stage 2, before systemd — so it describes exactly the boot closure and is
  # rebuilt from scratch every boot. The upper store's state is on the volume and
  # is the half that persists. That split is what makes upstream's warning about
  # `registerClosure` and a persistent overlay not apply.
  lowerStateDir = "/nix/var/nix";
  upperRoot = "/nix/.rw-store";
  upperLayer = "${upperRoot}/store";
  upperStateDir = "${upperRoot}/state";
  lowerStoreUri = "local?real=${lowerStoreViewDir}&state=${lowerStateDir}&read-only=true";
  remountHook = pkgs.writeShellScript "vivarium-local-overlay-remount" ''
    set -eu
    # util-linux's new mount API cannot remount overlayfs (#2528, #2576), and
    # upstream Nix's own test harness sets this globally for the same reason.
    # Load-bearing here, not vestigial: overlayfs runs in the *guest*, whose
    # kernel is below the 6.19 at which upstream stops reproducing the stale
    # handle — an observation upstream records without accounting for it.
    export LIBMOUNT_FORCE_MOUNT2=always
    exec ${pkgs.util-linux}/bin/mount -o remount "$1"
  '';
  storeUri =
    "local-overlay://?lower-store=${lib.escapeURL lowerStoreUri}"
    + "&upper-layer=${lib.escapeURL upperLayer}"
    + "&state=${lib.escapeURL upperStateDir}"
    + "&remount-hook=${lib.escapeURL (toString remountHook)}"
    # The initrd assembles the overlay below /sysroot; after switch-root the mount
    # is correct but /proc/self/mounts still records the initrd lowerdir prefix,
    # which the check string-matches. ADR-0088 replaces it with a vivarium-side
    # assertion — the measurement image's store-spike leg reads the merged mount itself.
    + "&check-mount=false";
  # An EnvironmentFile, never `Environment=`: systemd reads percent escapes in a
  # unit-file value as unit specifiers, and this URI is percent-encoded throughout.
  nixDaemonEnvironment = pkgs.writeText "vivarium-local-overlay-environment" ''
    NIX_REMOTE=${storeUri}
  '';
}
