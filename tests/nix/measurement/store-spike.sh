# shellcheck shell=bash
set -u
# `enableStrictShellChecks` builds this body with `writeShellApplication`, which
# prepends `pipefail` to the `set -e` a NixOS unit `script` already carries.
# This body predates that and relies on the older rule: `nix-store -q … | sed`
# below returns empty when the query finds nothing, and the very next line tests
# for empty. Under `pipefail` the assignment fails instead and errexit kills the
# probe mid-run — a negative result turned into a dead unit, on the one leg
# whose whole design is that a negative result is the finding.
set +o pipefail
# Same transport argument as the diagnostic: journald's console forwarder
# drops bytes under congestion, so write to the tty directly.
exec >/dev/console 2>&1
echo 'VIVARIUM_STORE_SPIKE_BEGIN'

# Every client below must reach the overlay store through the daemon.
# Root can write the merged /nix/store, so with NIX_REMOTE unset `nix`
# opens it directly as a plain LocalStore backed by the *lower* database
# — measuring a store nothing in this design uses, and reporting
# confidently about it. This is ADR-0088's "daemon-only" detail as an
# operational fact rather than a configuration one.
export NIX_REMOTE=daemon
export HOME=/root

# Every Nix tool resolves through the unit's `path`, which must therefore keep
# `pkgs.nix` — the extraction moved these off the absolute store paths they used
# to carry. The reason to check that first when debugging: the unit's PATH once
# covered nix-store and nix-build but not nix-collect-garbage, which cost a whole
# boot to a bare exit 127 — a `[FAIL]` for a collection that never ran, which
# reads exactly like a collection that failed.
nix_store=nix-store
nix_build=nix-build
nix_collect_garbage=nix-collect-garbage

upper_root=$VIVARIUM_UPPER_ROOT
upper_layer=$VIVARIUM_UPPER_LAYER
lower_dir=$VIVARIUM_LOWER_STORE_DIR
lower_view=$VIVARIUM_LOWER_STORE_VIEW_DIR
lower_store=$VIVARIUM_LOWER_STORE_URI
# The same lower store opened writable. Legitimate: its state directory
# is the tmpfs root's own /nix/var/nix, rebuilt from `regInfo` on every
# boot, and adding a row for a path whose bytes really are in the lower
# layer is the same operation `registerClosure` performs at boot.
lower_store_rw="local?real=$VIVARIUM_LOWER_STORE_VIEW_DIR&state=$VIVARIUM_LOWER_STATE_DIR"
canary_expr=$VIVARIUM_STORE_CANARY_EXPRESSION
persist_record=$upper_root/.vivarium-spike-persistence-canary
roots_dir=$VIVARIUM_UPPER_STATE_DIR/gcroots

# The boot counter is on the volume, so it *is* a persistence
# observation as well as a branch: on a cold volume it cannot exist.
boots_file=$upper_root/.vivarium-spike-boots
boot=1
if test -r "$boots_file"; then boot=$(($(cat "$boots_file") + 1)); fi
echo "$boot" >"$boots_file" || true
echo "VIVARIUM_STORE_SPIKE_BOOT=$boot"
# overlayfs runs in the guest, so the guest's kernel is the one that
# governs the stale-handle behaviour — not the host's.
echo "VIVARIUM_STORE_SPIKE_GUEST_KERNEL=$(uname -r)"
echo "VIVARIUM_STORE_SPIKE_NIX_VERSION=$($nix_store --version 2>&1 | head -n1)"

# --- premise (b): a volume backs the writable layer, from the initrd ---
mount_field() { awk -v mp="$1" -v f="$2" '$2 == mp { print $f; exit }' /proc/self/mounts; }
mount_id() { awk -v mp="$1" '$5 == mp { print $1; exit }' /proc/self/mountinfo; }
echo "VIVARIUM_STORE_SPIKE_UPPER_MOUNT=$(mount_field "$upper_root" 1) $(mount_field "$upper_root" 3) $(mount_field "$upper_root" 4)"
echo "VIVARIUM_STORE_SPIKE_STORE_MOUNT=$(mount_field /nix/store 1) $(mount_field /nix/store 3)"
echo "VIVARIUM_STORE_SPIKE_LOWER_VIEW_MOUNT=$(mount_field "$lower_view" 1) $(mount_field "$lower_view" 3)"
case "$(mount_field "$upper_root" 1)" in
  /dev/*) echo 'VIVARIUM_STORE_SPIKE_UPPER_DEVICE=block' ;;
  *) echo 'VIVARIUM_STORE_SPIKE_UPPER_DEVICE=unexpected' ;;
esac
# Mount IDs are allocated in mount order, so the writable layer having a
# lower id than the overlay built from it is direct evidence it was
# established first — which is what `neededForBoot` is supposed to buy.
upper_id=$(mount_id "$upper_root")
store_id=$(mount_id /nix/store)
echo "VIVARIUM_STORE_SPIKE_MOUNT_IDS=upper=$upper_id view=$(mount_id "$lower_view") store=$store_id"
if test -n "$upper_id" && test -n "$store_id" && test "$upper_id" -lt "$store_id"; then
  echo 'VIVARIUM_STORE_SPIKE_UPPER_BEFORE_STORE=yes'
else
  echo 'VIVARIUM_STORE_SPIKE_UPPER_BEFORE_STORE=no'
fi
# ADR-0067's stage-2 first-boot machinery must be *absent* here, not
# merely assumed away: this volume mounts before stage 2 exists.
echo "VIVARIUM_STORE_SPIKE_UPPER_OWNER=$(stat -c '%U:%G %a' "$upper_root")"
if test -e "$upper_root/.vivarium-first-boot"; then
  echo 'VIVARIUM_STORE_SPIKE_STAGE2_OWNERSHIP=present'
else
  echo 'VIVARIUM_STORE_SPIKE_STAGE2_OWNERSHIP=absent'
fi

# --- premise (c): the registered closure and the daemon compose ---
system_path=$(readlink -f /run/current-system)
echo "VIVARIUM_STORE_SPIKE_SYSTEM_PATH=$system_path"
if $nix_store --store "$lower_store" --check-validity "$system_path" >/dev/null 2>&1; then
  echo 'VIVARIUM_STORE_SPIKE_LOWER_DB_HAS_SYSTEM=yes'
else
  echo 'VIVARIUM_STORE_SPIKE_LOWER_DB_HAS_SYSTEM=no'
fi
# The daemon's own environment is the least deniable evidence that the
# overlay store is the one it opened. Two things had to be learned the
# expensive way: `systemctl show -p MainPID` reads 0 for a
# socket-activated unit, and nothing above this line had yet spoken to
# the daemon, so on the first two runs there was no process to find.
# Contact it first, then look for it.
$nix_store --check-validity "$system_path" >/dev/null 2>&1 || true
daemon_remote=unavailable
for environ in /proc/[0-9]*/environ; do
  proc=${environ%/environ}
  case "$(tr '\0' ' ' <"$proc/cmdline" 2>/dev/null || true)" in
    *nix-daemon*--daemon*)
      found=$(tr '\0' '\n' <"$environ" 2>/dev/null | grep '^NIX_REMOTE=' | head -n1 || true)
      test -n "$found" && daemon_remote=$found && break
      ;;
  esac
done
echo "VIVARIUM_STORE_SPIKE_DAEMON_NIX_REMOTE=$daemon_remote"

# --- upstream's `check-post-init` analogue: lower and merged agree ---
sample=$($nix_store -q --requisites "$system_path" 2>/dev/null | sed -n '3p')
echo "VIVARIUM_STORE_SPIKE_SAMPLE_PATH=$sample"
if test -n "$sample"; then
  merged=no
  lower=no
  verified=no
  $nix_store --check-validity "$sample" >/dev/null 2>&1 && merged=yes
  $nix_store --store "$lower_store" --check-validity "$sample" >/dev/null 2>&1 && lower=yes
  $nix_store --verify-path "$sample" >/dev/null 2>&1 && verified=yes
  echo "VIVARIUM_STORE_SPIKE_CHECK_POST_INIT=merged=$merged lower=$lower verified=$verified"
else
  echo 'VIVARIUM_STORE_SPIKE_CHECK_POST_INIT=indeterminate-no-sample'
fi

# --- upstream's `redundant-add` analogue: ADR-0087's sharing claim ---
# A path already valid in the lower database must never be copied up.
# This count is what decides whether the volume grows by a closure or by
# what the guest actually fetched.
copied=0
total=0
while IFS= read -r requisite; do
  total=$((total + 1))
  test -e "$upper_layer/$(basename "$requisite")" && copied=$((copied + 1))
done < <($nix_store -q --requisites "$system_path" 2>/dev/null)
echo "VIVARIUM_STORE_SPIKE_BOOT_CLOSURE_COPIED_UP=$copied/$total"

# `-i` and `--output` are mutually exclusive in coreutils, and the
# combination silently produced two empty measurements on the first run.
df_report() {
  echo "VIVARIUM_STORE_SPIKE_DF_H_$1=$(df -h "$upper_root" 2>&1 | tail -n1)"
  echo "VIVARIUM_STORE_SPIKE_DF_I_$1=$(df -i "$upper_root" 2>&1 | tail -n1)"
}
df_report EARLY

# --- the persistence canary: the database, not only the bytes ---
# A path added by the guest, with no twin below, so its validity can
# only come from the *upper* store's database. On a warm boot it must
# already be valid before anything is built or added. A single boot
# cannot show this, which is why two is the floor for ADR-0087.
if test -r "$persist_record"; then
  recorded=$(cat "$persist_record")
  echo "VIVARIUM_STORE_SPIKE_PERSIST_RECORDED=$recorded"
  valid=no
  upper=absent
  $nix_store --check-validity "$recorded" >/dev/null 2>&1 && valid=yes
  test -e "$upper_layer/$(basename "$recorded")" && upper=present
  echo "VIVARIUM_STORE_SPIKE_PERSIST_VALID_BEFORE_WRITE=$valid"
  echo "VIVARIUM_STORE_SPIKE_PERSIST_UPPER_BEFORE_WRITE=$upper"
else
  echo 'VIVARIUM_STORE_SPIKE_PERSIST_RECORDED=none-cold-volume'
fi
printf 'vivarium-persistence-canary\n' >/run/vivarium-persistence-canary
persist_status=0
persist=$($nix_store --add /run/vivarium-persistence-canary 2>/run/vivarium-persist.log) || persist_status=$?
echo "VIVARIUM_STORE_SPIKE_PERSIST_STATUS=$persist_status"
echo "VIVARIUM_STORE_SPIKE_PERSIST_PATH=$persist"
if test -n "$persist" && test -e "$persist"; then
  printf '%s' "$persist" >"$persist_record"
  mkdir -p "$roots_dir"
  ln -sfn "$persist" "$roots_dir/vivarium-persistence-canary"
else
  echo "VIVARIUM_STORE_SPIKE_PERSIST_ERROR=$(tail -n 3 /run/vivarium-persist.log | tr '\n' '|')"
fi

# --- the delete-duplicate scenario, in the two shapes it really has ---
# Upstream's own `stale-file-handle` test is unrunnable here: it
# garbage-collects the *lower* store three times, and ours is served
# --readonly. This is the reachable substitute, and the first host run
# showed the design checklist had it half wrong. `deleteStorePath`
# guards on `lowerStore->isValidPath(storePath)` — validity in the lower
# *database*, not presence of bytes below — so there are two branches
# and only one of them avoids a whiteout.
duplicate_status=0
duplicate=$(timeout 300 $nix_build --no-out-link "$canary_expr" 2>"/run/vivarium-canary.log") || duplicate_status=$?
echo "VIVARIUM_STORE_SPIKE_DUPLICATE_STATUS=$duplicate_status"
echo "VIVARIUM_STORE_SPIKE_DUPLICATE_PATH=$duplicate"
if ((duplicate_status != 0)); then
  echo "VIVARIUM_STORE_SPIKE_DUPLICATE_ERROR=$(tail -n 5 /run/vivarium-canary.log | tr '\n' '|')"
fi

delete_arm() {
  # $1 store-path base name, $2 marker suffix. Prints what the two
  # layers and the merged view look like after deleting the duplicate.
  local name=$1 suffix=$2 out upper merged whiteout
  out=$(timeout 300 $nix_store --delete "$duplicate" 2>&1) || true
  upper=present
  test -e "$upper_layer/$name" || upper=absent
  merged=absent
  test -e "/nix/store/$name" && merged=present
  whiteout=none
  test -c "$upper_layer/$name" && whiteout=char-device
  echo "VIVARIUM_STORE_SPIKE_DELETE_$suffix=upper=$upper merged=$merged whiteout=$whiteout"
  echo "VIVARIUM_STORE_SPIKE_DELETE_OUTPUT_$suffix=$(printf '%s' "$out" | tr '\n' '|' | tail -c 300)"
}

if test -n "$duplicate" && test -e "$duplicate"; then
  name=$(basename "$duplicate")
  lower_present=no
  upper_present=no
  test -e "$lower_dir/$name" && lower_present=yes
  test -e "$upper_layer/$name" && upper_present=yes
  lower_valid=no
  $nix_store --store "$lower_store" --check-validity "$duplicate" >/dev/null 2>&1 && lower_valid=yes
  echo "VIVARIUM_STORE_SPIKE_DUPLICATE_LAYERS=lower_bytes=$lower_present lower_valid=$lower_valid upper=$upper_present"

  if test "$lower_present" = yes && test "$upper_present" = yes; then
    # Arm 1 — bytes below, unknown to the lower database. This is the
    # ordinary write path ADR-0087 describes, and upstream's `else`
    # branch: deleted through the merged view, so a whiteout is the
    # *expected* outcome rather than a defect. It is not the
    # catastrophic case, because the path is not one the guest's store
    # considers valid and a rebuild replaces the whiteout outright.
    delete_arm "$name" UNREGISTERED
    rebuild_status=0
    timeout 300 $nix_build --no-out-link "$canary_expr" >/dev/null 2>"/run/vivarium-rebuild.log" || rebuild_status=$?
    echo "VIVARIUM_STORE_SPIKE_REBUILD_AFTER_UNREGISTERED=$rebuild_status"
    if ((rebuild_status != 0)) || grep -qi 'stale file handle' /run/vivarium-rebuild.log 2>/dev/null; then
      echo "VIVARIUM_STORE_SPIKE_REBUILD_ERROR=$(tail -n 5 /run/vivarium-rebuild.log | tr '\n' '|')"
    fi

    # Arm 2 — the branch the remount hook exists for. Register the path
    # in the lower database first, which asserts nothing false: its
    # bytes really are in the lower layer, and this is the state a later
    # boot closure produces on its own for anything the guest had
    # already copied up. Now `deleteStorePath` must delete through the
    # upper layer, remount, and leave the merged view resolving to the
    # lower inode with no whiteout at all.
    register_status=0
    $nix_store --dump-db "$duplicate" >/run/vivarium-canary-db 2>/dev/null || register_status=$?
    $nix_store --store "$lower_store_rw" --load-db </run/vivarium-canary-db >/dev/null 2>&1 || register_status=$?
    lower_valid_now=no
    $nix_store --store "$lower_store" --check-validity "$duplicate" >/dev/null 2>&1 && lower_valid_now=yes
    echo "VIVARIUM_STORE_SPIKE_LOWER_REGISTER=status=$register_status lower_valid=$lower_valid_now"
    if test "$lower_valid_now" = yes && test -e "$upper_layer/$name"; then
      merged_inode_before=$(stat -c %i "/nix/store/$name" 2>/dev/null || echo unknown)
      lower_inode=$(stat -c %i "$lower_dir/$name" 2>/dev/null || echo unknown)
      delete_arm "$name" REGISTERED
      merged_inode_after=$(stat -c %i "/nix/store/$name" 2>/dev/null || echo unknown)
      echo "VIVARIUM_STORE_SPIKE_DELETE_INODES=merged_before=$merged_inode_before merged_after=$merged_inode_after lower=$lower_inode"
    else
      echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-registration-did-not-take'
    fi
  else
    echo 'VIVARIUM_STORE_SPIKE_DELETE_UNREGISTERED=skipped-no-duplicate'
    echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-no-duplicate'
  fi
else
  echo 'VIVARIUM_STORE_SPIKE_DUPLICATE_LAYERS=indeterminate-no-duplicate'
  echo 'VIVARIUM_STORE_SPIKE_DELETE_UNREGISTERED=skipped-no-duplicate'
  echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-no-duplicate'
fi

# --- add-on: does a collection's remount disturb a running process? ---
# Expected safe: a remount reconfigures the superblock, and open fds and
# mappings survive it. Cheap enough to stop being an expectation.
sleep 120 >/dev/null 2>&1 &
sleeper=$!
sleeper_exe=$(readlink "/proc/$sleeper/exe" 2>/dev/null || true)

# --- ADR-0088's core claim: a rooted collection is non-destructive ---
# `deleteStorePath` returns immediately unless the path exists in the
# upper layer, so the thousands of host paths the guest never copied up
# cannot be whiteed out. The lower layer's entry count either side of a
# collection is the host-visible statement of that.
lower_before=$(find "$lower_dir" -mindepth 1 -maxdepth 1 | wc -l)
whiteouts_before=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c 2>/dev/null | wc -l)
gc_status=0
timeout 300 $nix_collect_garbage >/run/vivarium-gc.log 2>&1 || gc_status=$?
lower_after=$(find "$lower_dir" -mindepth 1 -maxdepth 1 | wc -l)
whiteouts=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c 2>/dev/null | wc -l)
system_still=no
$nix_store --check-validity "$system_path" >/dev/null 2>&1 && system_still=yes
system_present=no
test -e "$system_path" && system_present=yes
persist_survived=no
test -n "${persist:-}" && test -e "$persist" && persist_survived=yes
echo "VIVARIUM_STORE_SPIKE_GC=status=$gc_status lower_before=$lower_before lower_after=$lower_after whiteouts_before=$whiteouts_before whiteouts=$whiteouts system_valid=$system_still system_present=$system_present rooted_canary=$persist_survived"
# Name them, and — the question that actually decides whether ADR-0088
# holds — say how many sit over a path the *lower database* considers
# valid. Upstream's `else` branch writes a whiteout whenever an
# upper-layer path is absent from the lower database, so a whiteout over
# an unregistered path is specified behaviour. A whiteout over a
# registered one would be the guest blinding itself to its own boot
# closure, and that must be zero.
whiteout_names=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c -printf '%f\n' 2>/dev/null || true)
echo "VIVARIUM_STORE_SPIKE_WHITEOUT_NAMES=$(printf '%s' "$whiteout_names" | head -n 10 | tr '\n' ' ')"
whiteout_over_valid=0
while IFS= read -r whiteout; do
  test -n "$whiteout" || continue
  if $nix_store --store "$lower_store" --check-validity "/nix/store/$whiteout" >/dev/null 2>&1; then
    whiteout_over_valid=$((whiteout_over_valid + 1))
    echo "VIVARIUM_STORE_SPIKE_WHITEOUT_OVER_VALID_NAME=$whiteout"
  fi
done <<<"$whiteout_names"
echo "VIVARIUM_STORE_SPIKE_WHITEOUT_OVER_VALID=$whiteout_over_valid"
echo "VIVARIUM_STORE_SPIKE_GC_OUTPUT=$(tail -n 3 /run/vivarium-gc.log | tr '\n' '|' | tail -c 300)"

if kill -0 "$sleeper" 2>/dev/null && test "$(readlink "/proc/$sleeper/exe" 2>/dev/null || true)" = "$sleeper_exe"; then
  echo 'VIVARIUM_STORE_SPIKE_EXEC_ACROSS_REMOUNT=intact'
else
  echo 'VIVARIUM_STORE_SPIKE_EXEC_ACROSS_REMOUNT=disturbed'
fi
kill "$sleeper" 2>/dev/null || true

# --- add-on: a stray name in the share root aborts a guest collection ---
# `deleteStorePath` parses a store-path name *before* reaching the guard
# above, so one non-store entry in the lower layer is a hard stop. The
# same question applies to the upper layer, where LocalStore's own
# `.links` lands.
stray_lower=$(find "$lower_dir" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vcE '^[0-9a-z]{32}-' || true)
stray_upper=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vcE '^[0-9a-z]{32}-' || true)
echo "VIVARIUM_STORE_SPIKE_STRAY_NAMES=lower=$stray_lower upper=$stray_upper"
echo "VIVARIUM_STORE_SPIKE_STRAY_UPPER_NAMES=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vE '^[0-9a-z]{32}-' | head -n 10 | tr '\n' ' ')"

df_report LATE
echo "VIVARIUM_STORE_SPIKE_UPPER_USAGE=$(du -sk "$upper_layer" 2>/dev/null | awk '{print $1}')KiB"
echo 'VIVARIUM_STORE_SPIKE_COMPLETE'
