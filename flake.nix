{
  description = "rust dev shell (toolchain from rust-toolchain.toml)";

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
        formatter = pkgs.nixfmt-rfc-style;

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.cargo-nextest
            pkgs.cargo-deny
            pkgs.cargo-audit
            pkgs.just
            pkgs.pre-commit
            # Nix quality tools for the pre-commit `_nix` overlay hooks
            # (nixfmt/statix/deadnix run as language:system off PATH).
            pkgs.nixfmt-rfc-style
            pkgs.statix
            pkgs.deadnix
            # typos + committed also run as language:system off PATH. The
            # crate-ci pre-commit hooks are `language: python` and their wheels
            # ship prebuilt, dynamically-linked binaries, which cannot exec in a
            # dctl agents container (no FHS: no /lib64/ld-linux-x86-64.so.2).
            # A Nix build runs. See the hook comments in .pre-commit-config.yaml.
            pkgs.typos
            pkgs.committed
          ];
          # native deps for -sys crates, uncomment as needed:
          # buildInputs = [ pkgs.openssl ];
          # nativeBuildInputs = [ pkgs.pkg-config ];
          shellHook = ''echo "rust dev shell ready (toolchain from rust-toolchain.toml)"'';
        };
      }
    );
}
