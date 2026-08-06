{
  description = "vivarium's development environment (toolchain from rust-toolchain.toml)";

  # This flake's single purpose is the **development environment** for working on
  # vivarium: the Rust toolchain, the runtimes the pre-commit hooks resolve off
  # PATH, and the devShell direnv activates. That is all.
  #
  # It is **not part of what vivarium builds.** The microVM — the guest, its
  # launcher and their contract — is its own flake at `nix/flake.nix`, with its
  # own lock, precisely so a product input never has to enter this file. If you
  # find yourself adding a hypervisor, a guest kernel or `microvm.nix` here, it
  # belongs there instead.

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    # `...` is required: Nix always applies `outputs (inputs // { self = …; })`,
    # so a closed attrset breaks the flake the moment `self` is unused and gets
    # dropped (deadnix flags it). Keep it even when no extra arg is consumed.
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        # Reads channel + components + targets straight from rust-toolchain.toml.
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        # `nix fmt` uses the RFC 166 formatter (also on PATH for the pre-commit hook).
        formatter = pkgs.nixfmt;

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.cargo-nextest
            pkgs.cargo-deny
            pkgs.cargo-audit
            pkgs.just
            pkgs.pre-commit
            # Nix owns runtimes; pre-commit owns hooks. markdownlint-cli2 uses
            # this Node via language_version: system instead of nodeenv.
            pkgs.nodejs
            # Interpreter for the language:system check-table-pipes and
            # check-markdown-emphasis hooks.
            pkgs.python3
            # Binary for the language:system taplo-format hook.
            pkgs.taplo
            # Nix quality tools for the pre-commit `_nix` overlay hooks
            # (nixfmt/statix/deadnix run as language:system off PATH).
            pkgs.nixfmt
            pkgs.statix
            pkgs.deadnix
            # typos + committed also run as language:system off PATH. The
            # crate-ci pre-commit hooks are `language: python` and their wheels
            # ship prebuilt, dynamically-linked binaries, which cannot exec in a
            # dctl agents container (no FHS: no /lib64/ld-linux-x86-64.so.2).
            # A Nix build runs. See the hook comments in .pre-commit-config.yaml.
            pkgs.typos
            pkgs.committed
            # dprint (the `dprint`/`dprint-markdown` hooks) is the same story:
            # `language: system`, resolved off PATH, and a cargo-installed
            # dprint is a prebuilt glibc ELF that cannot exec without FHS.
            pkgs.dprint
          ];
          # native deps for -sys crates, uncomment as needed:
          # buildInputs = [ pkgs.openssl ];
          # nativeBuildInputs = [ pkgs.pkg-config ];
          # Cargo's build directory is kept OUT of the working tree, and this is
          # load-bearing rather than tidiness. The product flake is entered as
          # `path:$REPO_ROOT?dir=nix` so that the repository root is its source
          # tree and the crate can be built from its default layout — no
          # duplicate. `path:` fetches the whole working tree and does not honour
          # `.gitignore`, so a multi-gigabyte `target/` would be copied into the
          # store on every evaluation. Out of the tree, it costs nothing.
          shellHook = ''
            export CARGO_TARGET_DIR="''${CARGO_TARGET_DIR:-''${XDG_CACHE_HOME:-$HOME/.cache}/vivarium/target}"
            echo "rust dev shell ready (toolchain from rust-toolchain.toml; CARGO_TARGET_DIR=$CARGO_TARGET_DIR)"
          '';
        };
      }
    );
}
