# One manifest-declared volume, so the guest half of `[[volumes]]` is asserted at
# build time rather than only by a boot that happens to run with a manifest.
#
# The shipped image composes no tool option surface, so `vivarium.volumes` does not
# exist in it and `guest.nix` reads `config.vivarium.volumes or [ ]`. That is the
# right shape for the product and it leaves the declared-volume path exercised by
# nothing — every assertion about ordering, labels, mounting, and first-boot
# ownership would hold vacuously over an empty list. This fixture supplies the
# missing half by composing the very module a generated flake embeds, so what is
# checked here is the option surface a user's sandbox actually merges against.
#
# `size_gib` is deliberately absent: the ceiling then comes from
# `defaultVolumeSizeMiB`, which is the arm nothing else covers.
#
# Applied by the caller rather than taking `optionsModule` from `specialArgs`: the
# product's `specialArgs` are the shipped image's own, and widening them so a
# fixture can reach one file would put a verification argument in scope for every
# module of every guest.
{ optionsModule }:

{
  imports = [ optionsModule ];

  vivarium.volumes = [
    {
      name = "cache";
      mount = "/var/cache/project";
    }
  ];
}
