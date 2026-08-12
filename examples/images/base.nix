# A base image: the guest system every piece and manifest layers onto.
#
# Copy it into `$XDG_CONFIG_HOME/vivarium/images/` and edit it. It is an example
# rather than something vivarium resolves: the config root is the whole search
# path, so nothing here is found by name until you have copied it
# (ADR-0061, ADR-0045).
#
# Values here use `lib.mkDefault` because that is the role an image holds in the
# merge: a base default, which a piece may propose against and which your own
# manifest always outranks (spec/04). Set something without `mkDefault` here and
# a piece proposing the same value becomes an equal-priority tie instead.
{ lib, ... }:
{
  microvm = {
    hypervisor = lib.mkDefault "cloud-hypervisor";
    mem = lib.mkDefault 2048;
    vcpu = lib.mkDefault 2;
    balloon = lib.mkDefault true;
  };

  networking.hostName = lib.mkDefault "vivarium";

  # The `vivarium` guest account is deliberately absent here. Its uid and gid are
  # the share identity contract — fixed when the image is built and identical on
  # every host (spec/06) — so the guest module vivarium composes beneath your
  # layers owns the account, and an image that redeclared it would be proposing a
  # different value for the one field the uid/gid translation reads.

  # Open by default, which is a deliberate policy rather than an oversight: a
  # sandbox that cannot fetch a dependency is a sandbox nobody uses, and the
  # boundary it protects is the host, not the network (ADR-0007). Restrict it in
  # a piece or in your manifest.
  sandbox.egress.mode = lib.mkDefault "open";

  # The release this guest's stateful defaults follow. It is not "the version to
  # keep current" — moving it changes how existing state is interpreted.
  system.stateVersion = lib.mkDefault "25.05";
}
