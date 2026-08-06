# ADR-0089's thresholds and ADR-0091's ratio, measured under load.
#
# This leg also owns the two upstream Nix **test hooks** the measurement needs.
# They used to sit on the shipped `nix-daemon` unit, where they were a behaviour
# switch reachable by anything that could write `/run/vivarium/nix-free-space`.
# Moving them here is a security fix, not tidying: the base image's store daemon
# must not read a test hook for free space (ADR-0095).
#
# A measurement leg. It is composed into an image only by `nix/measurement`,
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
        pkgs.nix
        # Arm D restarts nix-daemon to make it re-read its config, and `path`
        # REPLACES the unit's PATH rather than extending the system one, so
        # systemd must be named here or `systemctl` is a bare exit 127.
        config.systemd.package
        pkgs.util-linux
      ];
      script = ''
        set -u
        set +e
        exec >/dev/console 2>&1
        echo 'VIVARIUM_STORE_PRESSURE_BEGIN=yes'
        echo 'VIVARIUM_STORE_PRESSURE_PROTOCOL=1'

        instruction=${pressureHandshakeDir}/instruction
        if [ ! -r "$instruction" ]; then
          echo 'VIVARIUM_STORE_PRESSURE_SKIPPED=no-instruction'
          exit 0
        fi

        ARM=; BUDGET_SECONDS=3600; BALLAST_TOTAL_MIB=3072; FILE_BYTES=8192; CHATTY=no
        # Arm E asserts these rather than writing them; unset means "any", which
        # the arm refuses.
        EXPECT_MIN_FREE=; EXPECT_MAX_FREE=
        # shellcheck disable=SC1090
        . "$instruction"
        echo "VIVARIUM_STORE_PRESSURE_ARM=$ARM budget=''${BUDGET_SECONDS}s ballast=''${BALLAST_TOTAL_MIB}MiB file_bytes=$FILE_BYTES chatty=$CHATTY"

        export NIX_REMOTE=daemon
        export HOME=/root
        nix_build=${pkgs.nix}/bin/nix-build
        nix_store=${pkgs.nix}/bin/nix-store
        # `nix config show` is a `nix-command` subcommand, and this guest
        # deliberately does not enable that feature globally (see nix.settings).
        # Without the opt-in every config read returns empty, which reads as "the
        # daemon has no thresholds" rather than as "the reader was refused" —
        # measured, and it cost a boot.
        nix_cli="${pkgs.nix}/bin/nix --extra-experimental-features nix-command"

        # `df -h` rounds, and a 4 GiB crossing is invisible in "4.0G". Emit the
        # raw statvfs fields the collector itself reads, for BOTH the merged
        # store and the upper filesystem. ADR-0089's whole premise is that
        # statvfs on the overlay reports the upper filesystem; if these two ever
        # disagree, that disagreement is the headline finding and everything
        # else in this run is noise.
        sample() {
          echo "VIVARIUM_STORE_PRESSURE_SAMPLE=$1 merged=$(stat -f -c '%a %S %f %b %d %c' /nix/store 2>/dev/null) upper=$(stat -f -c '%a %S %f %b %d %c' ${upperRoot} 2>/dev/null) dead=$($nix_store --gc --print-dead 2>/dev/null | wc -l)"
        }

        # A background heartbeat, because a collection pass can be silent for
        # minutes and the host's console reader must not read that as a hang.
        ( while true; do sleep 15; echo "VIVARIUM_STORE_PRESSURE_HEARTBEAT=$(date +%s)"; done ) &
        heartbeat=$!
        trap 'kill $heartbeat 2>/dev/null' EXIT

        echo "VIVARIUM_STORE_PRESSURE_CONFIG_MINFREE=$($nix_cli config show min-free 2>&1 | tail -n1)"
        echo "VIVARIUM_STORE_PRESSURE_CONFIG_MAXFREE=$($nix_cli config show max-free 2>/dev/null)"
        echo "VIVARIUM_STORE_PRESSURE_CONFIG_INTERVAL=$($nix_cli config show min-free-check-interval 2>/dev/null)"
        # If store optimisation ever ran it would hardlink identical ballast and
        # invert the density reading, so assert it off rather than assume it.
        echo "VIVARIUM_STORE_PRESSURE_CONFIG_OPTIMISE=$($nix_cli config show auto-optimise-store 2>/dev/null)"
        echo "VIVARIUM_STORE_PRESSURE_LINKS_FSTYPE=$(stat -f -c %T /nix/store/.links 2>/dev/null)"
        echo "VIVARIUM_STORE_PRESSURE_ITABLE_ZEROED=$(dumpe2fs /dev/vdb 2>/dev/null | grep -c ITABLE_ZEROED)"
        echo "VIVARIUM_STORE_PRESSURE_ITABLE_GROUPS=$(dumpe2fs /dev/vdb 2>/dev/null | grep -c '^Group ')"
        sample START

        # The ballast expression. Built through the daemon, one process per
        # iteration: a temproot lives in `${upperRoot}/state/temproots/<pid>`
        # and dies with the process that made it, so iteration N's output is
        # unrooted garbage the moment iteration N's `nix-build` exits. A single
        # long-lived `nix` process over many outputs would keep every one of
        # them rooted and the collector would free nothing.
        #
        # `builtins.storePath` is used for the builder and its tools because
        # those paths ARE in the boot closure and therefore valid in this
        # guest's database — the sandbox binds declared inputs only, so a bare
        # path string would not be visible to the builder at all.
        # The expression is written ONCE and parameterised through the
        # environment, not regenerated per iteration with the numbers pasted in.
        # That is deliberate: pasting would mean three languages (Nix's outer
        # indented string, the shell heredoc, and the generated Nix file) all
        # competing for `${"\${"}`, which is how this kind of generator acquires
        # escaping bugs that only appear at run time. `nix-build` evaluates
        # impurely by default, so `builtins.getEnv` is available; the builder
        # text is assembled with `+` so the generated file contains no
        # interpolation at all.
        expr=/tmp/vivarium-ballast.nix
        cat >"$expr" <<'EXPR'
        let
          bash = builtins.storePath "@BASH@";
          coreutils = builtins.storePath "@COREUTILS@";
          each = builtins.getEnv "BALLAST_FILE_BYTES";
          mib = builtins.getEnv "BALLAST_MIB";
          chatty = builtins.getEnv "BALLAST_CHATTY";
        in
        derivation {
          name = "vivarium-ballast-" + (builtins.getEnv "BALLAST_NAME");
          system = "@SYSTEM@";
          builder = bash + "/bin/bash";
          args = [
            "-c"
            ("export PATH=" + coreutils + "/bin\n"
             + "set -e\n"
             + "mkdir -p \"$out\"\n"
             + "total=$(( " + mib + " * 1024 * 1024 ))\n"
             + "each=" + each + "\n"
             + "n=$(( total / each ))\n"
             + "i=0; dir=0\n"
             + "while [ $i -lt $n ]; do\n"
             + "  if [ $(( i % 1000 )) -eq 0 ]; then dir=$(( i / 1000 )); mkdir -p \"$out/d$dir\"; fi\n"
             + "  head -c $each /dev/urandom > \"$out/d$dir/f$i\"\n"
             + "  i=$(( i + 1 ))\n"
             + "  if [ " + chatty + " = yes ] && [ $(( i % 64 )) -eq 0 ]; then echo \"ballast $i/$n\"; fi\n"
             + "done\n")
          ];
        }
        EXPR
        sed -i \
          -e "s|@BASH@|${pkgs.bash}|" \
          -e "s|@COREUTILS@|${pkgs.coreutils}|" \
          -e "s|@SYSTEM@|${pkgs.stdenv.hostPlatform.system}|" \
          "$expr"
        export BALLAST_FILE_BYTES="$FILE_BYTES"
        export BALLAST_CHATTY="$CHATTY"

        started=$(date +%s)
        if [ "$ARM" = c ]; then
          # Arm C — the trigger in isolation. Drive the faked free-space number
          # down past min-free and watch the daemon act on it. Nothing is
          # written, so this proves the trigger and the arithmetic only.
          hook=${freeSpaceHookFile}
          echo "VIVARIUM_STORE_PRESSURE_HOOK_PRESENT=$([ -w "$hook" ] && echo yes || echo no)"
          # One real, collectable path so a firing has something to delete.
          # `export`, not an assignment prefix: a `VAR=x name=$(...)` line is two
          # assignments, so the prefix would never reach the subshell.
          export BALLAST_NAME=seed BALLAST_MIB=64
          seed=$($nix_build --no-out-link --option substituters "" "$expr" 2>/tmp/seed.err)
          echo "VIVARIUM_STORE_PRESSURE_SEED=''${seed:-none} status=$?"
          sample SEEDED
          for gib in 16 8 6 5 4 3 2; do
            echo $(( gib * 1073741824 )) > "$hook"
            echo "VIVARIUM_STORE_PRESSURE_HOOK_SET=''${gib}GiB"
            export BALLAST_NAME="probe$gib" BALLAST_MIB=8
            out=$($nix_build --no-out-link --option substituters "" "$expr" 2>/tmp/probe.err)
            status=$?
            # The daemon announces a firing on the client's stderr as
            # "running auto-GC to free N bytes". Grepping for it is the only
            # direct evidence the collector ran; the free-space series alone
            # shows an effect with no named cause.
            fired=$(grep -c 'running auto-GC' /tmp/probe.err)
            freed=$(grep -o 'running auto-GC to free [0-9]* bytes' /tmp/probe.err | grep -o '[0-9]*' | tail -n1)
            echo "VIVARIUM_STORE_PRESSURE_ARMC=''${gib}GiB status=$status fired=$fired want_freed=''${freed:-none} out=''${out:-none}"
            echo "VIVARIUM_STORE_PRESSURE_ARMC_ERR=''${gib}GiB $(tr '\n' '|' </tmp/probe.err | tail -c 400)"
            sample "ARMC_$gib"
          done
          # Restore the inert value so nothing downstream in this boot sees a
          # store under fake pressure.
          echo 1099511627776 > "$hook"
          echo 'VIVARIUM_STORE_PRESSURE_HOOK_RESTORED=yes'
        elif [ "$ARM" = d ]; then
          # Arm D — a real crossing on a real filesystem. The distance to the
          # threshold is shortened rather than the ballast enlarged: min-free is
          # placed just under the actual free space, so a few gibibytes cross it.
          avail=$(stat -f -c '%a * %S' ${upperRoot} | awk '{print $1 * $3}')
          minfree=$(( avail - 1610612736 ))
          # The GAP is scaled down along with the distance, not kept at
          # ADR-0089's 4 GiB. With min-free placed just under a nearly empty
          # volume's free space, a 4 GiB gap puts max-free ABOVE the device's
          # capacity, and a target the filesystem can never reach turns a
          # bounded collection into "delete everything and stop when the
          # garbage runs out" — which measures the wrong thing. A 1 GiB gap
          # keeps the collection bounded, so `GCLimitReached` is what ends it
          # and the arithmetic is observable on real bytes.
          maxfree=$(( minfree + 1073741824 ))
          [ $minfree -lt 0 ] && minfree=0
          echo "VIVARIUM_STORE_PRESSURE_ARMD_PLAN=avail=$avail min_free=$minfree max_free=$maxfree"
          # The thresholds have to reach the DAEMON. `autoGC` runs in the
          # daemon's own goal loop and reads `settings.minFree` there, so the
          # client's `--option min-free` is accepted, ignored, and reported back
          # as if it had applied. Write the additive config the daemon was told
          # to read, then restart it so the new process picks the file up.
          printf 'min-free = %s\nmax-free = %s\n' "$minfree" "$maxfree" > ${extraConfFile}
          systemctl restart nix-daemon.service
          sleep 2
          echo "VIVARIUM_STORE_PRESSURE_ARMD_DAEMON_CONF=$(tr '\n' ';' < ${extraConfFile}) restart_status=$?"
          i=0
          chunk=256
          while [ $(( i * chunk )) -lt $BALLAST_TOTAL_MIB ]; do
            now=$(date +%s)
            if [ $(( now - started )) -gt $BUDGET_SECONDS ]; then
              echo "VIVARIUM_STORE_PRESSURE_ARMD_BUDGET_EXHAUSTED=$(( now - started ))s"
              break
            fi
            # BALLAST_MIB is the size of THIS derivation; the loop's own bound is
            # BALLAST_TOTAL_MIB. They were one variable at first, and exporting
            # the chunk size overwrote the bound — so the loop ran exactly once
            # and reported "the trigger did not fire" for a run that had barely
            # written anything. Keep the two names apart.
            export BALLAST_NAME="d$i" BALLAST_MIB=$chunk
            out=$($nix_build --no-out-link --option substituters "" \
              --option min-free $minfree --option max-free $maxfree \
              "$expr" 2>/tmp/d.err)
            status=$?
            fired=$(grep -c 'running auto-GC' /tmp/d.err)
            freed=$(grep -o 'running auto-GC to free [0-9]* bytes' /tmp/d.err | grep -o '[0-9]*' | tail -n1)
            enospc=$(grep -ci 'No space left' /tmp/d.err)
            echo "VIVARIUM_STORE_PRESSURE_ARMD=$i status=$status fired=$fired want_freed=''${freed:-none} enospc=$enospc out=''${out:-none}"
            [ $status -ne 0 ] && echo "VIVARIUM_STORE_PRESSURE_ARMD_ERR=$i $(tr '\n' '|' </tmp/d.err | tail -c 400)"
            sample "ARMD_$i"
            i=$(( i + 1 ))
          done
          echo "VIVARIUM_STORE_PRESSURE_ITERATIONS=$i"
        elif [ "$ARM" = e ]; then
          # Arm E — real reclamation on real ext4, which is the one thing arms C
          # and D could not reach. C fakes free space, so it fakes `availAfterGC`
          # too; D proved the thresholds cannot be reached from outside
          # `nix.settings` at all.
          #
          # So this arm writes NOTHING to the daemon's configuration and restarts
          # nothing. It asserts the image already carries scaled thresholds and
          # refuses in two seconds otherwise, rather than spending an hour of
          # ballast measuring a daemon that was never going to collect.
          have_min=$($nix_cli config show min-free 2>/dev/null | tail -n1)
          have_max=$($nix_cli config show max-free 2>/dev/null | tail -n1)
          if [ "$have_min" != "''${EXPECT_MIN_FREE:-}" ] || [ "$have_max" != "''${EXPECT_MAX_FREE:-}" ]; then
            echo "VIVARIUM_STORE_PRESSURE_SKIPPED=thresholds-not-in-image have=$have_min/$have_max want=''${EXPECT_MIN_FREE:-unset}/''${EXPECT_MAX_FREE:-unset}"
            sample END
            echo 'VIVARIUM_STORE_PRESSURE_COMPLETE=yes'
            exit 0
          fi
          # The re-arm damper, stated as arithmetic rather than hoped past:
          # `autoGC` returns early while available space is still above 97% of
          # the previous `availAfterGC`, so a second pass needs another 3% of
          # max-free consumed. The image is built so that (max-free - min-free)
          # is an order of magnitude above that, which makes silence between
          # collections unambiguously a broken trigger rather than a damped one.
          gap=$(( have_max - have_min ))
          echo "VIVARIUM_STORE_PRESSURE_ARME_DAMPER=gap=$gap max_free=$have_max damper_rearm=$(( have_max * 3 / 100 ))"

          # --- what the collector actually decides, per path -------------------
          # `deleteFromStore` prints `deleting '<path>'` at info level for EVERY
          # path it attempts, and the daemon forwards that to the client's
          # stderr, which this arm already captures. So the attempted set is free
          # and needs no hook. What it does not say is which of upstream's two
          # branches each path took, and that is the whole question: for a path
          # absent from the upper layer `LocalOverlayStore::deleteStorePath`
          # returns without touching `bytesFreed`, while `deleteFromStore` adds
          # that untouched variable to its running total and stops at the target.
          #
          # Classification is derived from two sets this side rather than from
          # the daemon's debug output, so the reading does not depend on a log
          # level. The lower store's database holds the boot closure and nothing
          # else, so enumerating it is one cheap call, not a walk of the share.
          lower_valid=/tmp/vivarium-lower-valid
          $nix_cli path-info --store "${storeLayout.lowerStoreUri}" --all 2>/tmp/lower.err \
            | sed 's|.*/||' | sort > "$lower_valid"
          echo "VIVARIUM_STORE_PRESSURE_ARME_LOWER_VALID=$(wc -l < "$lower_valid") err=$(tail -c 200 /tmp/lower.err | tr '\n' '|')"

          # Per iteration: classify the attempted paths against the upper layer
          # as it stood BEFORE the build, and against the lower database.
          upper_before=/tmp/vivarium-upper-before
          classify() { # classify <iteration>
            local it=$1 attempted upper_lower upper_only absent
            grep -o "deleting '/nix/store/[^']*'" /tmp/e.err \
              | sed -e "s|.*/||" -e "s|'$||" | sort -u > /tmp/vivarium-attempted
            attempted=$(wc -l < /tmp/vivarium-attempted)
            if [ "$attempted" -eq 0 ]; then
              return 0
            fi
            # In the upper layer before the collection: the collector could act.
            comm -12 /tmp/vivarium-attempted "$upper_before" > /tmp/vivarium-att-upper
            # Of those, valid in the lower database: deleted through the upper
            # layer and remounted. Otherwise deleted through the merged view,
            # leaving a whiteout. Absent from the upper layer: a no-op.
            upper_lower=$(comm -12 /tmp/vivarium-att-upper "$lower_valid" | wc -l)
            upper_only=$(comm -23 /tmp/vivarium-att-upper "$lower_valid" | wc -l)
            absent=$(comm -23 /tmp/vivarium-attempted "$upper_before" | wc -l)
            echo "VIVARIUM_STORE_PRESSURE_ARME_PATHS=$it attempted=$attempted upper_and_lower_valid=$upper_lower upper_only=$upper_only absent_from_upper=$absent stopped_at_target=$(grep -c 'deleted more than' /tmp/e.err)"
            # A bounded sample per class, so a reader can check the counts
            # against real names without 25,000 lines on the console.
            echo "VIVARIUM_STORE_PRESSURE_ARME_SAMPLE=$it absent_from_upper $(comm -23 /tmp/vivarium-attempted "$upper_before" | head -n 20 | tr '\n' ' ')"
            echo "VIVARIUM_STORE_PRESSURE_ARME_SAMPLE=$it in_upper $(head -n 20 /tmp/vivarium-att-upper | tr '\n' ' ')"
          }

          i=0
          chunk=256
          while [ $(( i * chunk )) -lt $BALLAST_TOTAL_MIB ]; do
            now=$(date +%s)
            if [ $(( now - started )) -gt $BUDGET_SECONDS ]; then
              echo "VIVARIUM_STORE_PRESSURE_ARME_BUDGET_EXHAUSTED=$(( now - started ))s"
              break
            fi
            export BALLAST_NAME="e$i" BALLAST_MIB=$chunk
            # The upper layer as it stands BEFORE this build, which is the state
            # the collector will see if it fires during it.
            ls -U ${upperLayer} 2>/dev/null | sort > "$upper_before"
            # No `--option min-free`: it does not reach the collector, and
            # passing it here would make this arm indistinguishable from arm D.
            out=$($nix_build --no-out-link --option substituters "" "$expr" 2>/tmp/e.err)
            status=$?
            classify "$i"
            fired=$(grep -c 'running auto-GC' /tmp/e.err)
            want=$(grep -o 'running auto-GC to free [0-9]* bytes' /tmp/e.err | grep -o '[0-9]*' | tail -n1)
            enospc=$(grep -ci 'No space left' /tmp/e.err)
            echo "VIVARIUM_STORE_PRESSURE_ARME=$i status=$status fired=$fired want_freed=''${want:-none} enospc=$enospc out=''${out:-none}"
            [ $status -ne 0 ] && echo "VIVARIUM_STORE_PRESSURE_ARME_ERR=$i $(tr '\n' '|' </tmp/e.err | tail -c 400)"
            # An inode ceiling reached before a block ceiling would read as "the
            # collector failed to reclaim" and it is not. `%d` is free file
            # nodes; the host fails the run's *interpretation*, not the run.
            sample "ARME_$i"
            i=$(( i + 1 ))
          done
          echo "VIVARIUM_STORE_PRESSURE_ITERATIONS=$i"

          # --- the discard leg, riding this arm's boot ------------------------
          # Every layer of the chain is already verified in source; only the
          # number is missing. Emit the guest's own view of each layer FIRST, so
          # a null result decomposes into "not advertised" / "advertised but ext4
          # issued nothing" / "issued but the host did not account" — three
          # findings with three different homes — instead of one silence.
          echo "VIVARIUM_STORE_PRESSURE_DISCARD_LSBLK=$(lsblk -D -n -o NAME,DISC-ALN,DISC-GRAN,DISC-MAX /dev/vdb 2>/dev/null | tr -s ' ' | tr '\n' ';')"
          echo "VIVARIUM_STORE_PRESSURE_DISCARD_MAX_BYTES=$(cat /sys/block/vdb/queue/discard_max_bytes 2>/dev/null)"
          echo "VIVARIUM_STORE_PRESSURE_DISCARD_GRANULARITY=$(cat /sys/block/vdb/queue/discard_granularity 2>/dev/null)"
          echo "VIVARIUM_STORE_PRESSURE_MOUNT_OPTS=$(awk '$2 == "${upperRoot}" { print $4 }' /proc/self/mounts 2>/dev/null)"
          sample BEFORE_FSTRIM
          # `discard` is deliberately NOT a mount option: a continuously
          # discarding mount would punch holes throughout the run and destroy the
          # host block series this measurement reads. `fstrim` is an ioctl.
          #
          # Its own reported figure is the length of the ranges handed to the
          # kernel, NOT host bytes freed — the headline number is the host
          # image's allocated-block delta. This line is context, not the result.
          trim_out=$(fstrim -v ${upperRoot} 2>&1)
          echo "VIVARIUM_STORE_PRESSURE_FSTRIM=$trim_out"
          sample AFTER_FSTRIM
          # The host samples the image every ten seconds. Hold long enough for a
          # clean post-trim reading rather than trying to align two clocks that
          # have no synchronisation guarantee between them.
          sleep 30
          sample AFTER_FSTRIM_SETTLED

          # --- the discard chain, isolated from the collector -----------------
          # The trim above can only return blocks the FILESYSTEM freed, so if the
          # collector freed nothing there is nothing to punch and a null host
          # delta says nothing about discard. That is a real confound and it is
          # separated here rather than argued away.
          #
          # This probe owes nothing to Nix: write a plain file into the store
          # volume, sync, delete it, sync, trim. Whatever the host image does
          # across those four points is the discard chain end to end — the
          # question `todo.md` leg (e) actually asks.
          probe=${upperRoot}/vivarium-discard-probe
          dd if=/dev/zero of="$probe" bs=1M count=1024 status=none conv=fsync
          sync
          sample DISCARD_PROBE_WRITTEN
          echo "VIVARIUM_STORE_PRESSURE_DISCARD_PROBE=written bytes=$(stat -c %s "$probe" 2>/dev/null) blocks=$(stat -c %b "$probe" 2>/dev/null)"
          # The host sampler runs every ten seconds; hold so the written peak is
          # sampled before the delete removes it again.
          sleep 25
          rm -f "$probe"
          sync
          sample DISCARD_PROBE_DELETED
          trim2=$(fstrim -v ${upperRoot} 2>&1)
          echo "VIVARIUM_STORE_PRESSURE_DISCARD_PROBE_FSTRIM=$trim2"
          sync
          sample DISCARD_PROBE_TRIMMED
          sleep 30
          sample DISCARD_PROBE_SETTLED
        else
          echo "VIVARIUM_STORE_PRESSURE_SKIPPED=unknown-arm-$ARM"
        fi

        sample END
        # The lazy inode table's progress, read from the filesystem rather than
        # inferred from the host image's allocated blocks.
        echo "VIVARIUM_STORE_PRESSURE_ITABLE_ZEROED_END=$(dumpe2fs /dev/vdb 2>/dev/null | grep -c ITABLE_ZEROED)"
        echo 'VIVARIUM_STORE_PRESSURE_COMPLETE=yes'
      '';
    };
  };
}
