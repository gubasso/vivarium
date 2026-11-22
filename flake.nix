{
  description = "vivarium's development environment (toolchain from rust-toolchain.toml)";

  # This flake's single purpose is the **development environment** for working on
  # vivarium: the Rust toolchain, the runtimes the git hooks resolve off
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
    # The release convention's own tool, pinned at a release tag. The
    # `exobrain outdated` line of `.envrc` reports a newer release and moves
    # nothing, and it names the command that moves the tag and the lock. A
    # person runs that command and commits this file and the lock together.
    # One project runs one `rk`: this pin, never a host install.
    release-kit = {
      url = "github:gubasso/release-kit/v0.10.3";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        rust-overlay.follows = "rust-overlay";
        # rk's own flake pins an earlier rk and sdd for its development. The
        # empty string points each at nothing, so flake.lock holds no node.
        release-kit.follows = "";
        spec-driven-docs.follows = "";
      };
    };
  };

  outputs =
    # `...` is required: Nix always applies `outputs (inputs // { self = …; })`,
    # so a closed attrset breaks the flake the moment `self` is unused and gets
    # dropped (deadnix flags it). Keep it even when no extra arg is consumed.
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      release-kit,
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
        # `nix fmt` uses the RFC 166 formatter (also on PATH for the nixfmt hook).
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
            # Nix owns runtimes; prek, which the host supplies, owns hooks.
            # markdownlint-cli2 uses this Node via language_version: system
            # instead of nodeenv. The
            # `slides/` deck is the second consumer: Nix supplies the runtime
            # and npm supplies Slidev, pinned by slides/package-lock.json, the
            # same division of labour (ADR-0104).
            pkgs.nodejs
            # Interpreter for the language:system check-table-pipes and
            # check-markdown-emphasis hooks.
            pkgs.python3
            # Binary for the language:system taplo-format hook.
            pkgs.taplo
            # Nix tools of the system hooks: nixfmt, which `nix fmt` and the
            # base-image lane run too (the formatter above), and statix, which
            # has no upstream hook. deadnix, typos, committed, shfmt, ruff, and
            # actionlint run as upstream hooks that prek builds, so they
            # have no package here, and a language library never enters this
            # shell.
            pkgs.nixfmt
            pkgs.statix
            # actionlint, a hook that prek builds, runs shellcheck from
            # PATH on the `run:` scripts of the workflows it reads.
            pkgs.shellcheck
            # rustup-init, which prek runs to build the rust hooks
            # (ripsecrets, deadnix), downloads with curl.
            pkgs.curl
            # dprint (the `dprint`/`dprint-markdown` hooks): upstream publishes
            # no hook, so the hooks run it off PATH from here.
            pkgs.dprint
            # lychee (the `lychee-system` link-check hook): upstream offers
            # only script, docker_image, and system ids, so the hook runs it
            # off PATH from here.
            pkgs.lychee
            # The host lanes base-image-check, exec-and-shell-check,
            # share-benchmark-check, store-density-check,
            # store-gc-interlock-check, and store-pressure-check parse JSON
            # with it.
            pkgs.jq
            # The `tests/net_namespaces` lane runs the four networking seams against
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
            # `rk`, from the pinned input above. The `rk-status-check` hook runs
            # it, so CI resolves it here rather than needing a host install on
            # the runner.
            release-kit.packages.${system}.default
          ];
          # native deps for -sys crates, uncomment as needed:
          # buildInputs = [ pkgs.openssl ];
          # nativeBuildInputs = [ pkgs.pkg-config ];
          # Cargo's build directory is kept OUT of the working tree, and this is
          # load-bearing rather than tidiness. The product flake is entered as
          # `git+file://$REPO_ROOT?dir=nix` so that the repository root is its
          # source tree and the crate can be built from its default layout — no
          # duplicate. `git+file:` reads only the git tree and leaves an ignored
          # `target/` out of the store, but a `path:` reference to the checkout
          # copies the whole working tree, ignored files included, so an
          # in-tree multi-gigabyte cache would ride along with every such
          # evaluation. Out of the tree, it costs nothing.
          # And it is one folder per worktree, the same one for the shell and for
          # every gated hook of that worktree (ADR-0106). Two worktrees never
          # share a folder: cargo judges freshness by file times, so a shared one
          # can hand one worktree the build of the other and report it fresh.
          # `tests/host/build-folder` names the folder, and `heavy-run` asks it
          # too, so a shell that named it some other way would fill a second
          # folder with the same objects. The shell asks the same resolver the
          # hooks ask, in its silent form: entering a shell writes nothing, so
          # this locates rather than gates, and no drive is a notice and a
          # folder under the cache root rather than a refusal. Both scripts are
          # reached by relative path because the shell is entered from the
          # repository root — direnv, `just`, and CI all do — and a shell entered
          # from elsewhere leaves CARGO_TARGET_DIR to cargo, with the reason on
          # stderr.
          shellHook = ''
            if [ -z "''${CARGO_TARGET_DIR:-}" ]; then
              if [ ! -x tests/host/build-folder ]; then
                echo "not entered from the repository root; cargo picks the build folder" >&2
              elif [ -x tests/host/disk-preflight ] \
                && heavy_drive=$(tests/host/disk-preflight --locate --images --require-drive 2>/dev/null); then
                CARGO_TARGET_DIR=$(tests/host/build-folder "$heavy_drive") && export CARGO_TARGET_DIR
              else
                CARGO_TARGET_DIR=$(tests/host/build-folder "''${XDG_CACHE_HOME:-$HOME/.cache}/vivarium") \
                  && export CARGO_TARGET_DIR
                echo "no heavy drive resolved; the build folder stays on this disk" >&2
              fi
              unset heavy_drive
            fi
            # Both greetings go to standard error. `nix develop --command` shares
            # the command's stdout, so on stdout these lines are prepended to
            # whatever it emits, and a caller that reads or `eval`s that output
            # would then receive this text.
            echo "rust dev shell ready (toolchain from rust-toolchain.toml; CARGO_TARGET_DIR=''${CARGO_TARGET_DIR:-target})" >&2
            echo "\`viv\` on PATH builds from this tree and shadows any installed one; \`just install\` for elsewhere" >&2
          '';
        };
      }
    );
}
