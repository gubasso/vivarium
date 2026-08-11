# A single-concern piece: outbound traffic reaches the declared hosts only.
#
# This is the shape of a team guarantee. `lib.mkForce` is the hard floor of the
# priority convention (spec/04): no personal manifest outranks it, which is the
# point — a security policy that a manifest could switch off would not be one.
# Use it for policy, never for taste.
#
# The allowlist itself concatenates rather than replacing, so adopting this
# piece adds to whatever your manifest already allows instead of discarding it.
{ lib, ... }:
{
  sandbox.egress = {
    mode = lib.mkForce "allowlist";
    allow = [
      "crates.io"
      "static.crates.io"
      "index.crates.io"
      "github.com"
    ];
  };
}
