{
  pkgs,
  launchArguments,
}:

let
  # One copy at a published path, read by two consumers: the runner renders the
  # launch specification from it, and `viv` reads the share list from it before
  # the runner ever runs — a declared mount's source must be expanded and
  # refused pre-boot (spec/06, ADR-0020), and on `--no-rebuild` nothing else
  # evaluates, so the built artifact is the only place the merged mount list
  # exists.
  contract = pkgs.writeTextDir "share/vivarium/launch-arguments.json" (
    builtins.toJSON launchArguments
  );
  launchArgumentsFile = "${contract}/share/vivarium/launch-arguments.json";
  # Substitution must precede `writeShellApplication`: its generated
  # buildCommand makes post-install substitution ineffective. Pre-commit
  # reports source-relative shellcheck lines; Nix's check is the backstop.
  launcher = pkgs.writeShellApplication {
    name = "vivarium-base-image";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.gnugrep
      pkgs.jq
    ];
    text = builtins.replaceStrings [ "@launchArgumentsPath@" ] [ launchArgumentsFile ] (
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
  name = "vivarium-base-image";
  paths = [
    launcher
    contract
    (pkgs.writeTextDir "share/vivarium/launch-contract-schema" "${toString launchArguments.schemaVersion}\n")
  ];
}
