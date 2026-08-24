{
  nixpkgs,
  microvm,
  system,
}:

let
  pkgs = import nixpkgs { inherit system; };
  inherit (nixpkgs) lib;

  # The crate in its default Cargo layout at the tree root — read directly, with
  # no snapshot to keep in sync. Two trees satisfy this: the repository, entered
  # as `path:$REPO_ROOT?dir=nix` (the `/nix` form pins the tree root one level
  # too deep and `../` becomes unreachable), and the `vivarium/` subtree the
  # installed binary writes into every generated flake (ADR-0102), which mirrors
  # the repository layout for exactly this reason.
  crateRoot = ../.;

  guestAgentPackage = pkgs.rustPlatform.buildRustPackage {
    pname = "vivarium-guest-agent";
    version = "0.1.0";
    src = lib.cleanSourceWith {
      name = "vivarium-crate-source";
      src = crateRoot;
      # Admits the crate and nothing else. The tree root is the repository or the
      # embedded subtree, so a blanket `type == "directory"` would descend into
      # `docs/` and a Cargo build directory; every admitted path is named instead.
      # `tests/` is deliberately not admitted, so verification cannot affect the
      # product derivation. The agent depends on the `vivarium` library for its
      # protocol types, and that library's `build.rs` embeds `nix/` and the crate
      # source, so everything `build.rs` walks is admitted here — the same set the
      # binary embeds, which is what keeps the two trees one tree.
      filter =
        path: _type:
        let
          rel = lib.removePrefix (toString crateRoot + "/") (toString path);
        in
        rel == "Cargo.toml"
        || rel == "Cargo.lock"
        || rel == "build.rs"
        || rel == "src"
        || lib.hasPrefix "src/" rel
        || rel == "crates"
        || lib.hasPrefix "crates/vivarium-guest-agent" rel
        || rel == "nix"
        || (lib.hasPrefix "nix/" rel && rel != "nix/flake.nix" && rel != "nix/flake.lock");
    };
    cargoLock.lockFile = crateRoot + "/Cargo.lock";
    cargoBuildFlags = [
      "--package"
      "vivarium-guest-agent"
    ];
    doCheck = false;
  };

  imageDefaults = {
    homeVolumeSizeMiB = 32768;
    storeVolumeSizeMiB = 32768;
    # spec/17's per-volume ceiling, for a `[[volumes]]` entry that declares no
    # `size_gib`. Separate from the home volume's on purpose: they are the same
    # number today, and a measurement variant that scales the home would
    # otherwise resize every volume a manifest declared, silently.
    defaultVolumeSizeMiB = 32768;
    storeMinFree = 4294967296; # 4 GiB — spec/17
    storeMaxFree = 8589934592; # 8 GiB — spec/17
    virtiofsdThreadPoolSize = 0; # ADR-0096, measured; was 4 under ADR-0051
    # The composition seam, and the reason this file no longer knows what a
    # measurement leg is: an opaque list of NixOS modules it appends without
    # asking what is in it. Empty is the shipped image.
    extraModules = [ ];
  };

  storeLayout = import ./store-layout.nix { inherit pkgs lib; };
  volumeLabel = "vivarium-default";
  storeVolumeLabel = "vivarium-store";
  # One sentinel for the directory rather than one per volume image, because the
  # number of volumes is a property of the merged configuration and not of the
  # host's argv: `viv start --no-rebuild` evaluates nothing and boots the last
  # build, so the host cannot know which named volumes that build declared. The
  # launcher reads the build's own contract and always can, so it is the half
  # that joins a name onto a directory. The directory itself is launch-channel
  # and stays out of every build output (N19).
  volumeDirSentinel = "VIVARIUM_LAUNCH_VOLUME_DIR";
  # The two volumes that exist without ever being declared (spec/06). Named here
  # rather than in `guest.nix` because they are also the image basenames the host
  # sees under `projects/<id>/<target>/volumes/`, so one spelling has to serve
  # the guest that mounts them and the state layout that holds them.
  homeVolumeName = "default";
  storeVolumeName = "store";
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
  # The guest link's addressing, named once and read by both halves: the guest
  # module configures the NIC and the nameserver from it, and the launcher
  # carries it to the supervisor, which configures the tap and — under allowlist
  # mode — binds the gating resolver on the gateway. The tap name and gateway are
  # the values the Q-005 spike proved inside an unprivileged pair.
  #
  # `dnsForwardAddress` is the address the uplink maps to the host's own
  # resolver, and it sits deliberately outside the guest link's /24: an address
  # inside it would be resolved on the local link, where nothing answers, instead
  # of being routed through the gateway toward the uplink.
  networkLayout = {
    tapName = "viv-tap0";
    gatewayAddress = "10.177.0.1";
    prefixLength = 24;
    guestAddress = "10.177.0.2";
    dnsForwardAddress = "10.177.53.53";
    # Locally administered, stable so the guest's interface match never moves.
    guestMac = "02:56:49:56:41:00";
    resolverPort = 53;
  };

  # The guest module and its build inputs, published so a generated flake can
  # compose the same guest a diagnostic image composes. Slice 012's item 1
  # measured the alternative: a manifest-built guest carries no vivarium shares,
  # no volumes, no store overlay and no `vivarium.credentials` option, so
  # `launch-arguments.nix` throws on the first `shareByTag`. The difference is the
  # input, not the launcher, so the input is what moves.
  guestModule = ./guest.nix;

  # The tool's own option surface, published for the same reason `guestModule` is:
  # a caller that wants to compose against `vivarium.*` must reach the file the
  # generated flake embeds (`src/config/flake.rs` reads this very path), not a
  # second copy. The shipped image does not include it — an image composes no tool
  # options, which is why `guest.nix` reads `config.vivarium.volumes or [ ]`.
  optionsModule = ./vivarium-options.nix;

  # `specialArgs` rather than module arguments because `guest.nix` takes them as
  # function arguments; the shipped values are `imageDefaults`, which is what
  # makes the manifest-built guest and the shipped image the same guest.
  guestSpecialArgs = {
    inherit
      storeLayout
      volumeLabel
      storeVolumeLabel
      volumeDirSentinel
      homeVolumeName
      storeVolumeName
      guestAgentPackage
      networkLayout
      ;
    inherit (imageDefaults)
      homeVolumeSizeMiB
      storeVolumeSizeMiB
      defaultVolumeSizeMiB
      storeMinFree
      storeMaxFree
      ;
  };

  # The launch half, against an already-resolved guest `config`. Both callers go
  # through here — `mkImage` below and the generated flake through
  # `nix/flake.nix`'s `lib` output — so the diagnostic path and the product path
  # cannot drift into two spellings of the same substitution table.
  mkLaunch =
    {
      config,
      virtiofsdThreadPoolSize ? imageDefaults.virtiofsdThreadPoolSize,
    }:
    rec {
      launchArguments = import ./launch-arguments.nix {
        inherit
          pkgs
          config
          # The launcher recovers each volume's tokenized image path by replacing
          # this prefix in the guest's own `image` field, so both halves read one
          # constant rather than agreeing on a spelling.
          volumeDirSentinel
          storeCanaryExpression
          gcInterlockCanaryExpression
          gcInterlockControlExpression
          networkLayout
          # Launch-channel, so it must not reach the guest: this is what keeps the
          # four pool-size variants on one guest closure and makes the sweep four
          # short boots rather than four full rebuilds.
          virtiofsdThreadPoolSize
          ;
        inherit (nixpkgs) lib;
      };
      runner = import ./runner.nix { inherit pkgs launchArguments; };
    };

  mkImage =
    variant:
    let
      # A shallow `//` accepts a typo and yields an image that looks right and is
      # not — the same shape as the failure that already cost a boot, where scaled
      # thresholds never reached the daemon. Three lines buy that back.
      unknownKeys = builtins.attrNames (builtins.removeAttrs variant (builtins.attrNames imageDefaults));
      v =
        lib.throwIf (unknownKeys != [ ])
          "vivarium image: unknown key(s) ${lib.concatStringsSep ", " unknownKeys} (known: ${lib.concatStringsSep ", " (builtins.attrNames imageDefaults)})"
          (imageDefaults // variant);
    in
    rec {
      guest = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = guestSpecialArgs // {
          inherit (v)
            homeVolumeSizeMiB
            storeVolumeSizeMiB
            defaultVolumeSizeMiB
            storeMinFree
            storeMaxFree
            ;
        };
        # The complete, readable statement of what is in this image. A measurement
        # input never travels in `specialArgs`, where it would be in scope for the
        # whole module tree and auditable only by grepping it.
        modules = [
          microvm.nixosModules.microvm
          guestModule
        ]
        ++ v.extraModules;
      };
      inherit
        (mkLaunch {
          inherit (guest) config;
          inherit (v) virtiofsdThreadPoolSize;
        })
        launchArguments
        runner
        ;
      settings = v;
    };

  shipped = mkImage { };
in
{
  inherit
    pkgs
    lib
    storeLayout
    storeCanaryExpression
    volumeLabel
    storeVolumeLabel
    imageDefaults
    networkLayout
    mkImage
    shipped
    guestAgentPackage
    guestModule
    optionsModule
    guestSpecialArgs
    mkLaunch
    ;
}
