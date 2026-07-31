{
  nixpkgs,
  microvm,
  system,
}:

let
  pkgs = import nixpkgs { inherit system; };
  volumeLabel = "vivarium-default";
  workspaceSourceSentinel = "VIVARIUM_LAUNCH_WORKSPACE_SOURCE";
  volumeImageSentinel = "VIVARIUM_LAUNCH_VOLUME_IMAGE";
  guest = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = {
      inherit volumeLabel workspaceSourceSentinel volumeImageSentinel;
    };
    modules = [
      microvm.nixosModules.microvm
      ./guest.nix
    ];
  };
  launchArguments = import ./launch-arguments.nix {
    inherit pkgs volumeLabel;
    inherit (guest) config;
    inherit (nixpkgs) lib;
  };
  runner = import ./runner.nix { inherit pkgs launchArguments; };
  contract = pkgs.runCommand "vivarium-first-microvm-contract" { } ''
    set -eu
    # The volume label is a build-to-launch contract that only a boot test would
    # otherwise catch: the launcher's mkfs applies it, and the guest resolves the
    # volume through /dev/disk/by-label/<label>. Compare the two independently
    # realised artifacts — the launcher's own JSON and the guest's own fstab —
    # rather than two copies of one Nix constant, which cannot disagree.
    launcher_json=$(grep -oE '/nix/store/[a-z0-9]+-vivarium-first-microvm-launch-arguments\.json' \
      ${runner}/bin/vivarium-first-microvm | head -n1)
    test -n "$launcher_json"
    grep -F '"volumeLabel":"${launchArguments.volumeLabel}"' "$launcher_json"
    grep -F '/dev/disk/by-label/${launchArguments.volumeLabel}' ${guest.config.system.build.toplevel}/etc/fstab
    # Per-share virtiofsd policy must reach the launcher from the guest module.
    grep -F '"cache":"always"' "$launcher_json"
    grep -F '"cache":"auto"' "$launcher_json"
    grep -E '^overlay[[:space:]]+/nix/store[[:space:]]+overlay' ${guest.config.system.build.toplevel}/etc/fstab
    # spec/06:22 — the read-only share must be read-only inside the guest too,
    # which upstream's generated `defaults` does not give us.
    ro_store_options=$(awk '$2 == "/nix/.ro-store" { print $4 }' ${guest.config.system.build.toplevel}/etc/fstab)
    for flag in ro nodev nosuid noexec; do
      case ",$ro_store_options," in
        *",$flag,"*) ;;
        *) echo "read-only store mount is missing $flag: $ro_store_options" >&2; exit 1 ;;
      esac
    done
    grep -F 'What=overlay' ${guest.config.system.build.toplevel}/etc/systemd/system/nix-store.mount.d/overrides.conf
    grep -F 'DefaultDependencies=false' ${guest.config.system.build.toplevel}/etc/systemd/system/nix-store.mount.d/overrides.conf
    touch "$out"
  '';
in
{
  inherit
    guest
    launchArguments
    runner
    contract
    ;
}
