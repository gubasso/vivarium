{
  pkgs,
  launchArguments,
  supervisorPackage,
}:

let
  launchArgumentsFile = pkgs.writeText "vivarium-first-microvm-launch-arguments.json" (
    builtins.toJSON launchArguments
  );
in
pkgs.writeShellApplication {
  name = "vivarium-first-microvm";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.gnugrep
    pkgs.jq
  ];
  # Substitution must precede `writeShellApplication`: its generated
  # buildCommand makes post-install substitution ineffective. Pre-commit
  # reports source-relative shellcheck lines; Nix's check is the backstop.
  text =
    builtins.replaceStrings
      [ "@launchArgumentsPath@" "@vivPath@" ]
      [
        (toString launchArgumentsFile)
        "${supervisorPackage}/bin/viv"
      ]
      (builtins.readFile ./runner.sh);
}
