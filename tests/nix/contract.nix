# The build-to-launch contract, asserted against two *independently realised*
# artifacts — the launcher's own JSON and the guest's own `/etc/fstab` — rather
# than against two copies of one Nix constant, which cannot disagree.
#
# Parameterised by variant (ADR-0095) so every image gets its own contract
# rather than only the shipped one. Every assertion below is about the store
# topology, so every one holds for every variant unchanged; `expect` carries
# only the numbers a variant is allowed to move.
{
  pkgs,
  guest,
  runner,
  volumeLabel,
  storeVolumeLabel,
  workspacesInternalRoot,
  expect,
}:

pkgs.runCommand "vivarium-first-microvm-contract" {
  nativeBuildInputs = [
    pkgs.binutils
    pkgs.findutils
    pkgs.jq
  ];
  VIVARIUM_RUNNER = runner;
  VIVARIUM_GUEST_SYSTEM = guest.config.system.build.toplevel;
  VIVARIUM_VOLUME_LABEL = volumeLabel;
  VIVARIUM_STORE_VOLUME_LABEL = storeVolumeLabel;
  # The product's own constant, so the "not under `/run/vivarium`" assertion
  # below compares a realised product value against a literal rather than a
  # literal against a literal, which could not disagree.
  VIVARIUM_WORKSPACES_INTERNAL_ROOT = workspacesInternalRoot;
  VIVARIUM_STORE_MIN_FREE = toString expect.storeMinFree;
  VIVARIUM_STORE_MAX_FREE = toString expect.storeMaxFree;
  VIVARIUM_STORE_VOLUME_SIZE_MIB = toString expect.storeVolumeSizeMiB;
  VIVARIUM_VIRTIOFSD_THREAD_POOL_SIZE = toString expect.virtiofsdThreadPoolSize;
  VIVARIUM_EXPECTED_UNITS = pkgs.lib.concatMapStrings (u: "${u} ") (
    pkgs.lib.sort (a: b: a < b) (
      [
        "vivarium-agent.service"
        "vivarium-volume-prepare.service"
      ]
      ++ pkgs.lib.optional (expect.declaredWorkspaces != [ ]) "vivarium-workspace.service"
      # The bind unit exists exactly when a mount is declared, so the unit
      # allowlist stays exact for both kinds of image.
      ++ pkgs.lib.optional (expect.declaredMounts != [ ]) "vivarium-mounts.service"
      ++ expect.units
    )
  );
  VIVARIUM_EXPECT_NO_UNITS = if expect.units == [ ] then "1" else "";
  # `<name> <mount> <label> <sizeMiB>` per declared volume, in `microvm.volumes`
  # order — empty for every image that declares none. The launcher's JSON and the
  # guest's fstab are then each compared against this third reading rather than
  # against each other alone, and the row count is what turns the loop below into
  # a total check instead of one that passes over an empty list.
  VIVARIUM_DECLARED_VOLUMES = pkgs.lib.concatMapStrings (
    volume:
    "${pkgs.lib.removeSuffix ".img" (baseNameOf volume.image)} ${volume.mountPoint} ${volume.label} ${toString volume.size}\n"
  ) expect.declaredVolumes;
  # `<tag> <internal mountPoint> <readonly> <source>` per declared mount, in
  # `microvm.shares` order — the source last because it is the one field `read`
  # may take to end of line. Same third-reading role as the volume rows above:
  # the launcher's JSON and the guest's own units are each compared against
  # this, and the row count keeps the loop total (slice 019).
  VIVARIUM_DECLARED_MOUNTS = pkgs.lib.concatMapStrings (
    share: "${share.tag} ${share.mountPoint} ${if share.readOnly then "1" else "0"} ${share.source}\n"
  ) expect.declaredMounts;
  VIVARIUM_DECLARED_WORKSPACES = pkgs.lib.concatMapStrings (
    share: "${share.tag} ${share.mountPoint} ${share.source}\n"
  ) expect.declaredWorkspaces;
} (builtins.readFile ./contract.sh)
