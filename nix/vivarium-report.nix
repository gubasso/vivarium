{
  lib,
  pkgs,
  vivariumInputs,
  optionsModule,
  layers,
  config,
}:

# The one evaluation both config readers consume: every tracked key's effective
# value, and every layer's own contribution to it.
#
# Rendered into each generated flake beside `vivarium-options.nix` and reached as
# `vivariumReport.<system>`. `viv config eval` and `viv config sources` read the
# same attribute and differ only in what they do with it — the first refuses when
# a conflict is present, the second reports it and exits `0` (ADR-0042). Two
# evaluations would let the two commands disagree about the same tree.
#
# Two things here are load-bearing and neither is obvious.
#
# Contributions are collected by evaluating each layer on its own against the
# options module, not by reading the merged system's definition list. That list
# is post-`filterOverrides`: nixpkgs drops every definition a higher priority
# shadowed before `definitionsWithLocations` is assembled, so the shadowed
# `mkDefault` a provenance view exists to show is already gone. Evaluated alone,
# a layer's own definition always survives, and `highestPrio` then reports the
# priority it was set at. `_module.check = false` is what lets an ordinary NixOS
# layer be evaluated against the vivarium options alone: its declarations for
# options this module does not declare are ignored rather than fatal.
#
# Every value is forced inside `builtins.tryEval` before it leaves. A tracked key
# that throws — an equal-priority tie is exactly that — must come back as
# `ok = false` rather than take the whole report with it, because rendering at
# that moment is the entire point of `config sources`. `deepSeq` is required with
# it: `tryEval` catches only what forcing the outer value raises, and a list of
# submodules raises nothing until its elements are forced.

let
  # `mkOptionDefault`, which is the priority the module system gives an option's own `default`
  # when it injects it as a definition. Every option here has one, so `isDefined` is true even for
  # a layer that mentions nothing: a contribution is a definition that outranks that injected
  # default, and this is the number that separates the two.
  optionDefaultPrio = 1500;

  # Forces `value` completely and reports whether it survived.
  probe =
    value:
    let
      attempt = builtins.tryEval (builtins.deepSeq value value);
    in
    if attempt.success then
      {
        ok = true;
        inherit (attempt) value;
      }
    else
      {
        ok = false;
        value = null;
      };

  # The public key names are the manifest's own, which is the language spec/01
  # renders both readers in: `resources.mem_mib`, never `vivarium.resources.mem_mib`.
  scalarPaths = {
    "resources.mem_mib" = [
      "vivarium"
      "resources"
      "mem_mib"
    ];
    "resources.vcpu" = [
      "vivarium"
      "resources"
      "vcpu"
    ];
    "sandbox.egress.mode" = [
      "sandbox"
      "egress"
      "mode"
    ];
    "volume.size_gib" = [
      "vivarium"
      "volume"
      "size_gib"
    ];
  };

  listPaths = {
    "mounts" = [
      "vivarium"
      "mounts"
    ];
    "volumes" = [
      "vivarium"
      "volumes"
    ];
    "sandbox.egress.allow" = [
      "sandbox"
      "egress"
      "allow"
    ];
    "volume.persist" = [
      "vivarium"
      "volume"
      "persist"
    ];
  };

  envPath = [
    "vivarium"
    "env"
  ];

  # A submodule's value carries `_module` alongside the authored fields, and
  # `_module.args` holds functions. Forcing it would throw and serializing it
  # could not produce JSON, so each list of submodules is projected down to the
  # fields the manifest surface actually has.
  projectors = {
    "mounts" = mount: {
      inherit (mount) source target readonly;
    };
    "volumes" = volume: {
      inherit (volume) name mount size_gib;
    };
  };

  project =
    key: value:
    if projectors ? ${key} && builtins.isList value then map projectors.${key} value else value;

  # The merged side. `config` is the evaluated system, so a tracked key that the
  # merge cannot produce throws here and is caught rather than propagated.
  mergedAt =
    key: path:
    let
      probed = probe (lib.attrByPath path null config);
    in
    probed // { value = if probed.ok then project key probed.value else null; };

  mergedEnvNames =
    let
      attempt = builtins.tryEval (builtins.attrNames (lib.attrByPath envPath { } config));
    in
    if attempt.success then attempt.value else [ ];

  # One layer, evaluated against the option surface and nothing else.
  layerOptions =
    layer:
    let
      evaluated = lib.evalModules {
        modules = [
          optionsModule
          { _module.check = false; }
          layer.module
        ];
        specialArgs = {
          inherit pkgs vivariumInputs;
        };
      };
      attempt = builtins.tryEval evaluated.options;
    in
    if attempt.success then attempt.value else null;

  # This layer's own definition of one key, or null when it does not set it.
  definitionAt =
    options: key: path:
    let
      attempt = builtins.tryEval (
        let
          option = lib.attrByPath path null options;
        in
        if option == null || !option.isDefined || option.highestPrio >= optionDefaultPrio then
          null
        else
          {
            prio = option.highestPrio;
            value = project key (builtins.deepSeq option.value option.value);
          }
      );
    in
    if options == null || !attempt.success then null else attempt.value;

  # `vivarium.env` is expanded per name because a tie is a property of one key,
  # not of the attrset holding it: two layers setting different variables collide
  # in no way at all, and reporting them at the option level would invent a
  # conflict. Per-name priorities need `mergeAttrDefinitionsWithPrio`, since a
  # `mkDefault` inside the attrset leaves the attrset itself at normal priority.
  envDefinitions =
    options:
    let
      attempt = builtins.tryEval (
        let
          option = lib.attrByPath envPath null options;
        in
        if option == null || !option.isDefined then
          { }
        else
          lib.mapAttrs'
            (
              name: entry:
              lib.nameValuePair "env.${name}" {
                prio = entry.highestPrio;
                value = builtins.deepSeq entry.value entry.value;
              }
            )
            (
              lib.filterAttrs (_: entry: entry.highestPrio < optionDefaultPrio) (
                lib.modules.mergeAttrDefinitionsWithPrio option
              )
            )
      );
    in
    if options == null || !attempt.success then { } else attempt.value;

  layerReport =
    layer:
    let
      options = layerOptions layer;
      fixed = lib.mapAttrs (key: path: definitionAt options key path) (scalarPaths // listPaths);
    in
    {
      inherit (layer) name kind;
      readable = options != null;
      defines = lib.filterAttrs (_: definition: definition != null) fixed // envDefinitions options;
    };

  reported = map layerReport layers;

  # Every name any layer mentions, so a key survives in the view even when the
  # merged attrset is the thing that failed to evaluate.
  envNames = lib.unique (
    mergedEnvNames
    ++ map (lib.removePrefix "env.") (
      builtins.filter (key: lib.hasPrefix "env." key) (
        lib.concatMap (layer: builtins.attrNames layer.defines) reported
      )
    )
  );
in
{
  keys =
    lib.mapAttrs (key: path: { class = "scalar"; } // mergedAt key path) scalarPaths
    // lib.mapAttrs (key: path: { class = "list"; } // mergedAt key path) listPaths
    // builtins.listToAttrs (
      map (name: {
        name = "env.${name}";
        value = {
          class = "scalar";
        }
        // mergedAt "env.${name}" (envPath ++ [ name ]);
      }) envNames
    );
  layers = reported;
}
