# A single-concern piece: everything a Rust project needs and nothing else.
#
# Pieces are the shareable layer, so this one is written to be adoptable by
# someone whose manifest you have never seen. Two habits make that work:
#
#   Propose with `lib.mkDefault`, so a manifest can still decide. Two pieces
#   that both set a scalar at normal priority collide, and priority never breaks
#   a tie — it fails evaluation instead (spec/04, ADR-0042).
#
#   Carry no personal data. A literal path like `/home/ana/...` in a shared
#   layer is refused at evaluation (N11); `${HOME}` stays unexpanded through the
#   merge and resolves against the host at launch, which is what makes the same
#   piece portable between two people.
{ lib, pkgs, ... }:
{
  environment.systemPackages = [
    pkgs.cargo
    pkgs.rustc
    pkgs.rust-analyzer
  ];

  vivarium.env.RUST_BACKTRACE = lib.mkDefault "1";

  vivarium.mounts = [
    {
      source = "\${HOME}/.cargo/registry";
      target = "~/.cargo/registry";
      readonly = false;
    }
  ];
}
