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
        # `viv`, built from the working tree on every invocation. This is a shell
        # package and not a flake output: `packages` at this level would be a
        # build output, which is the boundary `scripts/check-flake-boundary`
        # enforces, and the whole point of the shim is that it builds nothing
        # ahead of time. `scripts/viv-shim.sh` carries the rest of the rationale.
        devWrapper = pkgs.writeShellApplication {
          name = "viv";
          runtimeInputs = [
            toolchain
            pkgs.nix
            pkgs.jq
          ];
          text = builtins.readFile ./scripts/viv-shim.sh;
        };
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
            # The credential-relay acceptance trial arranges a real host agent:
            # ssh-agent, ssh-add, and ssh-keygen come from here, not the host.
            pkgs.openssh
            pkgs.just
            pkgs.pre-commit
            # Nix owns runtimes; pre-commit owns hooks. markdownlint-cli2 uses
            # this Node via language_version: system instead of nodeenv. The
            # `slides/` deck is the second consumer: Nix supplies the runtime
            # and npm supplies Slidev, pinned by slides/package-lock.json, the
            # same division of labour (ADR-0104).
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
            # Shell, Python, and Actions linting use the same Nix-owned runtime
            # model as the existing local hooks below.
            pkgs.shellcheck
            pkgs.shfmt
            pkgs.ruff
            pkgs.actionlint
            pkgs.zizmor
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
            # `scripts/baseline-pins` reads `nix flake archive --json` with it,
            # and `scripts/install-dev` reaches that script through this shell.
            pkgs.jq
            # The `tests/net_host` lane runs the four networking seams against
            # the real tools: `unshare`/`nsenter` for the namespace pair, `ip`
            # for the tap, `nft` for the ruleset. Dev-shell copies of what the
            # product pins through nix/flake.lock's backend programs.
            pkgs.util-linux
            pkgs.iproute2
            pkgs.nftables
            # `viv` itself, so entering the shell (or `direnv allow`) needs no
            # separate install step. The shim, not a built package, so what PATH
            # resolves is the working tree rather than the last evaluation of it.
            devWrapper
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
          # And it is one cache rather than two (ADR-0106). Every gated hook binds
          # `<drive>/cargo-target`, so a shell that defaulted somewhere else would
          # fill a second cache with the same objects and pay for both. The shell
          # asks the same resolver the hooks ask, in its silent form: entering a
          # shell writes nothing, so this locates rather than gates, and no drive
          # is a notice and the cache root rather than a refusal. The resolver is
          # reached by relative path because the shell is entered from the
          # repository root — direnv, `just`, and CI all do — and a shell entered
          # from elsewhere simply takes the fallback with the reason on stderr.
          shellHook = ''
            if [ -z "''${CARGO_TARGET_DIR:-}" ]; then
              if [ -x tests/host/disk-preflight ] \
                && heavy_drive=$(tests/host/disk-preflight --locate --images --require-drive 2>/dev/null); then
                export CARGO_TARGET_DIR="$heavy_drive/cargo-target"
              else
                export CARGO_TARGET_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/vivarium/target"
                echo "no heavy drive resolved; the compile cache stays on this disk" >&2
              fi
              unset heavy_drive
            fi
            # Both greetings go to standard error. `nix develop --command` shares
            # the command's stdout, so on stdout these lines are prepended to
            # whatever it emits — and `scripts/baseline-pins` exists to have its
            # stdout `eval`ed, which would then execute this text.
            echo "rust dev shell ready (toolchain from rust-toolchain.toml; CARGO_TARGET_DIR=$CARGO_TARGET_DIR)" >&2
            echo "\`viv\` on PATH builds from this tree and shadows any installed one; \`just install\` for elsewhere" >&2
          '';
        };
      }
    );
}
