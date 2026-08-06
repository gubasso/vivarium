{
  nixpkgs,
  microvm,
  system,
  # The one seam a measurement image needs (ADR-0095). Defaults reproduce the
  # shipped image exactly at every value; `legs = [ ]` is what makes the shipped
  # image probe-free, which is a *change* from what this file used to build.
  variant ? { },
}:

let
  pkgs = import nixpkgs { inherit system; };
  inherit (nixpkgs) lib;

  variantDefaults = {
    # Subset of nix/measurement's known legs. Empty is the shipped image.
    legs = [ ];
    homeVolumeSizeMiB = 32768;
    storeVolumeSizeMiB = 32768;
    storeMinFree = 4294967296; # 4 GiB — spec/17
    storeMaxFree = 8589934592; # 8 GiB — spec/17
    # Upstream Nix's own free-space TEST hook, which arms C and D drive. It is
    # seeded above `max-free` so it is inert on an ordinary boot — but an arm
    # that measures a REAL crossing needs it absent, not merely inert, because a
    # daemon reading it never consults `statvfs` at all.
    storeFreeSpaceHook = true;
    virtiofsdThreadPoolSize = 0; # ADR-0096, measured; was 4 under ADR-0051
  };

  # A shallow `//` accepts a typo and yields an image that looks right and is
  # not — the same shape as the failure that already cost a boot, where scaled
  # thresholds never reached the daemon. Three lines buy that back.
  unknownKeys = builtins.attrNames (
    builtins.removeAttrs variant (builtins.attrNames variantDefaults)
  );
  v =
    lib.throwIf (unknownKeys != [ ])
      "vivarium variant: unknown key(s) ${lib.concatStringsSep ", " unknownKeys} (known: ${lib.concatStringsSep ", " (builtins.attrNames variantDefaults)})"
      (variantDefaults // variant);

  storeLayout = import ./store-layout.nix { inherit pkgs lib; };
  measurementModules = import ./measurement {
    inherit lib storeLayout storeCanaryExpression;
    inherit (v) legs storeFreeSpaceHook;
  };
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
  # ADR-0085's measurement material, and deliberately *not* the canary above: the
  # store spike builds and deletes that one inside the guest, so sharing it would
  # make the two experiments each other's confound. Same conventions as above —
  # `builtins.storePath` so the builder is a real input, a self-contained
  # `derivation` because the guest has no channels, never `import`ed here.
  #
  # The output is a *directory* with two files, and the two files sit on separate
  # inodes on purpose. virtiofsd runs with `--inode-file-handles=never`, so it
  # holds an open descriptor per inode it knows: an fd held on `payload` pins
  # that inode against the host's unlink, which would mask the very behaviour the
  # experiment is after. `sibling` is the inode nothing pins, so it is the one a
  # forced FORGET can actually make the guest look up again.
  #
  # Nothing in the output references another store path. A referrer would make
  # the path undeletable, and deletability is the one property the whole
  # experiment requires.
  gcInterlockExpression =
    name: filler:
    pkgs.writeText "vivarium-${name}.nix" ''
      let
        bash = builtins.storePath "${pkgs.bash}";
        coreutils = builtins.storePath "${pkgs.coreutils}";
      in
      derivation {
        name = "vivarium-${name}";
        system = "${system}";
        builder = "''${bash}/bin/bash";
        args = [
          "-c"
          "export PATH=''${coreutils}/bin; mkdir -p \$out; yes '${filler}' | head -c 1048576 > \$out/payload; printf '%s-sibling\n' '${filler}' > \$out/sibling"
        ];
      }
    '';
  # The path the guest reads before the host collects it.
  gcInterlockCanaryExpression = gcInterlockExpression "gc-interlock-canary" "vivarium-gc-interlock-target";
  # The control: the guest never touches it, and the host deletes it in the same
  # step. A guest that still sees *this* proves the deletion did not propagate at
  # all, which makes the run inconclusive rather than a pass.
  gcInterlockControlExpression = gcInterlockExpression "gc-interlock-control" "vivarium-gc-interlock-control";
  guest = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = {
      inherit
        storeLayout
        volumeLabel
        storeVolumeLabel
        workspaceSourceSentinel
        volumeImageSentinel
        storeVolumeImageSentinel
        ;
      inherit (v)
        homeVolumeSizeMiB
        storeVolumeSizeMiB
        storeMinFree
        storeMaxFree
        ;
    };
    # The complete, readable statement of what is in this image. A measurement
    # input never travels in `specialArgs`, where it would be in scope for the
    # whole module tree and auditable only by grepping it.
    modules = [
      microvm.nixosModules.microvm
      ./guest.nix
    ]
    ++ measurementModules;
  };
  launchArguments = import ./launch-arguments.nix {
    inherit
      pkgs
      storeCanaryExpression
      gcInterlockCanaryExpression
      gcInterlockControlExpression
      ;
    # Launch-channel, so it must not reach the guest: this is what keeps the four
    # pool-size variants on one guest closure and makes the sweep four short
    # boots rather than four full rebuilds.
    inherit (v) virtiofsdThreadPoolSize;
    inherit (guest) config;
    inherit (nixpkgs) lib;
  };
  runner = import ./runner.nix { inherit pkgs launchArguments; };
  contract = import ./contract.nix {
    inherit
      pkgs
      guest
      runner
      volumeLabel
      storeVolumeLabel
      ;
    expect = {
      inherit (v)
        storeMinFree
        storeMaxFree
        storeVolumeSizeMiB
        virtiofsdThreadPoolSize
        ;
      units = map (l: "vivarium-${l}.service") (
        lib.optionals (v.legs != [ ]) (
          lib.filter (u: u != null) (
            map (
              l:
              {
                spike = "store-spike";
                gc-interlock = "gc-interlock";
                pressure = "store-pressure";
                bench = "share-benchmark";
                diagnostic = "first-microvm-diagnostic";
              }
              .${l} or null
            ) v.legs
          )
          ++ [ "measurement-stop" ]
        )
      );
    };
  };
in
{
  inherit
    guest
    launchArguments
    runner
    contract
    ;
}
