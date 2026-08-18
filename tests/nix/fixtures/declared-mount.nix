# One read-write and one read-only manifest-declared mount, so the guest half of
# `[[mounts]]` is asserted at build time rather than only by a boot that happens
# to run with a manifest (slice 019; the `declared-volume.nix` twin).
#
# The shipped image composes no tool option surface, so `vivarium.mounts` does
# not exist in it and `guest.nix` reads `config.vivarium.mounts or [ ]` — every
# assertion about derived shares, socket tokens, read-only flags, and the bind
# unit's table would hold vacuously over an empty list. This fixture supplies
# the missing half by composing the very module a generated flake embeds.
#
# Both sources are variable-shaped on purpose: a source travels to the launcher
# verbatim and unexpanded (N19, ADR-0020), and the contract compares the
# launcher's `sourceToken` against this declaration's own string. One target is
# `~`-relative and one absolute, so both arms of the guest-side expansion are
# built. Sources and targets are whitespace-free because the contract's row
# format is.
{ optionsModule }:

{
  imports = [ optionsModule ];

  vivarium.mounts = [
    {
      source = "\${HOME}/.config/declared";
      target = "~/.config/declared";
      readonly = false;
    }
    {
      source = "\${XDG_DATA_HOME}/declared.toml";
      target = "/workspaces/declared.toml";
      readonly = true;
    }
  ];
}
