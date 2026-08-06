# ADR-0089's thresholds and ADR-0091's ratio, measured under load.
#
# This leg also owns the two upstream Nix **test hooks** the measurement needs.
# They used to sit on the shipped `nix-daemon` unit, where they were a behaviour
# switch reachable by anything that could write `/run/vivarium/nix-free-space`.
# Moving them here is a security fix, not tidying: the base image's store daemon
# must not read a test hook for free space (ADR-0095).
#
# A measurement leg. It is composed into an image only by `tests/nix/measurement`,
# never by `nix/guest.nix`.
{
  config,
  lib,
  pkgs,
  storeLayout,
  storeFreeSpaceHook,
  ...
}:

let
  inherit (storeLayout) upperRoot upperLayer;
  ballastExpression = pkgs.replaceVars ./ballast.nix {
    inherit (pkgs) bash coreutils;
    system = pkgs.stdenv.hostPlatform.system;
  };
  # Read by `LocalStore::autoGC` in place of `statvfs` when the variable is set —
  # upstream's own hook, the one `tests/functional/gc-auto.sh` uses. Seeded far
  # above `max-free` so it is inert until a measurement arm lowers it.
  freeSpaceHookDir = "/run/vivarium";
  freeSpaceHookFile = "${freeSpaceHookDir}/nix-free-space";
  # An ADDITIVE config file the daemon reads on top of /etc/nix/nix.conf. Empty
  # by default. Measured not to reach `autoGC`, which is why the scaled image
  # exists at all; retained because arm D's negative result is the evidence.
  extraConfFile = "${freeSpaceHookDir}/nix-extra.conf";
  pressureHandshakeDir = "/workspaces/vivarium/.vivarium-store-pressure";
in
{
  systemd = {
    # The hook is INERT BY VALUE — seeded far above `max-free` so an ordinary
    # boot never collects. That is not the same as inert by ABSENCE, and the
    # difference cost this round a boot: arm E measures a REAL crossing, and a
    # daemon reading a file that says one tebibyte never sees it. So the hook is
    # installed only for the variants whose arms drive it, and the arm that reads
    # real `statvfs` gets an image that has no hook at all.
    tmpfiles.rules = lib.optionals storeFreeSpaceHook [
      "d ${freeSpaceHookDir} 0755 root root - -"
      "f ${freeSpaceHookFile} 0644 root root - 1099511627776"
      "f ${extraConfFile} 0644 root root - "
    ];

    services.nix-daemon.serviceConfig.Environment = lib.optionals storeFreeSpaceHook [
      "_NIX_TEST_FREE_SPACE_FILE=${freeSpaceHookFile}"
      "NIX_USER_CONF_FILES=${extraConfFile}"
    ];

    services.vivarium-store-pressure = {
      enableStrictShellChecks = true;
      environment = {
        BALLAST_EXPRESSION = ballastExpression;
        VIVARIUM_PRESSURE_HANDSHAKE_DIR = pressureHandshakeDir;
        VIVARIUM_UPPER_ROOT = upperRoot;
        VIVARIUM_UPPER_LAYER = upperLayer;
        VIVARIUM_FREE_SPACE_HOOK_FILE = freeSpaceHookFile;
        VIVARIUM_EXTRA_CONF_FILE = extraConfFile;
        VIVARIUM_LOWER_STORE_URI = storeLayout.lowerStoreUri;
      };
      description = "ADR-0089/ADR-0091 store pressure and collection-trigger measurement";
      wantedBy = [ "multi-user.target" ];
      after = [
        "nix-daemon.socket"
        "workspaces-vivarium.mount"
      ];
      unitConfig.RequiresMountsFor = [
        "/workspaces/vivarium"
        upperRoot
      ];
      serviceConfig = {
        Type = "oneshot";
        StandardOutput = "journal";
        StandardError = "journal";
        # Three hours. Arm D writes gibibytes through the daemon, and each
        # collection walks the merged store over virtiofs. The unit also keeps
        # its own wall-clock budget below this, so a slow host ends the loop
        # cleanly with a final sample rather than being killed mid-series.
        TimeoutStartSec = "10800s";
        ExecStopPost = pkgs.writeShellScript "vivarium-store-pressure-result" ''
          echo "VIVARIUM_STORE_PRESSURE_RESULT=$SERVICE_RESULT code=''${EXIT_CODE:-none} status=''${EXIT_STATUS:-none}" >/dev/console
        '';
      };
      path = [
        pkgs.coreutils
        pkgs.e2fsprogs
        pkgs.findutils
        pkgs.gawk
        pkgs.gnugrep
        pkgs.gnused
        pkgs.nix
        # Arm D restarts nix-daemon to make it re-read its config, and `path`
        # REPLACES the unit's PATH rather than extending the system one, so
        # systemd must be named here or `systemctl` is a bare exit 127.
        config.systemd.package
        pkgs.util-linux
      ];
      script = builtins.readFile ./store-pressure.sh;
    };
  };
}
