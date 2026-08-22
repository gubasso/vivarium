# ADR-0085's measurement: the host deletes a store path the guest has already
# read, while the guest runs, and this leg observes what the guest then sees.
# The only leg that requires the *host* to act mid-boot, which is why it carries
# a handshake through the workspace share rather than an instruction alone.
#
# A measurement leg. It is composed into an image only by `tests/nix/measurement`,
# never by `nix/guest.nix` — see ADR-0095.
{ pkgs, verificationWorkspaceInternal, ... }:

{
  systemd.services.vivarium-gc-interlock = {
    enableStrictShellChecks = true;
    environment.VIVARIUM_GC_INTERLOCK_DIR = "${verificationWorkspaceInternal}/.vivarium-gc-interlock";
    description = "ADR-0085 host garbage-collection interlock measurement";
    wantedBy = [ "multi-user.target" ];
    # `RequiresMountsFor` alone: it expands to both the `Requires` and the
    # `After` on the synthesised mount unit, so naming an escaped unit path by
    # hand — which the mount point's own value would have to be spelled into —
    # is unnecessary. The share, not the mirror (ADR-0100): this leg measures
    # the share, and the internal mount point is where the share is.
    unitConfig.RequiresMountsFor = [ verificationWorkspaceInternal ];
    serviceConfig = {
      Type = "oneshot";
      StandardOutput = "journal";
      StandardError = "journal";
      # Must cover the whole handshake: the before-phase, up to GO_TIMEOUT
      # (300s) waiting on the host, and the after-phase including a 60s
      # settle. The host's own VM ceiling has to clear this in turn.
      TimeoutStartSec = "900s";
      # systemd stops writing unit status to the console once boot has
      # completed, and this unit runs after multi-user.target — so a unit that
      # dies mid-probe leaves the console showing nothing but a missing marker,
      # which reads identically to a probe that returned no output. That is
      # exactly the silent-failure shape the harness forbids. ExecStopPost runs
      # even when the main process is killed by a signal, and systemd fills
      # these three variables in, so the run always says how it ended.
      ExecStopPost = pkgs.writeShellScript "vivarium-gc-interlock-result" ''
        echo "VIVARIUM_GC_INTERLOCK_RESULT=$SERVICE_RESULT code=''${EXIT_CODE:-none} status=''${EXIT_STATUS:-none}" >/dev/console
      '';
    };
    # No `nix`: this unit asks nothing of the store database. It reads files.
    path = [
      pkgs.coreutils
      pkgs.util-linux
    ];
    script = builtins.readFile ./gc-interlock.sh;
  };
}
