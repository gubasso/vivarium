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
test "$(jq -r .schemaVersion "$launcher_json")" = 7
test "$(jq -r .descriptorBudget.limit "$launcher_json")" = 524288
test "$(jq -r .descriptorBudget.workerPoolSize "$launcher_json")" = "$VIVARIUM_VIRTIOFSD_THREAD_POOL_SIZE"
test "$(jq -r .socketLegs.api "$launcher_json")" = '@API_SOCKET@'
test "$(jq -r .socketLegs.console "$launcher_json")" = '@CONSOLE_SOCKET@'
test "$(jq -r .tokens.controlSocket "$launcher_json")" = '@CONTROL_SOCKET@'
test "$(jq -r .vmCreate.vsock.cid "$launcher_json")" = 3
test "$(jq -r .vmCreate.vsock.socket "$launcher_json")" = '@CONTROL_SOCKET@'
test "$(jq -r .vmCreate.serial.mode "$launcher_json")" = Socket
test "$(jq -r .vmCreate.console.mode "$launcher_json")" = Off
test "$(jq -r .vmCreate.landlock_enable "$launcher_json")" = true
test "$(jq -r '.vmCreate.fs | length' "$launcher_json")" = "$(jq -r '.shareLaunch | length' "$launcher_json")"
# Guest networking (schema 6): the two renderings of the one NIC agree with the
# network object the supervisor reads, and the shipped image defaults to spec/05's
# open egress with no allowlist entries.
test "$(jq -r '.vmCreate.net | length' "$launcher_json")" = 1
test "$(jq -r '.vmCreate.net[0].tap' "$launcher_json")" = "$(jq -r .network.tapName "$launcher_json")"
test "$(jq -r '.vmCreate.net[0].mac' "$launcher_json")" = "$(jq -r .network.guestMac "$launcher_json")"
test "$(jq -r .egress.mode "$launcher_json")" = open
test "$(jq -r '.egress.allow | length' "$launcher_json")" = 0
# The uplink's DNS forward must sit outside the guest link's subnet, or the guest
# resolves it on the local link where nothing answers.
test "$(jq -r .network.dnsForwardAddress "$launcher_json")" != "$(jq -r .network.gatewayAddress "$launcher_json")"
# ADR-0102: no host-side vivarium program comes from this build. The runner's job
# ends at writing the launch specification — it execs nothing and names no built
# `viv` — and the supervisor enters the specification from the running
# installation at launch time, so the build JSON carries no `.supervisor`.
if grep -qE '(^|[^a-z])exec |@vivPath@' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"; then exit 1; fi
test "$(jq -r '.supervisor // "absent"' "$launcher_json")" = absent
if grep -qF -- '--no-landlock' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"; then exit 1; fi
if grep -Eq 'pids=|trap .*EXIT|wait .*pid|ulimit -n' "$VIVARIUM_RUNNER/bin/vivarium-first-microvm"; then exit 1; fi
# The stable path spec/10's pre-boot refusal reads, agreeing with the launcher's
# own JSON — two artifacts of one build.
test "$(cat "$VIVARIUM_RUNNER/share/vivarium/launch-contract-schema")" = "$(jq -r .schemaVersion "$launcher_json")"
agent_unit=$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/vivarium-agent.service
grep -qF 'User=vivarium' "$agent_unit"
grep -qF 'Group=vivarium' "$agent_unit"
grep -qF 'RuntimeDirectory=vivarium' "$agent_unit"
grep -qF 'RuntimeDirectoryMode=0700' "$agent_unit"
grep -qF 'UMask=0077' "$agent_unit"
grep -qF 'NoNewPrivileges=true' "$agent_unit"
grep -qE '^CapabilityBoundingSet=$' "$agent_unit"
grep -qE '^AmbientCapabilities=$' "$agent_unit"
agent_executable=$(sed -n 's/^ExecStart=\([^ ]*\).*/\1/p' "$agent_unit")
test -x "$agent_executable"
strings "$agent_executable" >agent.strings
grep -qF '/run/vivarium/ssh-agent.sock' agent.strings
grep -qF '/run/vivarium/gpg-agent.sock' agent.strings
if grep -Rq 'control\.sock_[0-9]' "$VIVARIUM_RUNNER" "$VIVARIUM_GUEST_SYSTEM"; then exit 1; fi
fstab=$VIVARIUM_GUEST_SYSTEM/etc/fstab
# Every share's mount point is a build-to-launch contract of the same kind as the
# volume labels below: the guest's own fstab enacts it. Compare the two artifacts
# rather than two copies of one Nix string.
#
# Still total over every share, which ADR-0100 is the reason to say out loud. The
# workspace's session-visible path moved to the host's own, and the temptation was
# to except that share here; instead `mountPoint` kept meaning "what the fstab
# mounts" and the mirror became a separate assertion below. An exception in this
# loop would have been a share nothing compares.
checked_mounts=0
while read -r tag mount_point; do
  test -n "$mount_point"
  awk -v want="$mount_point" '$2 == want { found = 1 } END { exit !found }' "$fstab" \
    || {
      echo "share $tag declares $mount_point, which the guest fstab does not mount" >&2
      exit 1
    }
  checked_mounts=$((checked_mounts + 1))
done < <(jq -r '.shareLaunch[] | "\(.tag) \(.mountPoint)"' "$launcher_json")
# A loop over an empty list passes without asserting anything, which is the shape
# of a check that reports on a field it stopped reading.
test "$checked_mounts" = "$(jq -r '.shareLaunch | length' "$launcher_json")"
test "$checked_mounts" -ge 1
# ADR-0100's guest half, asserted on the built unit rather than on a boot.
#
# The mirror is what puts the project where a session expects it, and every way it
# can fail quietly is checked here, because none of them fails loudly at runtime.
mirror_unit=$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/vivarium-workspace.service
test -f "$mirror_unit"
# The share this binds must be the share the launcher serves. Two independently
# realised artifacts again: the unit's own requirement against the launcher JSON.
workspace_mount_point=$(jq -r '.shareLaunch[] | select(.tag == "workspace") | .mountPoint' "$launcher_json")
test "$workspace_mount_point" = "$VIVARIUM_WORKSPACE_INTERNAL"
grep -qF "RequiresMountsFor=$VIVARIUM_WORKSPACE_INTERNAL" "$mirror_unit"
# A mount namespace of its own would make the bind invisible to every other
# process: the unit would succeed, log nothing, and change nothing. This is the
# only thing standing between that outcome and a later blanket-hardening pass, so
# it is an exact denial rather than a spot check.
if grep -qE '^(PrivateMounts|ProtectSystem|ProtectHome|PrivateTmp|PrivateDevices|ReadOnlyPaths|ProtectKernelTunables|RootDirectory|MountAPIVFS)=' "$mirror_unit"; then exit 1; fi
# spec/12 starts every session in the workspace, so an agent that can accept one
# before the mirror exists hands out a cwd that does not exist.
grep -qF 'vivarium-workspace.service' <<<"$(sed -n 's/^Requires=//p' "$agent_unit")"
grep -qF 'vivarium-workspace.service' <<<"$(sed -n 's/^After=//p' "$agent_unit")"
# And spec/06 requires the first-boot home ownership applied before the agent
# accepts a session. Both units are `WantedBy=multi-user.target`, so without an
# ordering edge which one wins is undefined — and the losing order hands a session
# a home its own user cannot write. Asserted here because the failure is a race:
# it would pass a boot test most of the time.
grep -qF 'vivarium-volume-prepare.service' <<<"$(sed -n 's/^Requires=//p' "$agent_unit")"
grep -qF 'vivarium-volume-prepare.service' <<<"$(sed -n 's/^After=//p' "$agent_unit")"
# Not under `/run/vivarium`, which `RuntimeDirectory=vivarium` on the agent has
# systemd delete whenever that unit restarts.
case $VIVARIUM_WORKSPACE_INTERNAL in /run/vivarium | /run/vivarium/*) exit 1 ;; esac
# The host refuses to mirror a project onto this path, and its copy of the
# literal lives in the installed binary, which this build no longer carries
# (ADR-0102). The cross-language pairing moved with it: a unit test in
# `src/launch/spec.rs` compares the crate constant against the embedded
# `nix/default.nix` text the binary ships.

# The guest process environment (spec/12). The agent clears the environment before
# every spawn, so a session gets exactly what the launcher carries here — and a
# `PATH` that disagreed with the guest's own would surface as a missing program
# rather than as a missing variable. Sourced from the guest's own
# `/etc/set-environment`, which is what a login in that guest would run, so this
# compares two independently realised artifacts.
session_home=$(jq -r .guestSession.home "$launcher_json")
session_user=$(jq -r .guestSession.user "$launcher_json")
# Absolute, because `env -i` takes away the PATH that would have found it — and an
# empty environment is the whole point: what the guest's own script sets has to be
# the only thing in the answer.
bash_path=$(command -v bash)
# shellcheck disable=SC2016  # `$0` and `$PATH` are the inner shell's, on purpose.
guest_path=$(env -i HOME="$session_home" USER="$session_user" \
  "$bash_path" --norc --noprofile -c '. "$0" >/dev/null 2>&1; printf %s "$PATH"' \
  "$VIVARIUM_GUEST_SYSTEM/etc/set-environment")
test "$(jq -r .guestSession.path "$launcher_json")" = "$guest_path"
# The shell a session reports is the executable, not the package that holds it.
test -x "$(jq -r .guestSession.shell "$launcher_json")"
for label in $VIVARIUM_VOLUME_LABEL $VIVARIUM_STORE_VOLUME_LABEL; do
  grep -F "\"label\":\"$label\"" "$launcher_json"
  grep -F "/dev/disk/by-label/$label" "$fstab"
done
# Order is a contract between cloud-hypervisor's `--disk` sequence and
# microvm.nix's drive-letter assignment, so assert the sequence itself rather
# than only that both rows exist.
test "$(jq -r '[.volumeLaunch[].label] | join(",")' "$launcher_json" | cut -d, -f1,2)" = "$VIVARIUM_VOLUME_LABEL,$VIVARIUM_STORE_VOLUME_LABEL"
# The two reserved volumes lead, in that order, whatever a layer declared after
# them. Asserted on `role` as well as on labels because the labels above are this
# image's; the roles are every image's, so a variant that declares volumes still
# has to keep the home volume's drive letter.
test "$(jq -r '[.volumeLaunch[].role] | join(",")' "$launcher_json" | cut -d, -f1,2)" = home,store
# Each image path is the launch-channel directory token joined to a name the
# build owns, and no two volumes may resolve to one file — upstream requires the
# `image` field to be unique, and a collision here is two disks over one image.
test "$(jq -r '[.volumeLaunch[] | select(.imagePath | startswith("@VOLUME_DIR@/") | not)] | length' "$launcher_json")" = 0
test "$(jq -r '[.volumeLaunch[].imagePath] | length' "$launcher_json")" \
  = "$(jq -r '[.volumeLaunch[].imagePath] | unique | length' "$launcher_json")"
test "$(jq -r '[.volumeLaunch[] | select(.imagePath != "@VOLUME_DIR@/" + .name + ".img")] | length' "$launcher_json")" = 0

# --- what a layer declared, end to end ----------------------------------
# The first-boot ownership half, read out of the built unit and the file it
# names rather than out of the module that wrote them. The table's path is taken
# from the unit's own `Environment=` line, so a unit that stopped passing it — or
# passed a different one — fails here instead of at a boot.
prepare_unit=$VIVARIUM_GUEST_SYSTEM/etc/systemd/system/vivarium-volume-prepare.service
prepare_table=$(sed -n 's/^Environment="VIVARIUM_VOLUME_TABLE=\(.*\)"$/\1/p' "$prepare_unit")
test -r "$prepare_table"

# Total rather than sampled, the way the share loop above is: every declared
# volume is checked, and the count of checked rows is compared against both the
# expectation and the launcher's own list. Without the count a loop over an empty
# list asserts nothing, and every image that declares no volume would report this
# whole section green — which is what "the assertion passed vacuously" looks like
# when it is not caught.
declared_seen=0
while read -r name mount label size_mib; do
  [ -n "$name" ] || continue
  # The launcher's JSON: the entry exists, is not one of the two reserved roles,
  # carries the label the guest will resolve by, and takes its ceiling from the
  # declaration (or from the per-volume default when it declared none).
  test "$(jq -r --arg n "$name" '.volumeLaunch[] | select(.name == $n) | .role' "$launcher_json")" = declared
  test "$(jq -r --arg n "$name" '.volumeLaunch[] | select(.name == $n) | .label' "$launcher_json")" = "$label"
  test "$(jq -r --arg n "$name" '.volumeLaunch[] | select(.name == $n) | .sizeMiB' "$launcher_json")" = "$size_mib"
  # ext4 truncates a longer label and still exits 0, which would leave the guest
  # waiting on a by-label device that never appears.
  test "${#label}" -le 16
  # The guest's own fstab, realised independently of the JSON above: this is the
  # half that proves the volume is actually mounted where the declaration asked,
  # rather than merely attached as a disk.
  awk -v m="$mount" '$2 == m' "$fstab" | grep -qF "/dev/disk/by-label/$label"
  # spec/06's first-boot ownership: the mount point has a row in the table the
  # prepare unit reads, and the unit waits for that mount before running.
  grep -qE "^$mount [^ ]+ [0-7]{4} 0$" "$prepare_table"
  grep -qF "$mount" <<<"$(sed -n 's/^RequiresMountsFor=//p' "$prepare_unit")"
  declared_seen=$((declared_seen + 1))
done <<<"$VIVARIUM_DECLARED_VOLUMES"
test "$declared_seen" = "$(grep -c . <<<"${VIVARIUM_DECLARED_VOLUMES:-}" || true)"
test "$((declared_seen + 2))" = "$(jq -r '.volumeLaunch | length' "$launcher_json")"
# The store volume is exempt from first-boot ownership (spec/06) and the home
# volume is not, whatever any image declares. Both halves, because a table built
# by filtering the attached volumes instead of the declared ones would still
# satisfy the first.
grep -qE "^$(jq -r .guestSession.home "$launcher_json") [^ ]+ 0700 1$" "$prepare_table"
# `/nix/.rw-store` is the writable store overlay, spelled the same way the fstab
# assertion below spells it. A row here would mean the table was rebuilt from the
# attached volumes rather than from the declared ones.
test "$(awk '$1 == "/nix/.rw-store"' "$prepare_table" | wc -l)" = 0
# ADR-0091: exactly one volume is provisioned for inodes, and it is the one
# whose mount point is the writable store overlay.
test "$(jq -r '[.volumeLaunch[] | select(.inodeRatio != null) | .role] | join(",")' "$launcher_json")" = store
test "$(jq -r '.volumeLaunch[] | select(.role == "store") | .inodeRatio' "$launcher_json")" = 8192
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
test "$(jq -r '.volumeLaunch[] | select(.role == "store") | .sizeMiB' "$launcher_json")" = "$VIVARIUM_STORE_VOLUME_SIZE_MIB"
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
