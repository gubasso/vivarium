# ADR-0087/ADR-0088's unmeasured mechanism, measured: does the guest's
# `local-overlay` store actually share the lower layer, persist its own
# database, and collect without whiteouting what the lower store knows.
#
# A measurement leg. It is composed into an image only by `tests/nix/measurement`,
# never by `nix/guest.nix` — see ADR-0095.
{
  pkgs,
  storeLayout,
  storeCanaryExpression,
  ...
}:

let
  inherit (storeLayout)
    lowerStoreDir
    lowerStoreViewDir
    lowerStateDir
    lowerStoreUri
    upperRoot
    upperLayer
    upperStateDir
    ;
in
{
  systemd.services.vivarium-store-spike = {
    enableStrictShellChecks = true;
    environment = {
      VIVARIUM_UPPER_ROOT = upperRoot;
      VIVARIUM_UPPER_LAYER = upperLayer;
      VIVARIUM_LOWER_STORE_DIR = lowerStoreDir;
      VIVARIUM_LOWER_STORE_VIEW_DIR = lowerStoreViewDir;
      VIVARIUM_LOWER_STORE_URI = lowerStoreUri;
      VIVARIUM_LOWER_STATE_DIR = lowerStateDir;
      VIVARIUM_STORE_CANARY_EXPRESSION = storeCanaryExpression;
      VIVARIUM_UPPER_STATE_DIR = upperStateDir;
    };
    description = "Persistent local-overlay guest store spike";
    wantedBy = [ "multi-user.target" ];
    after = [
      "nix-daemon.socket"
      "systemd-tmpfiles-setup.service"
    ];
    serviceConfig = {
      Type = "oneshot";
      StandardOutput = "journal";
      StandardError = "journal";
      # Bounded so a wedged probe cannot outlive the diagnostic's own ceiling.
      TimeoutStartSec = "600s";
    };
    path = [
      pkgs.coreutils
      pkgs.findutils
      pkgs.gawk
      pkgs.gnugrep
      pkgs.gnused
      pkgs.nix
      pkgs.util-linux
    ];
    script = builtins.readFile ./store-spike.sh;
  };
}
