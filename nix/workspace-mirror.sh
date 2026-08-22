# shellcheck shell=bash
#
# Bind every declared workspace share where the host holds that tree (ADR-0108, N16).
#
# Slice 020 made the workspace plural. The static half — one `wsN` tag and one
# internal mount point per declared tree — comes from the built image through the
# table this reads; the launch-expanded absolute paths stay launch-channel (N19)
# and arrive on the kernel command line, one `vivarium.workspace.<tag>=` each.
#
# Those paths are whitespace-separated on a command line while a project directory
# may legitimately hold a space, a quote or a newline. `src/launch/supervisor.rs`
# therefore percent-encodes each one, and the decode here is `%` -> `\x` plus
# `printf %b`. That is safe only because the encoder emits `%XX` for every byte
# outside the unreserved set, a literal backslash included — so the allowlist is
# checked BEFORE the decode, never after.
#
# Every path is validated and every pair is compared before the first bind. That
# ordering is the plural shape's own requirement: binding as it goes would let
# declaration order decide whether a refusal arrives as a refusal or as a guest
# already half mirrored, and a partial mirror is worse than none because a session
# starting in the tree that did land cannot tell.
#
# This unit is the authoritative copy of a check the host makes twice already
# (`src/cli/lifecycle.rs` owns the diagnostic a user meets, `src/launch/spec.rs`
# backstops a stale specification). It is not a security boundary: the command
# line is written by the trusted host side and the guest is the untrusted party
# here, not the reverse. What it adds is that it reads the image that actually
# booted rather than a list maintained by hand on the other side of the build.
set -euo pipefail

internal_root=$VIVARIUM_WORKSPACES_INTERNAL_ROOT
table=$VIVARIUM_WORKSPACE_TABLE

# The guest journal does not leave the guest; the console does, and `console.log`
# is what the host has to read when a boot fails.
say() { echo "VIVARIUM_WORKSPACE_MIRROR=$*" >/dev/console; }
refuse() {
  say "refused tag=$1 reason=$2"
  exit 1
}

# Declared before the read, so `set -u` has an array to expand even if the read
# assigns nothing; and `|| true` because `read` reports failure at end of input,
# and whether `/proc/cmdline` ends in a newline is the kernel's business rather
# than a contract. Under `set -e` that return would abort here with no diagnostic
# at all, which is the one outcome this unit must never produce.
#
# Read once, outside the table loop, rather than once per tag: the command line
# does not change between tags, and re-reading it per row would make the cost
# quadratic in the number of declared trees for no gain.
tokens=()
read -r -a tokens </proc/cmdline || true

tags=()
internals=()
paths=()
encoded_paths=()

# Pass one: validate every row on its own. Nothing binds in here.
while read -r tag internal; do
  [ -n "$tag" ] || continue
  # The tag family is build-controlled (`lib.imap0` in `nix/guest.nix`), so this
  # is a skew check rather than input validation: a table row spelling anything
  # but `wsN` means the table and the share list were built by different
  # generations, and mirroring on that pairing would bind the wrong tree.
  case $tag in ws[0-9]*) ;; *) refuse "$tag" malformed-tag ;; esac
  case $tag in *[!a-z0-9]*) refuse "$tag" malformed-tag ;; esac
  [ "$internal" = "$internal_root/$tag" ] || refuse "$tag" malformed-internal-path
  findmnt --mountpoint "$internal" >/dev/null || refuse "$tag" internal-share-not-mounted

  encoded=
  for token in "${tokens[@]}"; do
    case $token in
      "vivarium.workspace.$tag="*)
        [ -z "$encoded" ] || refuse "$tag" duplicate-parameter
        encoded=${token#"vivarium.workspace.$tag="}
        ;;
    esac
  done
  if [ -z "$encoded" ]; then
    # No launch path for this share. The measurement legs and a hand-booted image
    # read their share at its internal path, so having nothing to mirror is not a
    # failure — refusing here would make the image bootable by nothing but `viv`.
    # Per tag rather than per boot, since the plural shape allows a mix.
    say "absent tag=$tag internal=$internal"
    continue
  fi
  # Before the decode, for the reason in the header. A truncated command line —
  # the kernel copies at most COMMAND_LINE_SIZE-1 bytes and drops the rest in
  # silence — usually lands here rather than decoding to a shorter, valid, wrong
  # path. Truncation is likelier now than it was with one workspace, which is why
  # `CMDLINE_LIMIT` in `src/launch/supervisor.rs` refuses host-side first.
  printf '%s' "$encoded" | grep -qxE '([A-Za-z0-9._~/-]|%[0-9A-F]{2})+' \
    || refuse "$tag" malformed-encoding
  # The `x` sentinel, and it is what makes the header's claim about newlines true.
  # Command substitution deletes every trailing newline from what it captures, so a
  # tree ending in one — which the encoder took the trouble to carry across as
  # `%0A` rather than refuse — would decode to a shorter path, bind there, and
  # leave the session's cwd naming a directory that does not exist. Appending one
  # byte and removing it again costs nothing and is exact: `%b` reads `\xHH` as at
  # most two hex digits, so the sentinel is never absorbed into the byte before it.
  path=$(printf '%b' "${encoded//%/\\x}x")
  path=${path%x}
  case $path in /*) ;; *) refuse "$tag" not-absolute ;; esac
  case $path in */ | */.. | */../* | */. | */./* | *//*) refuse "$tag" not-normalized ;; esac

  # Names this image owns whatever the running mount table happens to say. The
  # dynamic check below is the real guarantee; this list is what still refuses
  # `/etc/...` on a generation whose /etc is not a mount of its own. Matching is
  # `$owned` or `$owned/`, never a string prefix, so `/nix` does not refuse
  # `/nixos-projects`.
  #
  # Two whole subtrees are deliberately absent, and both for the same reason: a
  # host keeps real projects under them, so a blanket denial refuses ordinary
  # work. `/var`, because ostree hosts put home directories at `/var/home/<user>`.
  # And `/run`, because a removable drive is mounted at `/run/media/<user>/<label>`
  # on an ordinary desktop — measured, by this repository's own fixtures, which
  # live on exactly such a drive and were refused by the first version of this
  # list. What the guest owns under those two roots is named individually instead,
  # `$internal_root` among them, which is why it no longer needs appending here.
  for owned in /nix /proc /sys /dev /etc /boot /usr /bin /sbin /lib /lib64 \
    /root /tmp /var/lib /var/log /var/tmp /var/empty /home/vivarium \
    /run/vivarium /run/vivarium-mounts /run/vivarium-workspaces /run/user \
    /run/current-system /run/booted-system /run/wrappers /run/systemd \
    /run/udev /run/dbus /run/lock /run/log /run/keys /run/credentials \
    /run/binfmt /run/nscd /run/opengl-driver; do
    case $path in "$owned" | "$owned"/*) refuse "$tag" "under-guest-owned:$owned" ;; esac
    case $owned in "$path"/*) refuse "$tag" "parent-of-guest-owned:$owned" ;; esac
  done

  # Read off the booted image rather than off the list above, which is what makes
  # this the authority and that a convenience: whatever nobody enumerated still has
  # to land somewhere this unit may create a directory.
  #
  # Two filesystems qualify and no others. The guest's root, which is where an
  # ordinary mirror point is created; and any tmpfs, which is what `/run` is inside
  # the guest and therefore where a tree mounted from a removable drive lands.
  # Both are ephemeral and vivarium-created, so a directory tree left in either
  # costs nothing and disappears at shutdown. Everything else is refused by
  # construction — the home volume, the store overlay, an already-mounted share, and
  # the API filesystems — because a directory created in one of those either
  # persists into a user's own data or corrupts a mount the guest is made of.
  existing=$path
  while [ ! -e "$existing" ] && [ ! -L "$existing" ]; do existing=$(dirname -- "$existing"); done
  [ "$existing" != "$path" ] || refuse "$tag" mirror-path-already-exists
  [ ! -L "$existing" ] || refuse "$tag" ancestor-is-a-symlink
  [ -d "$existing" ] || refuse "$tag" ancestor-is-not-a-directory
  ancestor_mount=$(findmnt -no TARGET --target "$existing")
  ancestor_fstype=$(findmnt -no FSTYPE --target "$existing")
  case "$ancestor_mount:$ancestor_fstype" in
    /:*) ;;
    *:tmpfs) ;;
    *) refuse "$tag" "ancestor-on:$ancestor_mount:$ancestor_fstype" ;;
  esac

  tags+=("$tag")
  internals+=("$internal")
  paths+=("$path")
  encoded_paths+=("$encoded")
done <"$table"

# Pass two: the pairwise rule, which only exists because the workspace became
# plural. ADR-0108 gives every declared tree its own host-symmetric mirror, and
# two trees where one contains the other cannot both have one — the inner bind
# would land inside the outer share and be shadowed by it, so a session in the
# inner tree would silently read the outer tree's copy. Refused rather than
# ordered, because no order makes both promises true.
#
# Equality is covered by the first `case`: an identical pair matches `"${paths[i]}"`.
# Matching is `$p` or `$p/`, never a string prefix, so `/a` does not refuse `/ab`.
# Both encoded spellings go in the reason so the host can name both sides.
for ((i = 0; i < ${#paths[@]}; i++)); do
  for ((j = i + 1; j < ${#paths[@]}; j++)); do
    case ${paths[j]} in "${paths[i]}" | "${paths[i]}"/*)
      refuse "${tags[j]}" "nested-with:${tags[i]}:${encoded_paths[i]}:${encoded_paths[j]}"
      ;;
    esac
    case ${paths[i]} in "${paths[j]}"/*)
      refuse "${tags[i]}" "nested-with:${tags[j]}:${encoded_paths[i]}:${encoded_paths[j]}"
      ;;
    esac
  done
done

# Pass three. Everything above passed, so every bind below is expected to succeed
# and a failure here is a genuine fault rather than a declaration being judged.
for ((i = 0; i < ${#paths[@]}; i++)); do
  path=${paths[i]}
  internal=${internals[i]}
  tag=${tags[i]}
  install -d -m 0755 -- "$(dirname -- "$path")"
  # 0555 on the leaf, and it is the last of four guards against the same failure: a
  # bind that did not happen must leave a directory nothing can write to, rather
  # than an empty writable one that reads as the tree and keeps every edit on a
  # filesystem that vanishes at shutdown.
  install -d -m 0555 -- "$path"
  # `--rbind` rather than `--bind`: spec/06 promises the guest sees the host's own
  # filesystem boundaries, so if the daemon ever announces submounts they must come
  # across with the mount rather than be flattened away by this bind.
  mount --rbind -- "$internal" "$path" || {
    rmdir -- "$path"
    refuse "$tag" bind-failed
  }
  findmnt --mountpoint "$path" >/dev/null || refuse "$tag" mirror-not-mounted
  # The encoded path rather than the decoded one, because this marker goes to a console and
  # a console is lines. A tree's path may hold a space or a newline — the encoder carries
  # both on purpose, and the sentinel above exists so the newline survives the decode — and
  # a decoded path written here would either need a delimiter a reader has to guess or, for
  # a newline, split this marker across lines and defeat every reader of it. `$encoded` is
  # whitespace-free by construction and identical to the path itself for any ordinary one,
  # so it stays legible in `console.log` where a failed boot is read. `path=` is last and
  # consumers read it to end of line, which costs nothing and survives a later change here.
  say "ok tag=$tag internal=$internal path=${encoded_paths[i]}"
done
