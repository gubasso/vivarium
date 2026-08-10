# The verification root for the microVM.
#
# Everything here names the product; nothing under `nix/` may name anything here
# except the single edge in `nix/flake.nix`, which exists because a flake is a
# publication surface and has to declare `checks`. `scripts/check-verification-boundary`
# keeps the cheap half of that rule and `tests/host/first-microvm-check` keeps the
# real one, by asserting the shipped image's derivation graph carries nothing from
# this tree.
#
# Parameterised by the product's evaluated context rather than by `nixpkgs`, so the
# shipped image, the measurement image, the scaled image and the four bench
# launchers share one nixpkgs, one supervisor derivation and one store layout.
{ product }:

let
  inherit (product) lib;

  # What a verification image may vary on top of the product's own knobs. Kept
  # disjoint from `product.imageDefaults` so a typo lands in exactly one of the two
  # unknown-key throws rather than silently reading as the other side's vocabulary.
  verificationDefaults = {
    # Subset of `tests/nix/measurement`'s known legs. Empty selects no probe at
    # all, which is what the shipped image is.
    legs = [ ];
    # Upstream Nix's own free-space TEST hook, which arms C and D drive. It is
    # seeded above `max-free` so it is inert on an ordinary boot — but an arm that
    # measures a REAL crossing needs it absent, not merely inert, because a daemon
    # reading it never consults `statvfs` at all.
    storeFreeSpaceHook = true;
  };

  # The contract is built against an already-realised image, never against a second
  # construction of one. `units` comes from the leg composition that built the image,
  # so the allowlist and the image cannot disagree about which probes exist.
  contractFor =
    image: units:
    import ./contract.nix {
      inherit (product) pkgs volumeLabel storeVolumeLabel;
      inherit (image) guest runner;
      expect = {
        # Read off the image's own resolved settings. Re-deriving them from the
        # defaults is what `nix/default.nix` used to do to itself, and it is how the
        # leg-to-unit map came to exist twice.
        inherit (image.settings)
          storeMinFree
          storeMaxFree
          storeVolumeSizeMiB
          virtiofsdThreadPoolSize
          ;
        inherit units;
      };
    };

  mkVerification =
    overrides:
    let
      unknownKeys = builtins.attrNames (
        builtins.removeAttrs overrides (
          (builtins.attrNames verificationDefaults) ++ (builtins.attrNames product.imageDefaults)
        )
      );
      settings = verificationDefaults // overrides;
      measurement = import ./measurement {
        inherit lib;
        inherit (product) storeLayout storeCanaryExpression;
        inherit (settings) legs storeFreeSpaceHook;
      };
      productOverrides = builtins.removeAttrs settings [
        "legs"
        "storeFreeSpaceHook"
      ];
      image = product.mkImage (productOverrides // { extraModules = measurement.modules; });
    in
    lib.throwIf (unknownKeys != [ ])
      "vivarium verification: unknown key(s) ${lib.concatStringsSep ", " unknownKeys} (known: ${
        lib.concatStringsSep ", " (
          (builtins.attrNames verificationDefaults) ++ (builtins.attrNames product.imageDefaults)
        )
      })"
      (image // { contract = contractFor image measurement.units; });

  poolSizes = [
    0
    1
    2
    4
  ];

  images = {
    # The shipped image, taken from the product's own thunk rather than rebuilt
    # here. `packages.first-microvm` and `checks.first-microvm` then name ONE guest
    # evaluation, so the check covers the artifact that is actually published; a
    # second `mkImage { }` would make that coverage a coincidence of the defaults,
    # and nothing would notice the day it stopped holding. It selects no probe unit,
    # carries no upstream test hook on its store daemon, and has no way to stop
    # itself — the host stops it over the API socket, the ordinary path `spec/10`
    # specifies (ADR-0095).
    shipped = product.shipped // {
      contract = contractFor product.shipped [ ];
    };

    # Every probe that has ever been booted, in one image, for the lanes that
    # already have recorded results against them.
    measurement = mkVerification {
      legs = [
        "spike"
        "gc-interlock"
        "pressure"
        "diagnostic"
      ];
    };

    # The one image that can answer "does a real collection on real ext4 reclaim
    # what the collector asked for". Only `nix.settings` at build time reaches
    # `LocalStore::autoGC`, so the thresholds have to be here.
    #
    # A 4 GiB volume with a 1 GiB floor and a 1.5 GiB target: the gap is 33% of the
    # target against `autoGC`'s 3% re-arm damper, so silence between collections is
    # unambiguously a broken trigger rather than a damped one.
    #
    # `legs = [ "pressure" ]` is a correctness requirement, not tidiness — the spike
    # leg runs `nix-collect-garbage` unconditionally and would delete the ballast
    # this image exists to measure.
    scaled = mkVerification {
      legs = [ "pressure" ];
      storeVolumeSizeMiB = 4096;
      storeMinFree = 1073741824; # 1 GiB
      storeMaxFree = 1610612736; # 1.5 GiB
      # The whole point of this image: the daemon must read the REAL filesystem.
      # With upstream's test hook installed it reads a seeded tebibyte instead and
      # never collects — measured, and it cost a boot.
      storeFreeSpaceHook = false;
    };

    # The peer-origin checks on both vsock ports reject a guest-local peer, and nothing
    # proves it without a guest that can originate one. `vsock_loopback` is what makes the
    # rejection observable rather than argued, so it is declared here and nowhere in the
    # product: the shipped image must not gain a transport whose only purpose is to attack
    # its own agent. The agent runs unprivileged with an empty capability set and cannot
    # load a module itself, so this has to be loaded at boot rather than on demand.
    #
    # It is an *initrd* module, and that is the load-bearing part rather than a style
    # choice. This guest carries no stage-2 module tree at all — `system.build.modulesTree`
    # realises empty and the only modules that exist anywhere are the shrunk set the initrd
    # closure pulls in. `boot.kernelModules` therefore writes `vsock_loopback` into
    # `modules-load.d` and nothing else: the name is requested at boot and the `.ko` is not
    # on the guest to satisfy it, so the load silently no-ops and the positive control fails
    # with an empty echo. `boot.initrd.kernelModules` is what puts the module in the shrunk
    # tree and loads it, which is both halves of what this check needs.
    agent =
      let
        image = product.mkImage {
          extraModules = [
            ({ pkgs, ... }: {
              vivarium.credentials.agents = [ "ssh" ];
              environment.systemPackages = [ pkgs.socat ];
              boot.initrd.kernelModules = [ "vsock_loopback" ];
            })
          ];
        };
      in
      image // { contract = contractFor image [ ]; };

    # The pool constant, under test. ADR-0051 pinned 4 on mechanism alone; the
    # concurrent sweep these variants run is what moved it to the daemon's own 0
    # (ADR-0096). They stay because the constant is only ever as good as its last
    # measurement, and this is the lane that produces one — it depends on host
    # cores, request mix and blocking operations, none of which reading can settle.
    #
    # The pool size is launch-channel, so these four variants share ONE guest
    # closure and the sweep costs four launcher builds rather than four image
    # builds. `tests/host/share-benchmark-check` asserts that equality before
    # spending the boots; if it ever fails, the premise is dead and the sweep is not
    # comparable.
    bench = builtins.listToAttrs (
      map (n: {
        name = toString n;
        value = mkVerification {
          legs = [ "bench" ];
          virtiofsdThreadPoolSize = n;
        };
      }) poolSizes
    );
  };

  packages = {
    first-microvm = images.shipped.runner;
    first-microvm-measurement = images.measurement.runner;
    first-microvm-scaled = images.scaled.runner;
    first-microvm-agent-check = images.agent.runner;
    first-microvm-bench-threads-0 = images.bench."0".runner;
    first-microvm-bench-threads-1 = images.bench."1".runner;
    first-microvm-bench-threads-2 = images.bench."2".runner;
    first-microvm-bench-threads-4 = images.bench."4".runner;
  };

  # Two checks, not seven. `tests/host/first-microvm-check` runs `nix flake check`
  # routinely and each check is a guest build; the scaled and bench variants assert
  # themselves as pre-boot gates in the lane that spends the boot, which is the same
  # guarantee at the point of use.
  checks = {
    first-microvm = images.shipped.contract;
    first-microvm-measurement = images.measurement.contract;
  };
in
{
  inherit
    poolSizes
    images
    packages
    checks
    ;
}
