# The share and boot benchmark: what it costs a user to work in the sandbox, and
# whether the pinned worker-pool size is the right number (ADR-0096, which this
# leg's concurrent sweep moved off ADR-0051's `4`).
#
# **Metric names are vivarium's own, deliberately.** The backlog asked for
# upstream Cloud Hypervisor's names so the figures would be comparable; reading
# upstream's harness, read at v52.0, settled that they are not. The version is
# named because that is where the reading was done, not as a current pin; the
# conclusion is about what the metrics measure, which no release since has
# changed. Its
# `boot_time_ms` is the interval between guest debug-I/O-port codes 0x40 and
# 0x41 — a kernel-internal window that excludes everything before kernel entry —
# and its `block_*_MiBps` is `fio --direct=1 --bs=4k --ioengine=io_uring` against
# a raw dedicated device. Neither is what is measured here. Publishing different
# measurements under upstream's names would be the method note's own failure
# ("assert on the thing the check is named after") applied to naming, and it is
# the shape most likely to survive review and mislead a later reader.
#
# A measurement leg. It is composed into an image only by `tests/nix/measurement`.
{ pkgs, workspaceInternalMountPoint, ... }:

let
  # The share's own mount point, not the mirrored path: this leg measures what
  # working through the share costs, and the mirror is a bind of the same mount.
  benchHandshakeDir = "${workspaceInternalMountPoint}/.vivarium-share-benchmark";
in
{
  systemd.services.vivarium-share-benchmark = {
    enableStrictShellChecks = true;
    environment.VIVARIUM_BENCH_HANDSHAKE_DIR = benchHandshakeDir;
    description = "Share throughput, metadata latency and boot timing measurement";
    wantedBy = [ "multi-user.target" ];
    after = [ "vivarium-volume-prepare.service" ];
    unitConfig.RequiresMountsFor = [
      workspaceInternalMountPoint
      "/home/vivarium"
    ];
    serviceConfig = {
      Type = "oneshot";
      StandardOutput = "journal";
      StandardError = "journal";
      # Three repetitions of two workloads across two filesystems and two cache
      # placements, each preceded by a cache drop.
      TimeoutStartSec = "1800s";
      ExecStopPost = pkgs.writeShellScript "vivarium-share-benchmark-result" ''
        echo "VIVARIUM_BENCH_RESULT=$SERVICE_RESULT code=''${EXIT_CODE:-none} status=''${EXIT_STATUS:-none}" >/dev/console
      '';
    };
    path = [
      pkgs.coreutils
      pkgs.findutils
      pkgs.gawk
      pkgs.gnugrep
      # Created by the workload correction: the backlog names `git status` and
      # `rg` specifically, and `path` REPLACES the unit's PATH rather than
      # extending the system one — a missing tool here is a bare exit 127.
      pkgs.git
      pkgs.ripgrep
      pkgs.util-linux
      pkgs.systemd
    ];
    script = builtins.readFile ./benchmark.sh;
  };
}
