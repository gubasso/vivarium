# The build-to-launch contract, asserted against two *independently realised*
# artifacts — the launcher's own JSON and the guest's own `/etc/fstab` — rather
# than against two copies of one Nix constant, which cannot disagree.
#
# Parameterised by variant (ADR-0095) so every image gets its own contract
# rather than only the shipped one. Every assertion below is about the store
# topology, so every one holds for every variant unchanged; `expect` carries
# only the numbers a variant is allowed to move.
{
  pkgs,
  guest,
  runner,
  volumeLabel,
  storeVolumeLabel,
  expect,
}:

pkgs.runCommand "vivarium-first-microvm-contract"
  {
    nativeBuildInputs = [
      pkgs.binutils
      pkgs.jq
    ];
  }
  ''
    set -eu
    # The volume label is a build-to-launch contract that only a boot test would
    # otherwise catch: the launcher's mkfs applies it, and the guest resolves the
    # volume through /dev/disk/by-label/<label>. Compare the two independently
    # realised artifacts — the launcher's own JSON and the guest's own fstab —
    # rather than two copies of one Nix constant, which cannot disagree.
    launcher_json=$(grep -oE '/nix/store/[a-z0-9]+-vivarium-first-microvm-launch-arguments\.json' \
      ${runner}/bin/vivarium-first-microvm | head -n1)
    test -n "$launcher_json"
    test "$(jq -r .schemaVersion "$launcher_json")" = 1
    test "$(jq -r .descriptorBudget.limit "$launcher_json")" = 524288
    test "$(jq -r .descriptorBudget.workerPoolSize "$launcher_json")" = ${toString expect.virtiofsdThreadPoolSize}
    test "$(jq -r .socketLegs.api "$launcher_json")" = '@API_SOCKET@'
    test "$(jq -r .socketLegs.console "$launcher_json")" = '@CONSOLE_SOCKET@'
    test "$(jq -r .vmCreate.serial.mode "$launcher_json")" = Socket
    test "$(jq -r .vmCreate.console.mode "$launcher_json")" = Off
    test "$(jq -r .vmCreate.landlock_enable "$launcher_json")" = true
    test "$(jq -r '.vmCreate.fs | length' "$launcher_json")" = "$(jq -r '.shareLaunch | length' "$launcher_json")"
    grep -qF 'exec ' ${runner}/bin/vivarium-first-microvm
    ! grep -qF -- '--no-landlock' ${runner}/bin/vivarium-first-microvm
    ! grep -Eq 'pids=|trap .*EXIT|wait .*pid|ulimit -n' ${runner}/bin/vivarium-first-microvm
    supervisor=$(jq -r .supervisor "$launcher_json")
    test -x "$supervisor"
    strings "$supervisor" > supervisor.strings
    grep -qF -- '--bounding-set=-all' supervisor.strings
    grep -qF -- '--ambient-caps=-all' supervisor.strings
    grep -qF -- '--inh-caps=-all' supervisor.strings
    grep -qF -- '--rlimit-nofile=' supervisor.strings
    fstab=${guest.config.system.build.toplevel}/etc/fstab
    for label in ${volumeLabel} ${storeVolumeLabel}; do
      grep -F "\"label\":\"$label\"" "$launcher_json"
      grep -F "/dev/disk/by-label/$label" "$fstab"
    done
    # Order is a contract between cloud-hypervisor's `--disk` sequence and
    # microvm.nix's drive-letter assignment, so assert the sequence itself rather
    # than only that both rows exist.
    test "$(jq -r '[.volumeLaunch[].label] | join(",")' "$launcher_json")" = '${volumeLabel},${storeVolumeLabel}'
    # ADR-0091: exactly one volume is provisioned for inodes, and it is the one
    # whose mount point is the writable store overlay.
    test "$(jq -r '[.volumeLaunch[] | select(.inodeRatio != null) | .argName] | join(",")' "$launcher_json")" = store-volume
    test "$(jq -r '.volumeLaunch[] | select(.argName == "store-volume") | .inodeRatio' "$launcher_json")" = 8192
    # ADR-0087: the store volume backs the writable layer, so the guest must
    # resolve it at ${storeVolumeLabel} and mount it before /nix/store exists.
    awk '$2 == "/nix/.rw-store"' "$fstab" | grep -qF '/dev/disk/by-label/${storeVolumeLabel}'
    # ADR-0088's interposed overlay, without which the read-only lower store
    # cannot create the .links directory LocalStore makes unconditionally.
    grep -E '^overlay[[:space:]]+/nix/\.local-overlay-lower-store[[:space:]]+overlay' "$fstab"
    # The daemon must be pointed at a local-overlay store through an
    # EnvironmentFile — never Environment=, where systemd would read the URI's
    # percent-encoding as unit specifiers.
    # nix ships its own nix-daemon.service, so NixOS renders every override into
    # a drop-in beside it rather than into the unit; read both.
    daemon_units=$(echo ${guest.config.system.build.toplevel}/etc/systemd/system/nix-daemon.service \
      ${guest.config.system.build.toplevel}/etc/systemd/system/nix-daemon.service.d/*.conf)
    environment_file=$(cat $daemon_units | sed -n 's/^EnvironmentFile=//p' | head -n1)
    test -n "$environment_file"
    grep -qF 'NIX_REMOTE=local-overlay://' "$environment_file"
    ! cat $daemon_units | grep -qE '^Environment=.*NIX_REMOTE'
    # Both flags: the lower store's read-only=true parameter sits behind the
    # second one, and enabling only the first fails at daemon start.
    for feature in local-overlay-store read-only-local-store; do
      grep -qE "^experimental-features = .*\b$feature\b" ${guest.config.system.build.toplevel}/etc/nix/nix.conf
    done
    # Per-share virtiofsd policy must reach the launcher from the guest module.
    grep -F '"cache":"always"' "$launcher_json"
    grep -F '"cache":"auto"' "$launcher_json"
    grep -E '^overlay[[:space:]]+/nix/store[[:space:]]+overlay' "$fstab"
    # spec/06:22 — the read-only share must be read-only inside the guest too,
    # which upstream's generated `defaults` does not give us.
    ro_store_options=$(awk '$2 == "/nix/.ro-store" { print $4 }' "$fstab")
    for flag in ro nodev nosuid noexec; do
      case ",$ro_store_options," in
        *",$flag,"*) ;;
        *) echo "read-only store mount is missing $flag: $ro_store_options" >&2; exit 1 ;;
      esac
    done
    # The regression guard for ADR-0088's one divergence from the prior art:
    # forcing `writableStoreOverlay` to null — as the vendored module does —
    # makes upstream emit its own `What=store` drop-in in place of this one, and
    # this line is what would catch it.
    # ADR-0085: the three canary expressions must stay three. A copy-paste that
    # re-collided any pair would silently make the store spike and the GC-interlock
    # experiment operate on one path — each deleting the other's subject — and
    # nothing else in the tree would notice.
    test "$(jq -r '[.storeCanaryExpression, .gcInterlockCanaryExpression, .gcInterlockControlExpression] | unique | length' "$launcher_json")" = 3
    grep -F 'What=overlay' ${guest.config.system.build.toplevel}/etc/systemd/system/nix-store.mount.d/overrides.conf
    grep -F 'DefaultDependencies=false' ${guest.config.system.build.toplevel}/etc/systemd/system/nix-store.mount.d/overrides.conf
    # --- variant-specific ---------------------------------------------------
    # The thresholds must be *in the image*. Neither a client-side
    # `--option min-free` nor `NIX_USER_CONF_FILES` on the daemon unit reaches
    # `LocalStore::autoGC` — measured, and it cost a boot that reported "the
    # trigger did not fire" for a store that had never approached the real
    # threshold. This turns that failure into a build error.
    grep -qE '^min-free = ${toString expect.storeMinFree}$' ${guest.config.system.build.toplevel}/etc/nix/nix.conf
    grep -qE '^max-free = ${toString expect.storeMaxFree}$' ${guest.config.system.build.toplevel}/etc/nix/nix.conf
    test "$(jq -r '.volumeLaunch[] | select(.argName == "store-volume") | .sizeMiB' "$launcher_json")" = ${toString expect.storeVolumeSizeMiB}
    test "$(jq -r '.virtiofsdThreadPoolSize' "$launcher_json")" = ${toString expect.virtiofsdThreadPoolSize}

    # --- what this image is allowed to boot ---------------------------------
    # An allowlist, not a denylist: the set of vivarium units in the image must
    # equal exactly the set this variant selected. A denylist naming today's
    # four probes would pass for a probe added tomorrow, which is the failure
    # this assertion exists to prevent.
    units=$(cd ${guest.config.system.build.toplevel}/etc/systemd/system && ls -1 vivarium-*.service 2>/dev/null | sort | tr '\n' ' ')
    expected=${
      pkgs.lib.escapeShellArg (
        pkgs.lib.concatMapStrings (u: "${u} ") (
          pkgs.lib.sort (a: b: a < b) ([ "vivarium-volume-prepare.service" ] ++ expect.units)
        )
      )
    }
    if [ "$units" != "$expected" ]; then
      echo "unit set mismatch:" >&2
      echo "  got:      $units" >&2
      echo "  expected: $expected" >&2
      exit 1
    fi
    # The upstream Nix *test* hook is a behaviour switch reachable by anything
    # that can write /run/vivarium/nix-free-space. It belongs to the pressure
    # leg, never to a shipped daemon (ADR-0095).
    daemon_dropins=${guest.config.system.build.toplevel}/etc/systemd/system/nix-daemon.service.d
    if [ ${if expect.units == [ ] then "true" else "false"} = true ]; then
      ! grep -rq '_NIX_TEST_FREE_SPACE_FILE' "$daemon_dropins" 2>/dev/null
      ! grep -rq 'nix-free-space' ${guest.config.system.build.toplevel}/etc/tmpfiles.d 2>/dev/null
    fi
    touch "$out"
  ''
