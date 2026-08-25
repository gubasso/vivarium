# ADR-0111's rule, made executable: a verification image extends the base image.
#
# The rule exists because a check is only worth what its subject is. Every image
# here is built by `product.mkImage`, which appends modules rather than replacing
# them — but "is built by" is a property of the code as written, not of the code as
# it will be. A variant that quietly replaced a setting instead of declaring an
# override would evaluate cleanly, boot, and produce a measurement of itself. This
# is what notices.
#
# Three comparisons, chosen because each catches a different way to stop extending:
#
#   settings — a variant may differ only on keys it named in its own override set.
#   Catches the drift where a default moves under an image that never asked.
#
#   shares and volumes — every base entry must survive into the variant WITH ITS
#   CONTENT, not merely with its name. Catches a module that redefines
#   `microvm.shares`, and equally one that keeps the `store` tag while
#   `lib.mkForce`-ing what it points at — which is the single most consequential
#   replacement available, since the store share is how the guest has a store at
#   all. Identity-only comparison would have let that through.
#
#   units — every base service name must survive. Catches a leg that captures a
#   unit name the product also uses, which would make the probe and the product
#   the same name and the measurement meaningless.
#
# Two exclusions, both measured rather than assumed, because an exclusion nobody
# checked is how a check comes to pass vacuously.
#
#   `extraModules` is the mechanism of extension, so every variant overrides it by
#   construction; asserting on it would fail for every image and prove nothing.
#
#   A volume's `size` is compared like every other field, but a difference is
#   forgiven when — and only when — that volume's own size key appears in the
#   variant's declared set: `homeVolumeSizeMiB` for the home volume,
#   `storeVolumeSizeMiB` for the store volume. `scaled` declares the second, so its
#   store volume legitimately differs in exactly that field and in no other.
#
#   Excluding `size` unconditionally was the first attempt and it was wrong. The
#   settings arm compares a variant's declared INPUTS against the base image's; it
#   never looks at the resolved guest volume. A module could therefore force-write
#   `microvm.volumes` with a changed size while leaving `storeVolumeSizeMiB` alone,
#   and nothing would have complained — the settings arm cannot see it, and the
#   volume arm had been told to look away. Forgiving the field per volume, against
#   that volume's own key, is what makes the two arms meet.
#
# Unit CONTENT is deliberately not compared, and this is the honest limit of the
# check rather than an oversight. Measured across every variant: `vivarium-agent`
# differs in all of them, because each verification image declares a tree and the
# agent unit orders itself against the mounts; `nix-daemon` differs under declared
# store thresholds; `dbus-broker` and `vivarium-volume-prepare` differ under an
# added package and an added volume. Every one of those is a legitimate consequence
# of appending, so a content comparison here would fail for its own reasons on every
# image — the rabbit hole `ADR-0111`'s slice named. What that leaves uncaught is a
# module that force-rewrites a base unit while keeping its name; `ADR-0111` records
# the gap rather than implying it is closed.
{
  lib,
  pkgs,
  product,
  base,
  variants,
}:

let
  # The base image's own vocabulary, minus the seam. Read off the base rather than
  # off `imageDefaults` so this file has no second opinion about what a setting is.
  comparedKeys = lib.filter (key: key != "extraModules") (builtins.attrNames base.settings);

  namesOf = attr: field: map (entry: entry.${field}) attr;

  driftFor =
    name: variant:
    let
      undeclared = lib.filter (
        key: !(lib.elem key variant.declaredKeys) && variant.settings.${key} != base.settings.${key}
      ) comparedKeys;

      missingShares = lib.subtractLists (namesOf variant.guest.config.microvm.shares "tag") (
        namesOf base.guest.config.microvm.shares "tag"
      );

      missingVolumes = lib.subtractLists (namesOf variant.guest.config.microvm.volumes "label") (
        namesOf base.guest.config.microvm.volumes "label"
      );

      # A base entry the variant still names, but no longer IS. `findFirst` returning
      # null means the entry is gone entirely, which the two lists above already report,
      # so this arm speaks only about entries that survived by name.
      # `verdictFor` returns both halves of the judgement on one surviving base entry:
      # `exclude` names the fields the field-by-field comparison must skip, and `agrees`
      # is the separate judgement made on exactly those fields. A forgiven field is still
      # checked — against what the variant declared, rather than against the base image.
      replaced =
        baseEntries: entries: field: verdictFor:
        map (entry: entry.${field}) (
          lib.filter (
            baseEntry:
            let
              match = lib.findFirst (candidate: candidate.${field} == baseEntry.${field}) null entries;
              verdict = verdictFor baseEntry;
            in
            match != null
            && (
              (builtins.removeAttrs match verdict.exclude) != (builtins.removeAttrs baseEntry verdict.exclude)
              || !(verdict.agrees match)
            )
          ) baseEntries
        );

      # The size key each reserved volume answers to. A volume whose label is neither
      # is a declared one, which by definition is not in the base image and so never
      # reaches this comparison.
      sizeKeyFor = {
        "${product.volumeLabel}" = "homeVolumeSizeMiB";
        "${product.storeVolumeLabel}" = "storeVolumeSizeMiB";
      };

      replacedShares =
        replaced base.guest.config.microvm.shares variant.guest.config.microvm.shares "tag"
          (_: {
            exclude = [ ];
            agrees = _: true;
          });

      replacedVolumes =
        replaced base.guest.config.microvm.volumes variant.guest.config.microvm.volumes "label"
          (
            baseVolume:
            let
              key = sizeKeyFor.${baseVolume.label} or null;
              declared = key != null && lib.elem key variant.declaredKeys;
            in
            {
              exclude = lib.optional declared "size";
              # Judged apart from the field comparison, and only where that comparison was
              # told to skip it: the resolved size must be the size the variant asked for.
              agrees = match: !declared || match.size == variant.settings.${key};
            }
          );

      missingUnits = lib.subtractLists (builtins.attrNames variant.guest.config.systemd.services) (
        builtins.attrNames base.guest.config.systemd.services
      );

      complaint =
        kind: items: lib.optional (items != [ ]) "${name}: ${kind} ${lib.concatStringsSep ", " items}";
    in
    (complaint "varies on undeclared setting(s)" undeclared)
    ++ (complaint "drops base share(s)" missingShares)
    ++ (complaint "replaces the content of base share(s)" replacedShares)
    ++ (complaint "drops base volume(s)" missingVolumes)
    ++ (complaint "replaces the content of base volume(s)" replacedVolumes)
    ++ (complaint "drops base service(s)" missingUnits);

  drift = lib.concatLists (lib.mapAttrsToList driftFor variants);
in
lib.throwIf (drift != [ ])
  ''
    vivarium verification: image(s) replace the base image rather than extending it.

    ${lib.concatStringsSep "\n    " drift}

    A verification image varies the base image only through keys it declares and
    modules it appends (ADR-0111). Declare the key in the image's own override set,
    or append rather than replace.
  ''
  (
    pkgs.runCommand "base-image-extends" { } ''
      touch "$out"
    ''
  )
