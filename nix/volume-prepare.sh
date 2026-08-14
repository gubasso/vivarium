# shellcheck shell=bash
#
# First boot of every volume the guest owns (spec/06, ADR-0067).
#
# A volume image is created lazily, so the first `viv start` that needs one attaches
# an empty filesystem owned by the guest's system identity — which would leave the
# default volume, mounted at the guest user's home, unwritable by the user who lives
# there, and a declared volume unwritable by the session that asked for it. This is
# the declarative fix, applied after the volumes mount and ordered ahead of the agent
# so no session ever observes the unowned state.
#
# One unit for every volume rather than one unit each, because the alternative makes
# the agent's ordering edge a list that has to be regenerated whenever a manifest
# declares a volume, and makes the marker rule below something written more than once.
# The cost is that one failed mount fails preparation for all of them, hence the agent,
# hence the boot — which is the correct posture: `RequiresMountsFor` is what stops this
# running over an empty directory where a mount should have been, and a chown that
# succeeds there produces a mount point that looks right and loses every write.
set -euo pipefail
table=$VIVARIUM_VOLUME_TABLE

# `<mount> <owner> <mode> <seed>`, built by the guest module from the volume
# DECLARATIONS rather than by filtering the attached volumes. That is what keeps the
# store volume out by construction (spec/06 exempts it): it was never declared, so it
# cannot appear here, and no filter can be weakened into including it. It also mounts
# in the initrd, before the guest has an identity to own it with.
while read -r mount owner mode seed; do
  [ -n "$mount" ] || continue
  marker=$mount/.vivarium-first-boot

  # The marker lives inside the volume, so a rebuild or a later boot is not a first
  # boot and nothing below runs again over data the user has since written.
  #
  # `-L` beside `-e`, and it is load-bearing rather than defensive: the volume belongs
  # to the guest user after this runs, so that user may replace this marker with a
  # symlink to any path it likes. A dangling symlink reads as absent to `-e` alone, and
  # the `touch` and `chown` at the foot of this loop would then follow it — this unit
  # runs as root, so that is a root-created file at a path the guest chose, handed to
  # the guest user. spec/12 keeps the session non-root; without this test the marker is
  # the way around that. A symlink here also means the volume is not empty, so a first
  # boot is not what this is, and skipping is the correct reading as well as the safe
  # one.
  if [ -e "$marker" ] || [ -L "$marker" ]; then
    continue
  fi

  # The mount point and nothing under it. Repairing ownership recursively would be
  # proportional to a volume that legitimately reaches tens of gibibytes, on every
  # boot, and would overwrite ownership the user set deliberately.
  chown "$owner" "$mount"
  chmod "$mode" "$mount"

  # Seeded once from the image's skeleton, which is the other half of spec/06's
  # first-boot rule: NixOS seeds a home when it creates the account, and this home is
  # a volume that did not exist then. Only the home volume asks for it — a declared
  # volume is a place for a cache or a build tree, not a login. The recursive chown
  # here is bounded by the skeleton rather than by the volume — it runs on this boot
  # only, over a volume that holds nothing else yet — so it does not reintroduce what
  # the paragraph above refuses. The shipped image ships no skeleton, so this copies
  # nothing today; an image or piece that adds one gets it.
  if [ "$seed" = 1 ]; then
    for entry in /etc/skel/* /etc/skel/.[!.]*; do
      [ -e "$entry" ] || continue
      cp -a -- "$entry" "$mount/"
      chown -R "$owner" "$mount/$(basename -- "$entry")"
    done
  fi

  touch "$marker"
  chown "$owner" "$marker"
done <"$table"
