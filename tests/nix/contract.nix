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
  VIVARIUM_STORE_MIN_FREE = toString expect.storeMinFree;
  VIVARIUM_STORE_MAX_FREE = toString expect.storeMaxFree;
  VIVARIUM_STORE_VOLUME_SIZE_MIB = toString expect.storeVolumeSizeMiB;
  VIVARIUM_VIRTIOFSD_THREAD_POOL_SIZE = toString expect.virtiofsdThreadPoolSize;
  VIVARIUM_EXPECTED_UNITS = pkgs.lib.concatMapStrings (u: "${u} ") (
    pkgs.lib.sort (a: b: a < b) ([ "vivarium-volume-prepare.service" ] ++ expect.units)
  );
  VIVARIUM_EXPECT_NO_UNITS = if expect.units == [ ] then "1" else "";
} (builtins.readFile ./contract.sh)
