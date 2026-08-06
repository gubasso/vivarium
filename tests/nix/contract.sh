# shellcheck shell=bash
set -eu
# The volume label is a build-to-launch contract that only a boot test would
# otherwise catch: the launcher's mkfs applies it, and the guest resolves the
# volume through /dev/disk/by-label/<label>. Compare the two independently
# realised artifacts — the launcher's own JSON and the guest's own fstab —
# rather than two copies of one Nix constant, which cannot disagree.
launcher_json=$(grep -oE '/nix/store/[a-z0-9]+-vivarium-first-microvm-launch-arguments\.json' \
  "$VIVARIUM_RUNNER/bin/vivarium-first-microvm" | head -n1)
test -n "$launcher_json"
test "$(jq -r .schemaVersion "$launcher_json")" = 1
test "$(jq -r .descriptorBudget.limit "$launcher_json")" = 524288
test "$(jq -r .descriptorBudget.workerPoolSize "$launcher_json")" = "$VIVARIUM_VIRTIOFSD_THREAD_POOL_SIZE"
test "$(jq -r .socketLegs.api "$launcher_json")" = '@API_SOCKET@'
test "$(jq -r .socketLegs.console "$launcher_json")" = '@CONSOLE_SOCKET@'
test "$(jq -r .vmCreate.serial.mode "$launcher_json")" = Socket
test "$(jq -r .vmCreate.console.mode "$launcher_json")" = Off
test "$(jq -r .vmCreate.landlock_enable "$launcher_json")" = true
test "$(jq -r '.vmCreate.fs | length' "$launcher_json")" = "$(jq -r '.shareLaunch | length' "$launcher_json")"
grep -qF 'exec ' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"
if grep -qF -- '--no-landlock' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"; then exit 1; fi
if grep -Eq 'pids=|trap .*EXIT|wait .*pid|ulimit -n' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"; then exit 1; fi
supervisor=$(jq -r .supervisor "$launcher_json")
test -x "$supervisor"
strings "$supervisor" >supervisor.strings
grep -qF -- '--bounding-set=-all' supervisor.strings
grep -qF -- '--ambient-caps=-all' supervisor.strings
grep -qF -- '--inh-caps=-all' supervisor.strings
grep -qF -- '--no-new-privs' supervisor.strings
grep -qF -- '--rlimit-nofile=' supervisor.strings
fstab=$VIVARIUM_GUEST_SYSTEM/etc/fstab
for label in $VIVARIUM_VOLUME_LABEL $VIVARIUM_STORE_VOLUME_LABEL; do
  grep -F "\"label\":\"$label\"" "$launcher_json"
  grep -F "/dev/disk/by-label/$label" "$fstab"
done
# Order is a contract between cloud-hypervisor's `--disk` sequence and
# microvm.nix's drive-letter assignment, so assert the sequence itself rather
# than only that both rows exist.
test "$(jq -r '[.volumeLaunch[].label] | join(",")' "$launcher_json")" = "$VIVARIUM_VOLUME_LABEL,$VIVARIUM_STORE_VOLUME_LABEL"
# ADR-0091: exactly one volume is provisioned for inodes, and it is the one
# whose mount point is the writable store overlay.
test "$(jq -r '[.volumeLaunch[] | select(.inodeRatio != null) | .argName] | join(",")' "$launcher_json")" = store-volume
test "$(jq -r '.volumeLaunch[] | select(.argName == "store-volume") | .inodeRatio' "$launcher_json")" = 8192
# ADR-0087: the store volume backs the writable layer, so the guest must
# resolve it at $VIVARIUM_STORE_VOLUME_LABEL and mount it before /nix/store exists.
awk '$2 == "/nix/.rw-store"' "$fstab" | grep -qF "/dev/disk/by-label/$VIVARIUM_STORE_VOLUME_LABEL"
# ADR-0088's interposed overlay, without which the read-only lower store
# cannot create the .links directory LocalStore makes unconditionally.
grep -E '^overlay[[:space:]]+/nix/\.local-overlay-lower-store[[:space:]]+overlay' "$fstab"
# The daemon must be pointed at a local-overlay store through an
# EnvironmentFile — never Environment=, where systemd would read the URI's
# percent-encoding as unit specifiers.
# nix ships its own nix-daemon.service, so NixOS renders every override into
# a drop-in beside it rather than into the unit; read both.
daemon_unit=$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/nix-daemon.service
daemon_dropins=("$VIVARIUM_GUEST_SYSTEM"/etc/systemd/system/nix-daemon.service.d/*.conf)
# Test the first element exists, not the element count. With `nullglob` off an
# unmatched glob leaves the literal pattern in the array, so a count is never
# zero and the assertion could not fail — the shape this file exists to delete.
test -e "${daemon_dropins[0]}"
daemon_units=("$daemon_unit" "${daemon_dropins[@]}")
cat "${daemon_units[@]}" >daemon-units
environment_file=$(sed -n 's/^EnvironmentFile=//p' daemon-units | head -n1)
test -n "$environment_file"
grep -qF 'NIX_REMOTE=local-overlay://' "$environment_file"
if grep -qE '^Environment=.*NIX_REMOTE' daemon-units; then exit 1; fi
# Both flags: the lower store's read-only=true parameter sits behind the
# second one, and enabling only the first fails at daemon start.
for feature in local-overlay-store read-only-local-store; do
  grep -qE "^experimental-features = .*\b$feature\b" "$VIVARIUM_GUEST_SYSTEM/etc/nix/nix.conf"
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
    *)
      echo "read-only store mount is missing $flag: $ro_store_options" >&2
      exit 1
      ;;
  esac
done
# ADR-0085: the three canary expressions must stay three. A copy-paste that
# re-collided any pair would silently make the store spike and the GC-interlock
# experiment operate on one path — each deleting the other's subject — and
# nothing else in the tree would notice.
test "$(jq -r '[.storeCanaryExpression, .gcInterlockCanaryExpression, .gcInterlockControlExpression] | unique | length' "$launcher_json")" = 3
# The regression guard for ADR-0088's one divergence from the prior art:
# forcing `writableStoreOverlay` to null — as the vendored module does —
# makes upstream emit its own `What=store` drop-in in place of this one, and
# these lines are what would catch it.
grep -F 'What=overlay' "$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/nix-store.mount.d/overrides.conf"
grep -F 'DefaultDependencies=false' "$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/nix-store.mount.d/overrides.conf"
# --- variant-specific ---------------------------------------------------
# The thresholds must be *in the image*. Neither a client-side
# `--option min-free` nor `NIX_USER_CONF_FILES` on the daemon unit reaches
# `LocalStore::autoGC` — measured, and it cost a boot that reported "the
# trigger did not fire" for a store that had never approached the real
# threshold. This turns that failure into a build error.
grep -qE "^min-free = $VIVARIUM_STORE_MIN_FREE$" "$VIVARIUM_GUEST_SYSTEM/etc/nix/nix.conf"
grep -qE "^max-free = $VIVARIUM_STORE_MAX_FREE$" "$VIVARIUM_GUEST_SYSTEM/etc/nix/nix.conf"
test "$(jq -r '.volumeLaunch[] | select(.argName == "store-volume") | .sizeMiB' "$launcher_json")" = "$VIVARIUM_STORE_VOLUME_SIZE_MIB"
test "$(jq -r '.virtiofsdThreadPoolSize' "$launcher_json")" = "$VIVARIUM_VIRTIOFSD_THREAD_POOL_SIZE"

# --- what this image is allowed to boot ---------------------------------
# An allowlist, not a denylist: the set of vivarium units in the image must
# equal exactly the set this variant selected. A denylist naming today's
# four probes would pass for a probe added tomorrow, which is the failure
# this assertion exists to prevent.
units=$(cd "$VIVARIUM_GUEST_SYSTEM/etc/systemd/system" && find . -maxdepth 1 -name 'vivarium-*.service' -printf '%f\n' | sort | tr '\n' ' ')
expected=$VIVARIUM_EXPECTED_UNITS
if [ "$units" != "$expected" ]; then
  echo "unit set mismatch:" >&2
  echo "  got:      $units" >&2
  echo "  expected: $expected" >&2
  exit 1
fi
# The upstream Nix *test* hook is a behaviour switch reachable by anything
# that can write /run/vivarium/nix-free-space. It belongs to the pressure
# leg, never to a shipped daemon (ADR-0095).
daemon_dropin_dir=$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/nix-daemon.service.d
if [ -n "$VIVARIUM_EXPECT_NO_UNITS" ]; then
  if grep -rq '_NIX_TEST_FREE_SPACE_FILE' "$daemon_dropin_dir" 2>/dev/null; then exit 1; fi
  if grep -rq 'nix-free-space' "$VIVARIUM_GUEST_SYSTEM/etc/tmpfiles.d" 2>/dev/null; then exit 1; fi
fi
# Nix's runCommand builder supplies `out`.
# shellcheck disable=SC2154
touch "$out"
