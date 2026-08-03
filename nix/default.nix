{
  nixpkgs,
  microvm,
  system,
}:

let
  pkgs = import nixpkgs { inherit system; };
  volumeLabel = "vivarium-default";
  storeVolumeLabel = "vivarium-store";
  workspaceSourceSentinel = "VIVARIUM_LAUNCH_WORKSPACE_SOURCE";
  volumeImageSentinel = "VIVARIUM_LAUNCH_VOLUME_IMAGE";
  storeVolumeImageSentinel = "VIVARIUM_LAUNCH_STORE_VOLUME_IMAGE";
  # The persistence spike's canary, and the one artifact that must exist on
  # *both* sides of the share: the host realises it before booting, so its bytes
  # sit in the lower layer, while the guest reaches it only through a text file
  # in its closure — never through the output path — so the boot-closure
  # registration does not know it. Building it in the guest is therefore the
  # ordinary write path over a path that already exists below, which is the
  # `delete-duplicate` material ADR-0088's remount hook exists for. A
  # self-contained expression rather than a nixpkgs call, because the guest
  # evaluates it with no channels and no flake.
  #
  # `builtins.storePath` rather than an interpolated string: a path *read from a
  # file* carries no string context, so the builder would not be an input of the
  # derivation and the sandbox would refuse to bind it. `storePath` re-attaches
  # the context on both sides, which also makes the two derivations byte-identical
  # instead of merely similar. It is forbidden under flake pure evaluation, so
  # this file is never `import`ed here — the harness realises it host-side with
  # `nix-build` on the very path the guest uses, which is the whole point.
  storeCanaryExpression = pkgs.writeText "vivarium-store-canary.nix" ''
    let
      bash = builtins.storePath "${pkgs.bash}";
    in
    derivation {
      name = "vivarium-store-canary";
      system = "${system}";
      builder = "''${bash}/bin/bash";
      args = [ "-c" "echo vivarium-store-canary > \$out" ];
    }
  '';
  guest = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = {
      inherit
        volumeLabel
        storeVolumeLabel
        workspaceSourceSentinel
        volumeImageSentinel
        storeVolumeImageSentinel
        storeCanaryExpression
        ;
    };
    modules = [
      microvm.nixosModules.microvm
      ./guest.nix
    ];
  };
  launchArguments = import ./launch-arguments.nix {
    inherit pkgs storeCanaryExpression;
    inherit (guest) config;
    inherit (nixpkgs) lib;
  };
  runner = import ./runner.nix { inherit pkgs launchArguments; };
  contract = pkgs.runCommand "vivarium-first-microvm-contract" { nativeBuildInputs = [ pkgs.jq ]; } ''
    set -eu
    # The volume label is a build-to-launch contract that only a boot test would
    # otherwise catch: the launcher's mkfs applies it, and the guest resolves the
    # volume through /dev/disk/by-label/<label>. Compare the two independently
    # realised artifacts — the launcher's own JSON and the guest's own fstab —
    # rather than two copies of one Nix constant, which cannot disagree.
    launcher_json=$(grep -oE '/nix/store/[a-z0-9]+-vivarium-first-microvm-launch-arguments\.json' \
      ${runner}/bin/vivarium-first-microvm | head -n1)
    test -n "$launcher_json"
    fstab=${guest.config.system.build.toplevel}/etc/fstab
    for label in ${volumeLabel} ${storeVolumeLabel}; do
      grep -F "\"label\":\"$label\"" "$launcher_json"
      grep -F "/dev/disk/by-label/$label" "$fstab"
    done
    # Order is a contract between cloud-hypervisor's `--disk` sequence and
    # microvm.nix's drive-letter assignment, so assert the sequence itself rather
    # than only that both rows exist.
    test "$(jq -r '[.volumeLaunch[].label] | join(",")' "$launcher_json")" = '${volumeLabel},${storeVolumeLabel}'
    # ADR-0091: exactly one volume is provisioned for inodes, and it is the one
    # whose mount point is the writable store overlay.
    test "$(jq -r '[.volumeLaunch[] | select(.inodeRatio != null) | .argName] | join(",")' "$launcher_json")" = store-volume
    test "$(jq -r '.volumeLaunch[] | select(.argName == "store-volume") | .inodeRatio' "$launcher_json")" = 8192
    # ADR-0087: the store volume backs the writable layer, so the guest must
    # resolve it at ${storeVolumeLabel} and mount it before /nix/store exists.
    awk '$2 == "/nix/.rw-store"' "$fstab" | grep -qF '/dev/disk/by-label/${storeVolumeLabel}'
    # ADR-0088's interposed overlay, without which the read-only lower store
    # cannot create the .links directory LocalStore makes unconditionally.
    grep -E '^overlay[[:space:]]+/nix/\.local-overlay-lower-store[[:space:]]+overlay' "$fstab"
    # The daemon must be pointed at a local-overlay store through an
    # EnvironmentFile — never Environment=, where systemd would read the URI's
    # percent-encoding as unit specifiers.
    # nix ships its own nix-daemon.service, so NixOS renders every override into
    # a drop-in beside it rather than into the unit; read both.
    daemon_units=$(echo ${guest.config.system.build.toplevel}/etc/systemd/system/nix-daemon.service \
      ${guest.config.system.build.toplevel}/etc/systemd/system/nix-daemon.service.d/*.conf)
    environment_file=$(cat $daemon_units | sed -n 's/^EnvironmentFile=//p' | head -n1)
    test -n "$environment_file"
    grep -qF 'NIX_REMOTE=local-overlay://' "$environment_file"
    ! cat $daemon_units | grep -qE '^Environment=.*NIX_REMOTE'
    # Both flags: the lower store's read-only=true parameter sits behind the
    # second one, and enabling only the first fails at daemon start.
    for feature in local-overlay-store read-only-local-store; do
      grep -qE "^experimental-features = .*\b$feature\b" ${guest.config.system.build.toplevel}/etc/nix/nix.conf
    done
    # Per-share virtiofsd policy must reach the launcher from the guest module.
    grep -F '"cache":"always"' "$launcher_json"
    grep -F '"cache":"auto"' "$launcher_json"
    grep -E '^overlay[[:space:]]+/nix/store[[:space:]]+overlay' "$fstab"
    # spec/06:22 — the read-only share must be read-only inside the guest too,
    # which upstream's generated `defaults` does not give us.
    ro_store_options=$(awk '$2 == "/nix/.ro-store" { print $4 }' "$fstab")
    for flag in ro nodev nosuid noexec; do
      case ",$ro_store_options," in
        *",$flag,"*) ;;
        *) echo "read-only store mount is missing $flag: $ro_store_options" >&2; exit 1 ;;
      esac
    done
    # The regression guard for ADR-0088's one divergence from the prior art:
    # forcing `writableStoreOverlay` to null — as the vendored module does —
    # makes upstream emit its own `What=store` drop-in in place of this one, and
    # this line is what would catch it.
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
