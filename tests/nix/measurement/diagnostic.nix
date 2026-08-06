# The deterministic first-microVM diagnostic: the leg that reads the guest's own
# view of every contract the launcher claims to have set up, and prints it to the
# console for the host harness to parse.
#
# A measurement leg. It is composed into an image only by `tests/nix/measurement`,
# never by `nix/guest.nix` — see ADR-0095. Note that it no longer owns the
# poweroff: `tests/nix/measurement/default.nix` composes a stop unit ordered after
# every selected leg, so an image built without this leg still stops.
{ pkgs, ... }:

{
  systemd.services.vivarium-first-microvm-diagnostic = {
    enableStrictShellChecks = true;
    description = "Deterministic first-microVM diagnostic";
    wantedBy = [ "multi-user.target" ];
    after = [
      "vivarium-volume-prepare.service"
      "workspaces-vivarium.mount"
    ];
    requires = [
      "vivarium-volume-prepare.service"
      "workspaces-vivarium.mount"
    ];
    serviceConfig = {
      Type = "oneshot";
      # Deliberately *not* `journal+console`. journald forwards to the console
      # with one best-effort `writev(2)` per line and no short-write retry
      # (`src/journal/journald-console.c`), so a congested tty truncates a line
      # mid-byte and reports nothing — measured here as ~0.1% silent loss with a
      # host reader attached for the whole run. The script re-points its own
      # stdout at /dev/console instead, which is a blocking fd written by
      # coreutils' own retry loops. The journal keeps a copy for post-mortem.
      StandardOutput = "journal";
      StandardError = "journal";
    };
    # `systemd.services.<name>.path` *replaces* the unit's PATH rather than
    # extending the system one, so every command the script calls must be
    # named here. Only coreutils, findutils, gnugrep, gnused and systemd are
    # added by NixOS itself — gawk is not, and its absence killed the whole
    # diagnostic at the first `awk` under the `set -e` NixOS wraps a `script`
    # in. It is already in the system closure, so naming it costs nothing.
    path = [
      pkgs.coreutils
      pkgs.findutils
      pkgs.gawk
      pkgs.gnugrep
      pkgs.nix
      pkgs.util-linux
    ];
    script = builtins.readFile ./diagnostic.sh;
  };
}
