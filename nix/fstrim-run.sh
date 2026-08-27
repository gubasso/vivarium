# shellcheck shell=bash
#
# Root's half of the agent's trim bridge (spec/12): consume the request the
# unprivileged agent wrote, trim each named mountpoint, and only then write the
# done marker the agent is waiting on — its acknowledgement promises completion,
# because the host reads the image's allocation right after it.

request=/run/vivarium/fstrim-requested
active=/run/vivarium/fstrim-active
done_marker=/run/vivarium/fstrim-done

# Aside first, so the path unit re-arms for a request that arrives mid-run.
mv "$request" "$active"
while IFS= read -r mount; do
  # The agent opens the request with a '# <token>' line; blank and token lines
  # are not mountpoints.
  case "$mount" in '' | '#'*) continue ;; esac
  # A mountpoint that cannot be trimmed is not a fault: the periodic in-guest
  # trim tolerates the same, and the host measures the outcome on the image's
  # own allocated blocks rather than on this status (ADR-0037).
  fstrim "$mount" || true
done <"$active"
# The processed request becomes the marker, token line included, so the agent
# can tell whose trim completed rather than trusting bare existence.
mv "$active" "$done_marker"
