# The measurement image's leg composition (ADR-0095).
#
# Every probe vivarium has ever booted lives under this directory and reaches an
# image only through this file. The base image — `legs = [ ]` — composes nothing,
# which is the whole point: a base image that boots a measurement service is a
# base image shipping a probe to users.
#
# **Composition owns ordering and stopping, not the leg bodies.** The legs used
# to name each other in `after =`/`before =`, which meant an image built without
# one silently changed the run topology — systemd treats an ordering edge to an
# absent unit as a no-op, so it composes cleanly and says nothing. Ordering is
# therefore derived here from the `legs` list, and a leg body names only units it
# genuinely depends on (mounts, the daemon socket).
#
# Stopping is composed for the same reason. The poweroff used to live inside the
# diagnostic leg, so an image selecting any other leg would boot and sit at
# `multi-user.target` forever. One stop unit, ordered after every selected leg,
# with **no `Requires=`** in either direction: a failed probe is that leg's
# finding and must never strand the machine.
{
  lib,
  storeLayout,
  storeCanaryExpression,
  storeFreeSpaceHook,
  workspacesInternalRoot,
  legs,
}:

let
  # The declared order is the run order. Adding a leg means one entry here, one
  # in `legModules`, one in `legUnit`, and the file. Unit names are derived
  # here once and carry their `.service` suffix for contract consumers.
  legOrder = [
    "spike"
    "gc-interlock"
    "pressure"
    "bench"
    "diagnostic"
  ];

  legModules = {
    spike = ./store-spike.nix;
    gc-interlock = ./gc-interlock.nix;
    pressure = ./store-pressure.nix;
    bench = ./benchmark.nix;
    diagnostic = ./diagnostic.nix;
  };

  legUnit = {
    spike = "vivarium-store-spike";
    gc-interlock = "vivarium-gc-interlock";
    pressure = "vivarium-store-pressure";
    bench = "vivarium-share-benchmark";
    diagnostic = "vivarium-first-microvm-diagnostic";
  };

  unknown = lib.subtractLists legOrder legs;

  # Selected legs in *declared* order, not in the order they were asked for, so
  # `[ "diagnostic" "pressure" ]` builds the same image as the reverse rather
  # than a differently-ordered one.
  selected = lib.filter (l: lib.elem l legs) legOrder;

  # The extra arguments the leg modules take beyond the standard module set.
  legArgs = {
    _module.args = {
      inherit storeLayout storeCanaryExpression storeFreeSpaceHook;
      # Derived from the guest's own share list rather than spelled a fourth
      # time: the leg asserts against where ws0 actually mounted.
      verificationWorkspaceInternal = "${workspacesInternalRoot}/ws0";
    };
  };

  # Chain the selected legs in declared order: each is ordered after the one
  # before it and nothing else, so removing a leg from the middle closes the gap
  # rather than leaving an edge pointing at an absent unit.
  chain = lib.imap0 (
    i: leg:
    lib.optionalAttrs (i > 0) {
      systemd.services.${legUnit.${leg}}.after = [ "${legUnit.${lib.elemAt selected (i - 1)}}.service" ];
    }
  ) selected;

  # One owner of poweroff, and it is unconditional. `After=` every selected leg
  # with no `Requires=`, so a leg that fails, times out or is killed still
  # reaches a stop. It emits a marker before stopping, because a machine that
  # powers off without saying so is indistinguishable from one that crashed.
  stopUnit =
    { pkgs, ... }:
    {
      systemd.services.vivarium-measurement-stop = {
        enableStrictShellChecks = true;
        environment.VIVARIUM_MEASUREMENT_LEGS = lib.concatStringsSep "," selected;
        description = "Stop the measurement image once every selected leg has run";
        wantedBy = [ "multi-user.target" ];
        after = map (l: "${legUnit.${l}}.service") selected;
        serviceConfig = {
          Type = "oneshot";
          StandardOutput = "journal";
          StandardError = "journal";
          # Bounds only the stop itself: every leg carries its own
          # `TimeoutStartSec`, and this unit is ordered after all of them.
          TimeoutStartSec = "120s";
        };
        path = [
          pkgs.coreutils
          pkgs.systemd
        ];
        script = builtins.readFile ./measurement-stop.sh;
      };
    };
in

lib.throwIf (unknown != [ ])
  "vivarium measurement: unknown leg(s) ${lib.concatStringsSep ", " unknown} (known: ${lib.concatStringsSep ", " legOrder})"
  {
    modules = lib.optionals (selected != [ ]) (
      [ legArgs ] ++ map (l: legModules.${l}) selected ++ chain ++ [ stopUnit ]
    );
    # The contract sorts the union before rendering, so selected-order here
    # is not load-bearing for derivation identity.
    units = lib.optionals (selected != [ ]) (
      map (l: "${legUnit.${l}}.service") selected ++ [ "vivarium-measurement-stop.service" ]
    );
  }
