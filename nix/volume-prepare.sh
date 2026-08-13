# shellcheck shell=bash
#
# First boot of the home volume (spec/06, ADR-0067).
#
# A volume image is created lazily, so the first `viv start` that needs one attaches
# an empty filesystem owned by the guest's system identity — which would leave the
# default volume, mounted at the guest user's home, unwritable by the user who lives
# there. This is the declarative fix, applied after the volume mounts and ordered
# ahead of the agent so no session ever observes the unowned state.
set -euo pipefail
home=$VIVARIUM_HOME
owner=$VIVARIUM_HOME_OWNER
marker=$home/.vivarium-first-boot

# The marker lives inside the volume, so a rebuild or a later boot is not a first
# boot and nothing below runs again over data the user has since written.
#
# `-L` beside `-e`, and it is load-bearing rather than defensive: the home is the
# guest user's own volume, so that user may replace this marker with a symlink to
# any path it likes. A dangling symlink reads as absent to `-e` alone, and the
# `touch` and `chown` at the foot of this file would then follow it — this unit runs
# as root, so that is a root-created file at a path the guest chose, handed to the
# guest user. spec/12 keeps the session non-root; without this test the marker is
# the way around that. A symlink here also means the volume is not empty, so a
# first boot is not what this is, and exiting is the correct reading as well as the
# safe one.
if [ -e "$marker" ] || [ -L "$marker" ]; then
  exit 0
fi

# The mount point and nothing under it. Repairing ownership recursively would be
# proportional to a home that legitimately reaches tens of gibibytes, on every boot,
# and would overwrite ownership the user set deliberately.
chown "$owner" "$home"
chmod 0700 "$home"

# Seeded once from the image's skeleton, which is the other half of spec/06's
# first-boot rule: NixOS seeds a home when it creates the account, and this home is
# a volume that did not exist then. The recursive chown here is bounded by the
# skeleton rather than by the home — it runs on this boot only, over a home that
# holds nothing else yet — so it does not reintroduce what the paragraph above
# refuses. The shipped image ships no skeleton, so this copies nothing today; an
# image or piece that adds one gets it.
for entry in /etc/skel/* /etc/skel/.[!.]*; do
  [ -e "$entry" ] || continue
  cp -a -- "$entry" "$home/"
  chown -R "$owner" "$home/$(basename -- "$entry")"
done

touch "$marker"
chown "$owner" "$marker"
