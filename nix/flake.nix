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
  # Enter this flake as `path:$REPO_ROOT?dir=nix`, not `path:$REPO_ROOT/nix`. A
  # flake's source tree is fixed by the reference used to enter it, not by where
  # its file sits: `?dir=` keeps the flake and this lock here while making the
  # repository the tree, which is what lets `default.nix` build the Rust crate
  # from its default layout at `../` instead of a duplicated snapshot of it. The
  # `/nix` form pins the tree one level too deep, and the crate then fails to
  # resolve under pure evaluation.
  #
  # These outputs exist as a *flake* rather than as bare `nix-build` targets for
  # one reason worth stating, because it looks like ceremony and is not:
  # `tests/host/first-microvm-check`'s three strongest evaluation-tier checks
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
    # Linux-only, as the system list rather than a filter over the outputs: every
    # one of these outputs boots a guest kernel behind KVM (N1), and
    # `launch-arguments.nix` resolves no other system. Asking
    # `legacyPackages.<system>.stdenv.isLinux` would first have to evaluate a
    # non-Linux `stdenv`, which nixpkgs 26.11 refuses for `x86_64-darwin`; naming
    # the supported systems answers the same question without evaluating an
    # unsupported one.
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        product = import ./. { inherit nixpkgs microvm system; };
        # `?dir=nix` keeps the repository root as the source tree. This flake
        # is the publication surface for checks, so this is the sole permitted
        # product-to-verification edge; `check-verification-boundary` guards it.
        verification = import ../tests/nix { inherit product; };
      in
      {
        inherit (verification) packages checks;
        # The seam a generated project flake enters this one through (slice 012).
        # It composes the same guest module with the same build inputs and hands
        # the resolved config back to `mkLaunch`, so a manifest-built guest and
        # the shipped image are the same guest reached by two routes rather than
        # two guests that happen to agree. `lib` and not `packages` because none
        # of the three is a derivation: two are a module and an attrset of build
        # inputs, and the third is a function.
        lib = {
          inherit (product) guestModule guestSpecialArgs mkLaunch;
        };
      }
    );
}
