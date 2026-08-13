# shellcheck shell=bash
set -u
# NixOS prepends `set -e` to every `script`, and for this unit that is
# actively wrong: the whole point here is to run commands that are
# *expected* to fail and to report the status they failed with. Under
# `set -e` the first such probe takes the shell down before it can print
# its own result, and — because systemd stops writing unit status to the
# console once boot has completed — the run then looks like a probe that
# simply returned nothing. Measured that way on the first host run of
# this check: the unit died at the first lookup of a deleted path, and
# only ExecStopPost revealed it had died at all.
set +e
# Same transport argument as the spike and the diagnostic: journald's
# console forwarder drops bytes under congestion.
exec >/dev/console 2>&1
# Every marker carries a `=value`, including these two: the host reads
# markers by name with a trailing `=`, so a bare word would be invisible
# to it — and an invisible completion marker reads as an incomplete run.
echo 'VIVARIUM_GC_INTERLOCK_BEGIN=yes'
echo 'VIVARIUM_GC_INTERLOCK_PROTOCOL=1'

# From the unit rather than spelled here: the share's guest mount point is a
# product constant now (ADR-0100), and a second copy of it would be a second
# thing to move.
ws=$VIVARIUM_GC_INTERLOCK_DIR

# Guest uid 0 sits inside virtiofsd's `forbid-guest:0:1000` range, so
# *every* workspace syscall — including this existence test — has to run
# as the one mapped identity. As root it would return EPERM and this unit
# would report "no instruction" on a run that had one, which is precisely
# the confident-but-vacuous result the harness exists to avoid.
as_ws() { runuser -u vivarium -- "$@"; }

if ! as_ws test -r "$ws/instruction"; then
  echo 'VIVARIUM_GC_INTERLOCK_SKIPPED=no-instruction'
  echo 'VIVARIUM_GC_INTERLOCK_COMPLETE=yes'
  exit 0
fi

TARGET=
CONTROL=
RUN_ID=
GO_TIMEOUT=300
while IFS='=' read -r key value; do
  case $key in
    TARGET | CONTROL | RUN_ID | GO_TIMEOUT) printf -v "$key" '%s' "$value" ;;
  esac
done < <(as_ws cat "$ws/instruction")

# A malformed instruction must stop the run, not silently probe some
# other path and report confident nonsense about it.
for pair in "TARGET:$TARGET" "CONTROL:$CONTROL"; do
  case ${pair#*:} in
    /nix/store/?*) ;;
    *)
      echo "VIVARIUM_GC_INTERLOCK_SKIPPED=bad-${pair%%:*}"
      echo 'VIVARIUM_GC_INTERLOCK_COMPLETE=yes'
      exit 0
      ;;
  esac
done
echo "VIVARIUM_GC_INTERLOCK_RUN_ID=$RUN_ID"
echo "VIVARIUM_GC_INTERLOCK_TARGET=$TARGET"
echo "VIVARIUM_GC_INTERLOCK_CONTROL=$CONTROL"

errfile=/run/vivarium-gc-interlock.err

# One line per probe, and the stderr text is carried *verbatim* because
# it is the only thing that distinguishes the errno: coreutils prints
# strerror(3), so "Stale file handle", "Input/output error" and "No such
# file or directory" are three separate answers to ADR-0085's question.
# Exit 124 is `timeout` firing, and a hang is itself a loud symptom, so
# it is reported as one rather than allowed to wedge the unit.
probe() {
  local marker=$1
  shift
  local out status err
  out=$(timeout 30 "$@" 2>"$errfile")
  status=$?
  err=$(tr '\n' '|' <"$errfile" | tail -c 200)
  out=$(printf '%s' "$out" | tr '\n' '|' | tail -c 200)
  if [ "$status" = 124 ]; then echo "VIVARIUM_GC_INTERLOCK_HANG=$marker"; fi
  echo "VIVARIUM_GC_INTERLOCK_${marker}=status=$status out=$out err=$err"
}

# A read from a descriptor opened *before* the deletion, which is a
# different question from a fresh lookup by name: virtiofsd runs with
# `--inode-file-handles=never`, so it holds its own descriptor per known
# inode and the host's unlink frees no blocks while that lasts.
# Bytes go to a file rather than through a command substitution: `$(...)`
# strips trailing newlines, which would make every digest here disagree
# with the host's for a reason that has nothing to do with the store.
# A file also keeps the *reader's* exit status, where a pipe would report
# sha256sum's.
fdbytes=/run/vivarium-gc-interlock.bin
probe_fd() {
  local marker=$1 fd=$2
  local status err
  case $fd in
    8)
      timeout 30 cat <&8 >"$fdbytes" 2>"$errfile"
      status=$?
      ;;
    9)
      timeout 30 cat <&9 >"$fdbytes" 2>"$errfile"
      status=$?
      ;;
  esac
  err=$(tr '\n' '|' <"$errfile" | tail -c 200)
  if [ "$status" = 124 ]; then echo "VIVARIUM_GC_INTERLOCK_HANG=$marker"; fi
  echo "VIVARIUM_GC_INTERLOCK_${marker}=status=$status out=$(sha256sum <"$fdbytes" | cut -d' ' -f1) bytes=$(stat -c %s "$fdbytes") err=$err"
}

# --- before-phase: the anti-vacuous core -----------------------------
# Removing a path the guest has never read proves nothing — nothing was
# cached, so nothing can go stale, and the run would report a confident
# result for a hazard it never triggered. The `ready` file below is not
# written until all of this has succeeded, so the host cannot delete
# anything until the guest has genuinely read it.
probe BEFORE_STAT stat -c '%i %h %s' "$TARGET/payload"
probe BEFORE_LIST ls -1 "$TARGET"
probe BEFORE_READ sha256sum "$TARGET/payload"
probe BEFORE_SIBLING sha256sum "$TARGET/sibling"

# Two descriptors, opened now and read at different points later. fd 9 is
# partly consumed here so the after-phase reads its *remainder* — a fixed,
# host-comparable byte range that needs no seek primitive bash lacks. fd 8
# stays untouched until after drop_caches.
# Guarded: a redirection failure on `exec` terminates a non-interactive
# shell outright, which would lose every marker after this point.
if [ -r "$TARGET/payload" ]; then
  exec 9<"$TARGET/payload"
  exec 8<"$TARGET/payload"
  echo 'VIVARIUM_GC_INTERLOCK_OPEN_FDS=ok'
else
  echo 'VIVARIUM_GC_INTERLOCK_OPEN_FDS=failed'
fi
timeout 30 head -c 4096 <&9 >"$fdbytes" 2>"$errfile"
status=$?
echo "VIVARIUM_GC_INTERLOCK_BEFORE_FD9_HEAD=status=$status out=$(sha256sum <"$fdbytes" | cut -d' ' -f1) bytes=$(stat -c %s "$fdbytes")"

# The control is deliberately not touched, and saying so on the console
# makes that a recorded property of the run rather than an inference.
echo 'VIVARIUM_GC_INTERLOCK_CONTROL_UNREAD=yes'

# --- handshake -------------------------------------------------------
# No shell is spawned under `runuser`: this unit's PATH carries coreutils
# and util-linux only, and pulling in a shell to write two lines would
# widen the closure for nothing. Written to `.tmp` and renamed so the
# host can never observe a half-written handshake file.
printf 'RUN_ID=%s\nSTATE=ready\n' "$RUN_ID" | as_ws tee "$ws/ready.tmp" >/dev/null
ready_status=$?
sync
as_ws mv "$ws/ready.tmp" "$ws/ready" || ready_status=$?
echo "VIVARIUM_GC_INTERLOCK_READY_WRITTEN=$ready_status"

waited=0
go_state=seen
while ! as_ws test -e "$ws/go"; do
  sleep 0.5
  waited=$((waited + 1))
  if [ "$waited" -gt $((GO_TIMEOUT * 2)) ]; then
    go_state=timeout
    break
  fi
done
echo "VIVARIUM_GC_INTERLOCK_GO_WAIT=$go_state after $((waited / 2))s"

# --- round 1: caches warm, which is the state a running build is in ---
probe AFTER_STAT stat -c '%i %h %s' "$TARGET/payload"
probe AFTER_LIST ls -1 "$TARGET"
probe AFTER_REOPEN sha256sum "$TARGET/payload"
probe AFTER_SIBLING sha256sum "$TARGET/sibling"
probe_fd AFTER_FD9_REST 9
# Stepped, because the first-ever lookup of a path deleted host-side is
# the one operation here with no prior art to predict it: if it takes the
# shell down, the last STEP marker is what says which call did it.
echo 'VIVARIUM_GC_INTERLOCK_STEP=control-stat'
probe AFTER_CONTROL_STAT stat -c '%i %h %s' "$CONTROL/payload"
echo 'VIVARIUM_GC_INTERLOCK_STEP=control-read'
probe AFTER_CONTROL sha256sum "$CONTROL/payload"
echo 'VIVARIUM_GC_INTERLOCK_STEP=round1-done'

# --- round 2: force the guest to forget. The decisive round. ----------
# While the guest remembers an inode, virtiofsd keeps its descriptor open
# and the host's unlink frees nothing, so round 1 can succeed for reasons
# that say nothing about a collected path. drop_caches makes the guest
# send FORGET, virtiofsd closes the descriptor, and only then can a lookup
# by name actually reach a host path that is gone.
sync
if echo 3 >/proc/sys/vm/drop_caches 2>"$errfile"; then
  echo 'VIVARIUM_GC_INTERLOCK_DROP_CACHES=ok'
else
  echo "VIVARIUM_GC_INTERLOCK_DROP_CACHES=failed $(tr '\n' '|' <"$errfile")"
fi
probe AFTER_DROP_SIBLING sha256sum "$TARGET/sibling"
probe AFTER_DROP_REOPEN sha256sum "$TARGET/payload"
probe AFTER_DROP_LIST ls -1 "$TARGET"
probe AFTER_DROP_CONTROL sha256sum "$CONTROL/payload"
probe_fd AFTER_DROP_FD8 8

# --- round 3: settle --------------------------------------------------
# The store share is cache="always", so its entry timeout is 86400s and a
# timeout-driven revalidation is structurally out of reach inside one
# boot. This round exists to show that, not to hope otherwise.
sleep 60
probe SETTLED_REOPEN sha256sum "$TARGET/payload"
probe SETTLED_SIBLING sha256sum "$TARGET/sibling"
exec 9<&- 8<&-
echo 'VIVARIUM_GC_INTERLOCK_COMPLETE=yes'
