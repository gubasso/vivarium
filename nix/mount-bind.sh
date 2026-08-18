# shellcheck shell=bash
#
# Bind each declared mount's share at the target the declaration named (spec/06,
# ADR-0020). The share itself mounts at a build-time internal point under
# `/run/vivarium-mounts`, because whether a declared `source` is a directory or a
# regular file is a host fact that resolves only at launch, so the guest's own
# fstab cannot mount it at the target directly. A file mount's share holds
# exactly that one file — the host stages it as the only entry of the share's
# export root (ADR-0105), so nothing here has to hide a sibling; there are
# none. This unit closes the gap the way `workspace-mirror.sh` does
# for the project tree: the static half — tag, read-only flag, target — is baked
# into `VIVARIUM_MOUNT_TABLE` from the merged configuration, and the launch half
# — dir or file, and which entry of the share is the file — arrives on the
# kernel command line as `vivarium.mount.<tag>=dir` or
# `vivarium.mount.<tag>=file:<encoded basename>`.
#
# The basename is percent-encoded by the host for the same reason the workspace
# path is: the command line is whitespace-separated while a file name may hold a
# space, a quote or a newline. The decode is `%` -> `\x` plus `printf %b`, safe
# only because the allowlist is checked BEFORE the decode — see the header of
# `workspace-mirror.sh`, which owns the full argument.
#
# The command line is written by the trusted host side; the guest is the
# untrusted party, not the reverse. The checks here are against a stale or
# hand-built specification, not against an attacker.
set -euo pipefail

internal_root=$VIVARIUM_MOUNT_INTERNAL_ROOT
guest_home=$VIVARIUM_GUEST_HOME
guest_owner=$VIVARIUM_GUEST_OWNER

say() { echo "VIVARIUM_MOUNT_BIND=$*" >/dev/console; }
refuse() {
  say "refused tag=$1 reason=$2"
  exit 1
}

# Create every missing ancestor of $1 (a directory path). Components under the
# guest home are chowned to the session user: the home volume persists, and a
# root-owned `~/.cargo` created on the way to `~/.cargo/registry` would follow
# the user across every later boot.
make_path() {
  local tag=$1 path=$2 cur stack=() dir
  cur=$path
  while [ ! -e "$cur" ] && [ ! -L "$cur" ]; do
    stack=("$cur" "${stack[@]}")
    cur=$(dirname -- "$cur")
  done
  [ ! -L "$cur" ] || refuse "$tag" ancestor-is-a-symlink
  [ -d "$cur" ] || refuse "$tag" ancestor-is-not-a-directory
  for dir in "${stack[@]}"; do
    install -d -m 0755 -- "$dir"
    case $dir in
      "$guest_home" | "$guest_home"/*) chown "$guest_owner" "$dir" ;;
    esac
  done
}

# Refuse $2 when any existing component of it is a symbolic link. The kernel
# follows nonfinal symlinks at bind time, and part of a target path — the
# persistent home — is guest-writable across boots, so a symlinked ancestor
# planted by an earlier session would redirect this root-run bind to a path the
# build-time target checks never saw. Walking with `-L` (lstat) needs no
# resolution of its own, and it runs before any session process exists, so what
# it clears is what the mount sees. Components that do not exist yet cannot
# redirect anything; `make_path` creates them and creates no links.
refuse_symlinked_components() {
  local tag=$1 path=$2 rest part cur=
  rest=${path#/}
  while [ -n "$rest" ]; do
    part=${rest%%/*}
    case $rest in
      */*) rest=${rest#*/} ;;
      *) rest= ;;
    esac
    [ -n "$part" ] || continue
    cur=$cur/$part
    if [ -L "$cur" ]; then
      [ "$cur" != "$path" ] || refuse "$tag" target-is-a-symlink
      refuse "$tag" ancestor-is-a-symlink
    fi
    [ -e "$cur" ] || break
  done
}

# Declared before the read for `set -u`, and `|| true` because `read` reports
# failure at end of input — the same shape, for the same reason, as the
# workspace mirror's own cmdline read.
tokens=()
read -r -a tokens </proc/cmdline || true

bound=0
while read -r tag readonly_flag target; do
  [ -n "$tag" ] || continue

  internal=$internal_root/$tag
  # `RequiresMountsFor` already failed the unit if the share did not mount; this
  # is the second half of the same guard: a bind from an EMPTY internal point
  # would succeed and serve nothing, silently.
  findmnt --mountpoint "$internal" >/dev/null || refuse "$tag" internal-share-not-mounted

  value=
  for token in "${tokens[@]}"; do
    case $token in
      "vivarium.mount.$tag="*)
        [ -z "$value" ] || refuse "$tag" duplicate-parameter
        value=${token#"vivarium.mount.$tag="}
        ;;
    esac
  done

  if [ -z "$value" ]; then
    # The image booted without `viv` — a measurement leg or a hand boot. The
    # share is readable at its internal point; having no plan for it is not a
    # failure, exactly as an absent workspace parameter is not.
    say "absent tag=$tag internal=$internal"
    continue
  fi

  kind=
  entry=
  case $value in
    dir) kind="dir" ;;
    file:*)
      kind="file"
      encoded=${value#file:}
      # Allowlist before decode (see header). No `/` in the class: the entry is
      # one name inside the share, never a path.
      printf '%s' "$encoded" | grep -qxE '([A-Za-z0-9._~-]|%[0-9A-F]{2})+' \
        || refuse "$tag" malformed-entry-encoding
      # The `x` sentinel carries a trailing newline through command
      # substitution; `workspace-mirror.sh` owns the explanation.
      entry=$(printf '%b' "${encoded//%/\\x}x")
      entry=${entry%x}
      case $entry in
        . | ..) refuse "$tag" entry-escapes-the-share ;;
        */*) refuse "$tag" entry-is-a-path ;;
      esac
      ;;
    *) refuse "$tag" malformed-kind ;;
  esac

  if [ "$kind" = dir ]; then
    source=$internal
  else
    source=$internal/$entry
    [ -e "$source" ] || refuse "$tag" entry-missing
    [ -f "$source" ] || refuse "$tag" entry-not-a-regular-file
  fi

  # Every component, not only the final one: the exists-branch below would
  # otherwise accept `~/link/existing` with `~/link` pointing anywhere.
  refuse_symlinked_components "$tag" "$target"

  if [ -e "$target" ] || [ -L "$target" ]; then
    # The target may legitimately exist: a home-volume path like
    # `~/.cargo/registry` persists across boots. It must only be the right
    # shape to bind over; the symlink walk above already refused links.
    if [ "$kind" = dir ]; then
      [ -d "$target" ] || refuse "$tag" target-not-a-directory
    else
      [ -f "$target" ] || refuse "$tag" target-not-a-regular-file
    fi
  else
    if [ "$kind" = dir ]; then
      make_path "$tag" "$target"
    else
      make_path "$tag" "$(dirname -- "$target")"
      install -m 0644 /dev/null "$target"
      case $target in
        "$guest_home"/*) chown "$guest_owner" "$target" ;;
      esac
    fi
  fi

  # `--rbind` for the same spec/06 reason as the workspace mirror: the host's
  # own filesystem boundaries cross with the mount.
  mount --rbind -- "$source" "$target" || refuse "$tag" bind-failed
  findmnt --mountpoint "$target" >/dev/null || refuse "$tag" bind-not-mounted

  if [ "$readonly_flag" = 1 ]; then
    # Defence in depth: the virtiofs superblock is already read-only (the share
    # mounts `ro,nodev,nosuid,noexec` from the guest fstab), so this remount
    # pins the same flags on the bind itself, where a later mount-table reader
    # expects to see them (spec/06:26).
    mount -o remount,bind,ro,nodev,nosuid,noexec -- "$target" \
      || refuse "$tag" readonly-remount-failed
  fi

  bound=$((bound + 1))
  say "ok tag=$tag kind=$kind target=$target"
done <"$VIVARIUM_MOUNT_TABLE"

say "done bound=$bound"
