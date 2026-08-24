{ lib, ... }:

# The tool-owned option surface every composed layer merges against.
#
# This module is copied into each generated flake rather than imported from a
# flake input: ADR-0058 makes that tree self-contained and regenerable, and a
# reference out of it would make this repository a build input of every user's
# sandbox. `src/config/flake.rs` embeds the text and renders it as
# `vivarium-options.nix`, first in the module list so no layer can be the thing
# that declares the option it sets.
#
# Two namespaces, because spec/04 separates two channels and ADR-0041 decides
# membership per option rather than by prefix:
#
#   `vivarium.*` — the launch channel. The tool reads these by pure evaluation
#   and applies them when the VM starts. `vivarium.volumes` is the exception
#   ADR-0041 names: a volume's guest mountpoint is guest system configuration,
#   so only its host image path and virtual size resolve at launch.
#
#   `sandbox.*` — declared policy the guest system is built against.
#
# Every option is typed rather than free-form so a malformed declaration fails
# at evaluation with a precise message instead of at boot (spec/04). Nothing
# here has a `config` half: the options exist to be merged and read, and a
# default that produced a value would make "undeclared" indistinguishable from
# "declared as the default" — a distinction spec/03 requires the readers keep.

let
  inherit (lib) mkOption types;

  # Host-side variables in a mount source stay unexpanded through evaluation and
  # resolve against the host environment only at launch (spec/04, spec/07). That
  # is what lets one shared piece be adopted by two people, so the type is a
  # plain string and no expansion happens here.
  mountModule = {
    options = {
      source = mkOption {
        type = types.str;
        description = "Host path, with host-side variables left unexpanded.";
      };
      target = mkOption {
        type = types.str;
        description = "Guest path the source is mirrored to.";
      };
      readonly = mkOption {
        type = types.bool;
        default = false;
        description = "Whether the guest sees the mount read-only.";
      };
    };
  };

  volumeModule = {
    options = {
      name = mkOption {
        type = types.str;
        description = "Volume name, unique across the merged configuration.";
      };
      mount = mkOption {
        type = types.str;
        description = "Guest mountpoint. Build channel: changing it rebuilds.";
      };
      size_gib = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = "Virtual size ceiling in GiB, or null for the default.";
      };
    };
  };
in
{
  options = {
    vivarium = {
      env = mkOption {
        type = types.attrsOf types.str;
        default = { };
        description = "Environment variables the guest session starts with.";
      };

      mounts = mkOption {
        type = types.listOf (types.submodule mountModule);
        default = [ ];
        # Every host path that crosses, including the trees a manifest declares as
        # `[[workspaces]]`: those compile to a row whose `target` is its own expanded
        # `source` (ADR-0110), so there is no second option and the guest holds no
        # notion of a workspace. Ownership stays a question `viv` answers from manifest
        # text, which is why nothing here records which rows came from that table.
        description = "Host paths mirrored into the guest. Layers concatenate.";
      };

      resources = {
        # Null rather than a number, and deliberately so: an undeclared ceiling
        # is resolved from the host at launch under spec/17's auto-sizing, and
        # `0` would be a different and wrong answer. The readers render null.
        mem_mib = mkOption {
          type = types.nullOr types.ints.positive;
          default = null;
          description = "Memory ceiling in MiB, or null to size from the host.";
        };
        vcpu = mkOption {
          type = types.nullOr types.ints.positive;
          default = null;
          description = "Virtual CPU ceiling, or null to size from the host.";
        };
      };

      volumes = mkOption {
        type = types.listOf (types.submodule volumeModule);
        default = [ ];
        description = "Volumes beyond the home volume. Layers concatenate.";
      };

      volume = {
        size_gib = mkOption {
          type = types.nullOr types.ints.positive;
          default = null;
          description = "Home-volume size ceiling in GiB, or null for the default.";
        };
        persist = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Guest paths the home volume keeps across a rebuild.";
        };
      };
    };

    sandbox.egress = {
      mode = mkOption {
        type = types.enum [
          "open"
          "allowlist"
        ];
        default = "open";
        description = "Whether outbound traffic is unrestricted or filtered.";
      };
      # Concatenates, which is the merge semantics spec/04 fixes for lists: every
      # layer contributing a destination adds one, and no layer silently drops
      # another's. A layer that must narrow the policy does it through `mode`.
      allow = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Reachable destinations under allowlist mode.";
      };
    };
  };
}
