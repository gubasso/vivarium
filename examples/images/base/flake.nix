# The base flake: the file that makes this image's build inputs yours.
#
# vivarium's generated flake takes this file as a real flake input named after
# the image, and follows `nixpkgs` and `microvm` out of it when it declares
# them (ADR-0112). Change either URL and your guest — kernel, packages, and
# hypervisor — resolves from your reference, pinned by the lock only
# `viv update` moves. Delete this file and the image is an ordinary module
# layer again, built from vivarium's shipped references.
#
# Rules worth knowing before editing:
# - Declare an input here with an explicit `inputs.<name>.url`. An `outputs`
#   argument alone also counts as a declaration, but only the explicit form
#   says what it resolves to.
# - Keep the `follows` line whenever you declare `microvm`: without it the
#   guest and its shares build from two different `nixpkgs` trees, which
#   vivarium refuses (`lock.baseline-split`).
# - A `flake.lock` beside this file is your own pin statement: it seeds every
#   re-resolution, and `viv update` moves nothing past it until it moves.
# - URLs land in the generated flake, the lock, and the world-readable Nix
#   store — a reference embedding a credential is a build-time secret (N10),
#   exactly as a token in a manifest is.
#
# Your module layer (`default.nix`) can reach this flake's outputs as
# `vivariumInputs.<image-name>.<attr>` — declare an input here, expose it in
# `outputs`, and the layer can fold it into the guest.
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = _: { };
}
