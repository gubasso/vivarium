# The share and boot benchmark: what it costs a user to work in the sandbox, and
# whether ADR-0051's pinned worker-pool size is the right number.
#
# **Metric names are vivarium's own, deliberately.** The backlog asked for
# upstream Cloud Hypervisor's names so the figures would be comparable; reading
# upstream's harness at the pinned v52.0 settled that they are not. Its
# `boot_time_ms` is the interval between guest debug-I/O-port codes 0x40 and
# 0x41 — a kernel-internal window that excludes everything before kernel entry —
# and its `block_*_MiBps` is `fio --direct=1 --bs=4k --ioengine=io_uring` against
# a raw dedicated device. Neither is what is measured here. Publishing different
# measurements under upstream's names would be the method note's own failure
# ("assert on the thing the check is named after") applied to naming, and it is
# the shape most likely to survive review and mislead a later reader.
#
# A measurement leg. It is composed into an image only by `nix/measurement`.
{ pkgs, ... }:

let
  benchHandshakeDir = "/workspaces/vivarium/.vivarium-share-benchmark";
in
{
  systemd.services.vivarium-share-benchmark = {
    description = "Share throughput, metadata latency and boot timing measurement";
    wantedBy = [ "multi-user.target" ];
    after = [
      "workspaces-vivarium.mount"
      "home-vivarium.mount"
      "vivarium-volume-prepare.service"
    ];
    unitConfig.RequiresMountsFor = [
      "/workspaces/vivarium"
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
    script = ''
      set -u
      set +e
      exec >/dev/console 2>&1
      echo 'VIVARIUM_BENCH_BEGIN=yes'
      echo 'VIVARIUM_BENCH_PROTOCOL=1'

      instruction=${benchHandshakeDir}/instruction
      if [ ! -r "$instruction" ]; then
        echo 'VIVARIUM_BENCH_SKIPPED=no-instruction'
        exit 0
      fi

      REPS=3; TREE=; POOL=
      # shellcheck disable=SC1090
      . "$instruction"
      echo "VIVARIUM_BENCH_CONFIG=reps=$REPS tree=$TREE pool=$POOL"

      # PID 1 stops writing unit status to the console after boot, but it can
      # still interleave — and a split marker is indistinguishable from a lost
      # one on a channel where a burst-fidelity defect is already on the record.
      kill -s RTMIN+21 1 2>/dev/null || true
      trap 'kill -s RTMIN+20 1 2>/dev/null || true' EXIT

      # --- boot timing --------------------------------------------------------
      # Guest-side only. The host measures its own launcher-exec-to-first-marker
      # interval separately and reports it under a different name; two numbers,
      # both named, neither impersonating the other.
      echo "VIVARIUM_BENCH_guest_userspace_ms=$(systemd-analyze time 2>/dev/null | head -n1 | tr '\n' ' ')"
      echo "VIVARIUM_BENCH_guest_blame_top=$(systemd-analyze blame 2>/dev/null | head -n5 | tr '\n' ';')"

      # --- the workload tree --------------------------------------------------
      # Generated host-side and copied in before any timing starts, so tree
      # creation is never inside a timed region and the two filesystems hold
      # byte-identical trees rather than similar ones.
      share_tree=${benchHandshakeDir}/tree
      vol_tree=/home/vivarium/bench-tree
      if [ ! -d "$share_tree" ]; then
        echo 'VIVARIUM_BENCH_SKIPPED=no-tree'
        exit 0
      fi
      rm -rf "$vol_tree"
      cp -a "$share_tree" "$vol_tree" || { echo 'VIVARIUM_BENCH_SKIPPED=tree-copy-failed'; exit 0; }

      count_of() { find "$1" -type f | wc -l; }
      digest_of() { find "$1" -type f -printf '%P %s\n' | sort | sha256sum | cut -d' ' -f1; }
      sc=$(count_of "$share_tree"); vc=$(count_of "$vol_tree")
      sd=$(digest_of "$share_tree"); vd=$(digest_of "$vol_tree")
      echo "VIVARIUM_BENCH_TREE=share_files=$sc volume_files=$vc share_digest=$sd volume_digest=$vd"
      if [ "$sc" != "$vc" ] || [ "$sd" != "$vd" ]; then
        echo 'VIVARIUM_BENCH_SKIPPED=trees-differ'
        exit 0
      fi

      # --- the cache drop -----------------------------------------------------
      # The workspace share is `cache = "auto"`, and a 50k-file tree fits the
      # guest page cache easily — so repetitions 2 and 3 would issue almost no
      # FUSE traffic, and the pool is only exercised BY FUSE traffic. Without
      # this the sweep returns four near-identical numbers and "the constant
      # does not matter" becomes a publishable-looking conclusion drawn from a
      # measurement that never reached the thing it names.
      #
      # Dropped before EVERY measured repetition, not once as a warm-up: a
      # warm-up guarantees the condition under which the measurement says
      # nothing. The HOST page cache stays warm either way, and that is stated
      # in the register rather than pretended away.
      drop_caches() { sync; echo 3 >/proc/sys/vm/drop_caches 2>/dev/null || true; }

      ms_since() { echo $(( ( $(date +%s%N) - $1 ) / 1000000 )); }

      timed() { # timed <metric> <dir> <cmd...>
        local metric=$1 dir=$2; shift 2
        local r t0 out
        for r in $(seq 1 "$REPS"); do
          drop_caches
          t0=$(date +%s%N)
          out=$( (cd "$dir" && "$@") 2>&1 >/dev/null )
          t=$(ms_since "$t0")
          echo "VIVARIUM_BENCH_$metric=$t rep=$r pool=$POOL''${out:+ stderr=$(echo "$out" | tr '\n' '|' | tail -c 120)}"
        done
      }

      # --- metadata and content workloads, share vs volume --------------------
      # `git status` is an lstat storm over the index — the metadata-heavy shape
      # that actually discriminates pool sizes. `rg` is the content-read shape.
      # Both are what the backlog names; a bare `find` walk and a `grep -r` are
      # a metadata workload and a read-throughput workload in metadata costume.
      export HOME=/home/vivarium
      git config --global --add safe.directory '*' 2>/dev/null || true

      timed workspace_git_status_ms "$share_tree" git status --porcelain
      timed volume_git_status_ms    "$vol_tree"   git status --porcelain
      timed workspace_rg_ms         "$share_tree" rg --no-messages --count-matches vivarium
      timed volume_rg_ms            "$vol_tree"   rg --no-messages --count-matches vivarium

      # --- caches on the volume vs on the share -------------------------------
      # `spec/06` says "keep regenerable caches off the share" and argues it from
      # negative-lookup semantics alone. This is the leg that puts a number on
      # it — and it is a PROXY for a cache workload, not a cache workload, which
      # the register must say. Same tree, same tool, only the cache directory's
      # filesystem changes.
      for placement in share volume; do
        case $placement in
          share)  cache_dir=${benchHandshakeDir}/cache ;;
          volume) cache_dir=/home/vivarium/bench-cache ;;
        esac
        rm -rf "$cache_dir"; mkdir -p "$cache_dir"
        drop_caches
        t0=$(date +%s%N)
        # A git repository whose object/index writes land in $cache_dir: the
        # cache placement is what varies, the worked-on tree is constant.
        (cd "$vol_tree" && GIT_INDEX_FILE="$cache_dir/index" git status --porcelain) >/dev/null 2>&1
        echo "VIVARIUM_BENCH_cache_on_''${placement}_ms=$(ms_since $t0) pool=$POOL"
      done

      # --- block throughput ---------------------------------------------------
      # Against `/home/vivarium` (the default volume), never against the store
      # volume, which carries a live filesystem.
      #
      # The image is sparse on a copy-on-write host filesystem, so the FIRST
      # pass measures first-allocation rather than steady state. Write twice and
      # report both; the second is the number to compare across variants.
      bench_file=/home/vivarium/bench-block
      for pass in 1 2; do
        drop_caches
        w=$( { dd if=/dev/zero of="$bench_file" bs=1M count=512 oflag=direct conv=fsync; } 2>&1 | tail -n1 )
        echo "VIVARIUM_BENCH_home_volume_write_MiBps=$(echo "$w" | grep -oE '[0-9.]+ [MG]B/s' | tail -n1) pass=$pass pool=$POOL raw=$(echo "$w" | tr ',' ' ')"
      done
      drop_caches
      r=$( { dd if="$bench_file" of=/dev/null bs=1M count=512 iflag=direct; } 2>&1 | tail -n1 )
      echo "VIVARIUM_BENCH_home_volume_read_MiBps=$(echo "$r" | grep -oE '[0-9.]+ [MG]B/s' | tail -n1) pool=$POOL raw=$(echo "$r" | tr ',' ' ')"
      rm -f "$bench_file"

      rm -rf "$vol_tree" /home/vivarium/bench-cache
      echo 'VIVARIUM_BENCH_COMPLETE=yes'
    '';
  };
}
