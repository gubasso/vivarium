{
  pkgs,
  launchArguments,
}:

let
  launchArgumentsFile = pkgs.writeText "vivarium-first-microvm-launch-arguments.json" (
    builtins.toJSON launchArguments
  );
  # Substitution must precede `writeShellApplication`: its generated
  # buildCommand makes post-install substitution ineffective. Pre-commit
  # reports source-relative shellcheck lines; Nix's check is the backstop.
  launcher = pkgs.writeShellApplication {
    name = "vivarium-first-microvm";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.gnugrep
      pkgs.jq
    ];
    text = builtins.replaceStrings [ "@launchArgumentsPath@" ] [ (toString launchArgumentsFile) ] (
      builtins.readFile ./runner.sh
    );
  };
in
# The launcher plus the contract schema at a stable path. `viv` reads the schema
# from the selected build before executing anything in it (spec/10): a build kept
# reachable on purpose — an old generation, `--no-rebuild` — may speak an older
# contract than the running binary, and the refusal must name both numbers before
# boot rather than fail inside the launch.
pkgs.symlinkJoin {
  name = "vivarium-first-microvm";
  paths = [
    launcher
    (pkgs.writeTextDir "share/vivarium/launch-contract-schema" "${toString launchArguments.schemaVersion}\n")
  ];
}
