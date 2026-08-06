{
  description = "vivarium's first microVM: the buildable guest, its launcher, and their contract";

  # Deliberately a flake of its own, separate from the repository root's.
  #
  # The root `flake.nix` sets up the *development environment* for working on
  # vivarium — the Rust toolchain, the pre-commit hook runtimes, the devShell
  # direnv activates. It is not part of what vivarium builds, and it must not
  # acquire product inputs or product outputs.
  #
  # This tree is the other thing: the microVM the launch path boots. It needs
  # `microvm.nix` and a nixpkgs that pins a guest kernel, a Nix, a hypervisor
  # and a filesystem daemon — none of which the dev shell has any business
  # depending on. Keeping the two locks apart is also correct rather than merely
  # tidy: a backend pin moves on its own clock (ADR-0078), and coupling it to
  # whatever nixpkgs the Rust toolchain wants would drag one by the other.
  #
  # These outputs exist as a *flake* rather than as bare `nix-build` targets for
  # one reason worth stating, because it looks like ceremony and is not:
  # `scripts/first-microvm-check`'s three strongest evaluation-tier checks
  # evaluate `packages.<system>.<attr>.drvPath` and subject it to environment,
  # cwd and location metamorphism. An `--impure --expr` construction cannot be
  # subjected to environment metamorphism by construction, so moving off flake
  # attributes would silently delete the purity evidence.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    # `...` is required: Nix always applies `outputs (inputs // { self = …; })`,
    # so a closed attrset breaks the flake the moment `self` is unused and gets
    # dropped (deadnix flags it). Keep it even when no extra arg is consumed.
    {
      nixpkgs,
      flake-utils,
      microvm,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      # Linux-only: every one of these outputs boots a guest kernel behind KVM.
      nixpkgs.lib.optionalAttrs nixpkgs.legacyPackages.${system}.stdenv.isLinux (
        let
          mkVariant =
            variant:
            import ./. {
              inherit
                nixpkgs
                microvm
                system
                variant
                ;
            };

          # The shipped image. No probe unit, no upstream test hook on its store
          # daemon, and no way to stop itself — the host stops it over the API
          # socket, which is the ordinary path `spec/10` specifies (ADR-0095).
          base = mkVariant { };

          # Every probe that has ever been booted, in one image, for the lanes
          # that already have recorded results against them.
          measurement = mkVariant {
            legs = [
              "spike"
              "gc-interlock"
              "pressure"
              "diagnostic"
            ];
          };

          # The one image that can answer "does a real collection on real ext4
          # reclaim what the collector asked for". Only `nix.settings` at build
          # time reaches `LocalStore::autoGC`, so the thresholds have to be here.
          #
          # A 4 GiB volume with a 1 GiB floor and a 1.5 GiB target: the gap is 33%
          # of the target against `autoGC`'s 3% re-arm damper, so silence between
          # collections is unambiguously a broken trigger rather than a damped one.
          #
          # `legs = [ "pressure" ]` is a correctness requirement, not tidiness —
          # the spike leg runs `nix-collect-garbage` unconditionally and would
          # delete the ballast this image exists to measure.
          scaled = mkVariant {
            legs = [ "pressure" ];
            storeVolumeSizeMiB = 4096;
            storeMinFree = 1073741824; # 1 GiB
            storeMaxFree = 1610612736; # 1.5 GiB
            # The whole point of this image: the daemon must read the REAL
            # filesystem. With upstream's test hook installed it reads a seeded
            # tebibyte instead and never collects — measured, and it cost a boot.
            storeFreeSpaceHook = false;
          };

          # The pool constant, under test. ADR-0051 pinned 4 on mechanism alone;
          # the concurrent sweep these variants run is what moved it to the
          # daemon's own 0 (ADR-0096). They stay because the constant is only
          # ever as good as its last measurement, and this is the lane that
          # produces one — it depends on host cores, request mix and blocking
          # operations, none of which reading can settle.
          #
          # The pool size is launch-channel, so these four variants share ONE
          # guest closure and the sweep costs four launcher builds rather than
          # four image builds. `scripts/share-benchmark-check` asserts that
          # equality before spending the boots; if it ever fails, the premise is
          # dead and the sweep is not comparable.
          bench =
            n:
            mkVariant {
              legs = [ "bench" ];
              virtiofsdThreadPoolSize = n;
            };
        in
        {
          packages = {
            first-microvm = base.runner;
            first-microvm-measurement = measurement.runner;
            first-microvm-scaled = scaled.runner;
            first-microvm-bench-threads-0 = (bench 0).runner;
            first-microvm-bench-threads-1 = (bench 1).runner;
            first-microvm-bench-threads-2 = (bench 2).runner;
            first-microvm-bench-threads-4 = (bench 4).runner;
          };

          # Two checks, not five. `scripts/first-microvm-check` runs `nix flake
          # check` routinely and each check is a guest build; the scaled and
          # bench variants assert themselves as pre-boot gates in the lane that
          # spends the boot, which is the same guarantee at the point of use.
          checks = {
            first-microvm = base.contract;
            first-microvm-measurement = measurement.contract;
          };
        }
      )
    );
}
