# shellcheck shell=bash
set -u
# `enableStrictShellChecks` routes this body through `writeShellApplication`,
# whose preamble adds `pipefail` on top of the `set -e` NixOS already wraps a
# unit `script` in. That is a behaviour change, not a lint: a probe here reads
# a query through a pipe and treats an empty result as a finding, which under
# `pipefail` aborts the unit instead. Errexit stays on deliberately; only the
# pipeline rule is returned to what this body was written against.
set +o pipefail
# NixOS wraps a unit `script` in `set -e`. Every probe below is an
# experiment whose negative result is the finding, so a non-zero exit must
# not strand the VM — but stopping is no longer this unit's job. The
# composed `vivarium-measurement-stop` unit is ordered after every selected
# leg with no `Requires=`, so it runs whether or not this one succeeds, and
# it is the single owner of poweroff (ADR-0095). Two owners would race.
# Write straight to the console device rather than through journald. This
# is the transport under test, so every byte below is a direct blocking
# write to the same tty Cloud Hypervisor exports as `--serial socket=`.
# CH 52.0 attaches the accepted socket to the UART without
# `set_nonblocking`, so a slow host reader stalls the writer instead of
# losing bytes — which makes this path lossless once journald is out of it.
exec >/dev/console 2>&1
# PID 1 writes its own status lines to this same console, and a status
# message landing mid-write splits a marker across two lines — all bytes
# present, but the token broken. That is interleaving, not loss, and it is
# the difference between a transport defect and cosmetic noise. Silence the
# competing writer for the duration rather than teaching the host to guess:
# systemd(1) documents SIGRTMIN+21 as "disable status messages on console"
# and SIGRTMIN+20 as its inverse.
kill -s RTMIN+21 1 2>/dev/null || true
restore_status() { kill -s RTMIN+20 1 2>/dev/null || true; }
trap restore_status EXIT
echo 'VIVARIUM_DIAGNOSTIC_FIRST_MARKER'
# A deterministic early burst measures lossless delivery, now that the
# replay question is answered (Cloud Hypervisor does not replay to a late
# client). The host counts marker occurrences and never bytes. State the
# expected count *before* the burst so a lost tail cannot also lose the
# number it should be judged against.
burst=/run/vivarium-console-burst
yes VIVARIUM_CONSOLE_BURST | head -c 1048576 >"$burst" || true
echo "VIVARIUM_CONSOLE_LINES_EXPECTED=$(grep -c VIVARIUM_CONSOLE_BURST "$burst")"
cat "$burst"
echo
echo 'VIVARIUM_CONSOLE_BYTES_CLAIMED=1048576'
rm -f "$burst"
sleep 10

# Two facts since ADR-0100, and they are separate on purpose. The share mounts at
# a build-time constant, which is what this first probe reads; where a session
# actually finds the project is a bind made at boot from the kernel command line,
# which the second probe reads. A run where the share is rw and the mirror never
# happened is exactly the state that would otherwise look like a working guest
# with an empty project directory.
mount_line="$(awk -v want="$VIVARIUM_WORKSPACE_INTERNAL" '$2 == want { print; exit }' /proc/self/mounts)"
echo "VIVARIUM_WORKSPACE_MOUNT=$mount_line"
if printf '%s\n' "$mount_line" | grep -Eq ' virtiofs (.*,)?rw(,| )'; then
  echo 'VIVARIUM_WORKSPACE_CONTRACT=virtiofs-rw'
else
  echo 'VIVARIUM_WORKSPACE_CONTRACT=unexpected'
fi

# The mirror is `vivarium-workspace.service`'s to make, and it says so on the
# console itself. Re-read here from the mount table so the diagnostic reports what
# the kernel holds rather than what a unit claimed, and so the two can disagree.
mirror_source=$(awk -v want="$VIVARIUM_WORKSPACE_INTERNAL" '$2 == want { print $1; exit }' /proc/self/mounts)
mirror_line=$(awk -v want="$VIVARIUM_WORKSPACE_INTERNAL" -v src="$mirror_source" \
  '$1 == src && $2 != want { print $2; exit }' /proc/self/mounts)
# Reported in the mount table's own escaping rather than decoded here. `/proc/self/mounts`
# writes a space as `\040` and a newline as `\012` for exactly the reason this marker needs:
# the value stays on one line and carries no delimiter a reader has to guess. Decoding it
# in the guest would undo that — a decoded newline would split this marker in two, and the
# command substitution doing the decoding would strip a trailing one first. The host lane
# decodes instead, where the result is compared rather than printed.
echo "VIVARIUM_WORKSPACE_MIRROR_PATH=${mirror_line:-absent}"

echo "VIVARIUM_STORE_PING_BEGIN"
# Same feature gate as the inner-develop spike below: without it these two
# report the guest's default feature set rather than anything about the
# shared store. `nix-store --check-validity` is stable CLI and needs none
# of this, which is why it stayed a usable answer even when these did not.
nix_unstable="nix --extra-experimental-features nix-command"
$nix_unstable store ping 2>&1 || true

# A single "is an arbitrary lowerdir path valid?" question cannot be
# interpreted, because "no" is the *expected* answer and says nothing
# about whether registration works at all. microvm.nix's registerClosure
# loads only `system.build.toplevel`'s closure via `nix-store --load-db`
# at boot (nixos-modules/microvm/store-disk.nix), so the honest probe is
# two questions: a path inside that closure must be valid, and a path
# outside it must not be. The pair is what measures ADR-0038's reach.
check_validity() {
  if nix-store --check-validity "$1" >/dev/null 2>&1; then echo yes; else echo no; fi
}
system_path="$(readlink -f /run/current-system)"
echo "VIVARIUM_STORE_SYSTEM_PATH=$system_path"
echo "VIVARIUM_STORE_DB_VALID_INCLOSURE=$(check_validity "$system_path")"

# The discriminator must ask about a path *in the store* — a
# /nix/.ro-store path is outside it and Nix rejects it outright, which
# would report invalid on every run regardless of the real answer.
lower_path="$(find /nix/.ro-store -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null || true)"
echo "VIVARIUM_STORE_LOWER_PATH=$lower_path"
if test -n "$lower_path"; then
  known_path="/nix/store/$(basename "$lower_path")"
  echo "VIVARIUM_STORE_KNOWN_PATH=$known_path"
  $nix_unstable path-info "$known_path" 2>&1 || true
  # Whether the sampled path happens to sit inside the registered closure
  # decides how its validity reads, so state it rather than inferring it.
  if nix-store -q --requisites "$system_path" 2>/dev/null | grep -qxF "$known_path"; then
    echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=yes'
  else
    echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=no'
  fi
  echo "VIVARIUM_STORE_DB_VALID_SAMPLE=$(check_validity "$known_path")"
else
  echo 'VIVARIUM_STORE_KNOWN_PATH='
  echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=indeterminate-empty-lowerdir'
  echo 'VIVARIUM_STORE_DB_VALID_SAMPLE=indeterminate-empty-lowerdir'
fi
echo "VIVARIUM_STORE_PING_END"

# Free page reporting can only return pages the guest actually frees, so
# the probe has to dirty *guest RAM*. The previous form wrote to the ext4
# volume, whose pages are host-file page cache rather than guest anonymous
# memory — freeing them need not produce anything reportable. /dev/shm is
# tmpfs, i.e. guest RAM, so deleting the file frees guest pages outright.
#
# The host cannot sample a transition it cannot see, so each phase is
# announced and then held long enough to be sampled. The host sampler
# aligns its series on these markers; without the hold there is no
# post-free window at all, which is why the earlier reading could only
# ever corroborate.
mem_kib() { awk -v k="$1:" '$1 == k { print $2 }' /proc/meminfo; }

# ADR-0094's posture, observed rather than asserted. A Nix-level assertion
# would only repeat its own input; what is in question is whether the
# generated swap unit actually *activated* in this guest — the module
# routes through a generator, and its own docs record a reset/udev race
# that leaves a device initialized but unusable. Read the running state.
echo "VIVARIUM_MEM_ZRAM_DEVICES=$(zramctl --noheadings --output NAME,DISKSIZE,ALGORITHM 2>/dev/null | tr '\n' ';')"
echo "VIVARIUM_MEM_SWAP=$(swapon --show=NAME,TYPE,SIZE,PRIO --noheadings --bytes 2>/dev/null | tr '\n' ';')"
# The three knobs ADR-0094 deliberately leaves alone. Printed so a later
# reader can see they are the kernel's own values and not vivarium's.
echo "VIVARIUM_MEM_SYSCTL=swappiness=$(cat /proc/sys/vm/swappiness 2>/dev/null) page_cluster=$(cat /proc/sys/vm/page-cluster 2>/dev/null) compaction_proactiveness=$(cat /proc/sys/vm/compaction_proactiveness 2>/dev/null) page_reporting_order=$(cat /sys/module/page_reporting/parameters/page_reporting_order 2>/dev/null)"

echo "VIVARIUM_MEM_PHASE=baseline MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
sleep 10
if dd if=/dev/zero of=/dev/shm/vivarium-memory-probe bs=1M count=512 status=none; then
  echo 'VIVARIUM_MEMORY_PROBE=ok'
else
  echo 'VIVARIUM_MEMORY_PROBE=failed'
fi
echo "VIVARIUM_MEM_PHASE=allocated MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
sleep 10
rm -f /dev/shm/vivarium-memory-probe || true
echo "VIVARIUM_MEM_PHASE=freed MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
# Free page reporting is asynchronous: the balloon walks free pages and
# hints them to the VMM over the reporting virtqueue. Give it a window
# before declaring the reading settled.
sleep 20
echo "VIVARIUM_MEM_PHASE=settled MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"

# A non-zero exit here IS the spike's answer, not an error — but only once
# the command can actually reach the store question. `nix develop` is
# gated on nix-command/flakes, so without these the spike exits 1 having
# measured nothing but the guest's default feature set. Passed per
# invocation rather than set in `nix.settings`, because whether the base
# image enables these globally is a base-image decision this spike must
# not pre-empt.
# Run as the mapped project user, not root. The workspace share maps host
# uid 1000 to guest uid 1000, so to guest root every file in it is owned
# by someone else and libgit2's ownership check refuses to open the
# repository at all — the spike then measures that refusal instead of the
# store. Running as `vivarium` makes the check pass on its merits and
# keeps the ADR-0066 translation contract intact; `safe.directory` would
# only suppress the symptom. HOME must be set explicitly because
# `runuser -u` does not start a login shell.
inner_status=0
runuser -u vivarium -- env HOME=/home/vivarium \
  timeout 120 nix --extra-experimental-features 'nix-command flakes' \
  --offline develop "$VIVARIUM_WORKSPACE_INTERNAL" --command true \
  >/home/vivarium/inner-nix-develop.log 2>&1 || inner_status=$?
echo "VIVARIUM_INNER_NIX_DEVELOP_STATUS=$inner_status"
tail -n 40 /home/vivarium/inner-nix-develop.log || true
echo 'VIVARIUM_DIAGNOSTIC_COMPLETE'
