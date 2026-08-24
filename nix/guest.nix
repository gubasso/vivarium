{
  config,
  lib,
  pkgs,
  storeLayout,
  volumeLabel,
  storeVolumeLabel,
  volumeDirSentinel,
  homeVolumeName,
  storeVolumeName,
  guestAgentPackage,
  networkLayout,
  # Variant scalars (ADR-0095). The defaults in `nix/default.nix` reproduce the
  # shipped image; a measurement variant scales them at build time, which is the
  # only route that reaches the Nix daemon's own `autoGC` — measured, and the
  # reason a scaled *image* exists rather than a runtime override.
  homeVolumeSizeMiB,
  storeVolumeSizeMiB,
  defaultVolumeSizeMiB,
  storeMinFree,
  storeMaxFree,
  ...
}:

let
  # The priority this module holds for everything a user may legitimately choose.
  #
  # Not `mkDefault` (1000): an image holds that role in the merge (spec/04), and a product value at
  # the same priority is a conflict rather than a floor — measured against the shipped `base.nix`.
  # Not `mkOptionDefault` (1500) either: upstream's own option defaults sit there, so
  # `microvm.hypervisor` tied with microvm.nix's `default = "qemu"` and the guest failed to
  # evaluate at all for an image that expressed no preference — measured, by the trials.
  #
  # 1250 is the one level between the two. A layer that says nothing gets vivarium's value instead
  # of upstream's; a layer that says anything at all outranks it.
  productDefault = lib.mkOverride 1250;

  inherit (storeLayout)
    lowerStoreDir
    lowerStoreViewDir
    upperRoot
    upperStateDir
    nixDaemonEnvironment
    ;

  # Read back out of the account declared below rather than respelled as literals,
  # so the first-boot unit and the user it prepares a home for cannot disagree about
  # the path, the name, or the group. `launch-arguments.nix` reads the same
  # attribute for the session it renders.
  sessionUser = config.users.users.vivarium;

  # What the merged configuration asked for beyond the two volumes every project
  # has. `or [ ]` rather than a plain read, because the tool's option surface
  # lives in `nix/vivarium-options.nix`, which only a generated flake composes —
  # the shipped diagnostic image has no `vivarium.volumes` at all, and there
  # "declared nothing" and "cannot declare" are the same answer. Re-declaring the
  # option here is not the fix: nixpkgs throws on two declarations that both
  # carry a default, and one copy of the surface is what ADR-0058 buys.
  requestedVolumes = config.vivarium.volumes or [ ];

  # Layers concatenate (spec/04), so one volume declared by an image and again by
  # the manifest arrives twice. Two entries would be two disks over one image,
  # which upstream's own assertions refuse — and would have to, since they would
  # also share a label and the guest resolves by label.
  #
  # A ceiling may be raised between boots and never lowered in place (spec/06),
  # so the surviving size is the largest anyone asked for rather than the last.
  volumeNames = lib.unique (map (volume: volume.name) requestedVolumes);
  declarationsOf = name: lib.filter (volume: volume.name == name) requestedVolumes;
  declaredVolumes = map (
    name:
    let
      declarations = declarationsOf name;
      sizes = lib.filter (size: size != null) (map (volume: volume.size_gib) declarations);
    in
    {
      inherit name;
      # The first declaration decides, and the assertion below is what makes that safe: two
      # layers may name one volume only while they agree about where it mounts.
      inherit (lib.head declarations) mount;
      mounts = lib.unique (map (volume: volume.mount) declarations);
      sizeMiB = if sizes == [ ] then defaultVolumeSizeMiB else 1024 * lib.foldl' lib.max 0 sizes;
      # `viv-` rather than `vivarium-`, and the four characters are the point:
      # ext4 caps a label at 16 bytes and `mkfs.ext4 -L` truncates past it with a
      # warning rather than an error, which would leave the guest waiting on a
      # `/dev/disk/by-label` node that never appears. The two reserved labels keep
      # their existing spelling, because changing one orphans every image already
      # on disk. The remaining budget is asserted below rather than assumed.
      label = "viv-${name}";
    }
  ) volumeNames;

  # What `vivarium-volume-prepare` owns on a volume's first boot, built from the
  # DECLARATIONS rather than by filtering `config.microvm.volumes`. The store
  # volume is therefore absent because it was never declared, which is spec/06's
  # exemption held structurally rather than by a filter someone can weaken — and
  # it mounts in the initrd anyway, before the guest has an identity to own it.
  #
  # Mode is a column because the two cases genuinely differ: `0700` is a home,
  # and a declared path such as `/var/cache/project` is part of the guest's
  # filesystem layout that other components may traverse. It is one cell to
  # revisit if `[[volumes]]` ever grows an ownership field; spec/06 has none, so
  # neither does this.
  preparedVolumes = [
    {
      mount = sessionUser.home;
      mode = "0700";
      seed = "1";
    }
  ]
  ++ map (volume: {
    inherit (volume) mount;
    mode = "0755";
    seed = "0";
  }) declaredVolumes;

  # Whitespace-free by assertion, which is what lets a row be four fields on a
  # line rather than a format the unit would have to parse.
  volumePrepareTable = pkgs.writeText "vivarium-volume-prepare-table" (
    lib.concatMapStrings (
      row: "${row.mount} ${sessionUser.name}:${sessionUser.group} ${row.mode} ${row.seed}\n"
    ) preparedVolumes
  );

  # What the merged configuration asked to mount beyond the project tree
  # (spec/06, ADR-0020). `or [ ]` for the same reason as `vivarium.volumes`
  # above: the option surface lives in `nix/vivarium-options.nix`, which only a
  # generated flake composes.
  requestedMounts = config.vivarium.mounts or [ ];
  # Guest-side target expansion (ADR-0020): `~` is the guest home, and each side
  # expands its own home, which is what lets an identity mount such as
  # `~/.config/foo` name one spelling even though the guest username differs
  # from the host's.
  expandTarget =
    target:
    if target == "~" then
      sessionUser.home
    else if lib.hasPrefix "~/" target then
      "${sessionUser.home}/${lib.removePrefix "~/" target}"
    else
      target;

  # Where every declared share lands before `vivarium-mounts.service` binds it at
  # its target. A build constant, because whether a `source` is a directory or a
  # regular file — and so what actually binds at the target — is a host fact that
  # resolves only at launch (a file is served through its parent directory), so
  # the guest's own fstab cannot mount the share at the target directly.
  mountInternalRoot = "/run/vivarium-mounts";

  declaredMounts = lib.imap0 (index: mount: {
    # `mnt<i>` by merged-list order: deterministic (layers concatenate in a
    # fixed order), `[a-z0-9]` so the launcher's socket-token case roundtrip is
    # total, and short — a share's socket path has 108 bytes to live in. The
    # tag is a per-boot artifact; nothing persists it, so an index shifting
    # when the list is edited costs nothing.
    tag = "mnt${toString index}";
    internal = "${mountInternalRoot}/mnt${toString index}";
    target = expandTarget mount.target;
    inherit (mount) source readonly;
  }) requestedMounts;

  # Whitespace-free by assertion, like the volume table: three fields a line.
  mountTable = pkgs.writeText "vivarium-mount-table" (
    lib.concatMapStrings (
      mount: "${mount.tag} ${if mount.readonly then "1" else "0"} ${mount.target}\n"
    ) declaredMounts
  );

  # Paths the guest owns, which no declared mount may target: binding a share
  # over one either corrupts a mount the guest is made of or shadows state the
  # guest must keep. The session home is deliberately absent as a prefix —
  # identity mounts under `~` are this surface's primary use — and guarded as
  # an exact match and an ancestor below. Kept in agreement with the host's own
  # `GUEST_OWNED_PATHS` (`src/launch/workspace.rs`), which refuses the same paths
  # at resolution and owns the reasoning for the individually named `/run` and
  # `/var` entries; a unit test pins the two lists against each other.
  guestOwnedTargetRoots = [
    "/nix"
    "/proc"
    "/sys"
    "/dev"
    "/etc"
    "/boot"
    "/usr"
    "/bin"
    "/sbin"
    "/lib"
    "/lib64"
    "/root"
    "/tmp"
    "/var/lib"
    "/var/log"
    "/var/tmp"
    "/var/empty"
    "/run/vivarium"
    mountInternalRoot
    "/run/user"
    "/run/current-system"
    "/run/booted-system"
    "/run/wrappers"
    "/run/systemd"
    "/run/udev"
    "/run/dbus"
    "/run/lock"
    "/run/log"
    "/run/keys"
    "/run/credentials"
    "/run/binfmt"
    "/run/nscd"
    "/run/opengl-driver"
    upperRoot
  ];

  # `sandbox.egress` is build-channel policy the guest system is built against.
  # `or`-defaulted like `vivarium.volumes` above and for the same reason: the
  # option surface lives in `nix/vivarium-options.nix`, which only a generated
  # flake composes, and the default is spec/05's own.
  egressMode = config.sandbox.egress.mode or "open";
in
{
  options.vivarium.credentials.agents = lib.mkOption {
    type = lib.types.listOf (
      lib.types.enum [
        "ssh"
        "gpg"
      ]
    );
    default = [ ];
    apply = lib.unique;
    description = "Closed set of credential relays built into this guest image.";
  };

  # The bind table this module derives, published so `launch-arguments.nix` reads
  # the guest's own answer rather than re-deriving `mnt<index>` from the same
  # option list. Two independent derivations of one index is how a share's
  # published target comes to name a different share's mount — silently, which is
  # the failure a contract exists to catch. Internal because it is not a knob: a
  # layer setting it would be describing a guest it does not build.
  options.vivarium.internal.bindTable = lib.mkOption {
    internal = true;
    type = lib.types.listOf (lib.types.attrsOf lib.types.str);
    default = [ ];
    description = "Where the guest binds each declared share, by tag.";
  };

  config = {
    vivarium.internal.bindTable = map (mount: { inherit (mount) tag target; }) declaredMounts;

    # Everything below that a user may legitimately choose sits at `productDefault`.
    # Everything at normal priority is the launch contract — the shares, the volumes,
    # the store overlay, the guest identity — and a layer that contradicts one of
    # those is meant to fail loudly rather than quietly win.
    networking = {
      hostName = productDefault "vivarium-first";
      # The guest half of the link `nix/default.nix` declares once (spec/05), at
      # normal priority: the addressing is a contract with the supervisor's tap
      # setup, and a layer that contradicts it should fail loudly rather than
      # quietly win. The interface itself is configured under `systemd.network`
      # below, matched by the launcher's fixed MAC.
      useDHCP = false;
      # Off because the nameserver is a static `/etc/resolv.conf` below, not
      # `networking.nameservers`: the resolvconf script only collects runtime
      # data from hooks nothing in this guest feeds, so that option's value
      # silently reached no file at all — inert, not absent.
      resolvconf.enable = false;
    };
    # And systemd-resolved (which networkd enables by default) stays off too: a
    # guest-local caching stub between the guest and its nameserver is exactly
    # what spec/05 rules out under allowlist mode — the gating resolver must be
    # the only DNS the guest is given, and a cache would re-serve answers on its
    # own clock. The nameserver is the one mode-dependent value: under allowlist
    # the gating resolver on the gateway; under open the uplink's DNS forward,
    # which reaches the host's own resolver.
    services.resolved.enable = false;
    environment.etc."resolv.conf".text = "nameserver ${
      if egressMode == "allowlist" then networkLayout.gatewayAddress else networkLayout.dnsForwardAddress
    }\n";
    boot.initrd.systemd.enable = true;
    # Naming these as required initrd modules makes evaluation/build fail if the
    # selected guest kernel ceases to provide AF_VSOCK or its virtio transport.
    boot.initrd.availableKernelModules = [
      "vsock"
      "vmw_vsock_virtio_transport_common"
      "vmw_vsock_virtio_transport"
    ];

    microvm = {
      # The backend is a user choice in shape only: `launch-arguments.nix`
      # renders Cloud Hypervisor's argv and its `VmConfig`, so another value
      # would produce a launch specification for a backend nobody starts. The
      # assertion below turns that into an evaluation failure with a sentence,
      # which is why this may sit at the lowest priority rather than being forced.
      hypervisor = productDefault "cloud-hypervisor";
      # Inert for launch and kept anyway: the launcher takes memory and vcpu
      # count as launch-channel tokens (N19), so these two decide nothing the
      # host boots. They stay because a guest module that declared no size at all
      # would leave upstream's own defaults to answer for vivarium.
      mem = productDefault 2048;
      vcpu = productDefault 2;
      balloon = productDefault true;
      # Not a user choice: the launcher declares `--balloon size=0M` and the two
      # halves must agree about the starting size.
      initialBalloonMem = 0;
      writableStoreOverlay = upperRoot;

      shares = [
        {
          tag = "store";
          # N5: this machine-independent constant is how mounts.nix identifies the host store.
          source = "/nix/store";
          mountPoint = "/nix/.ro-store";
          proto = "virtiofs";
          readOnly = true;
          cache = "always";
        }
      ]
      # One share per declared mount, appended after the store — the same
      # "reserved first, declared appended" contract the volumes keep. A tree the
      # manifest declared as `[[workspaces]]` is one of these, arriving with its
      # `target` equal to its `source` (ADR-0110).
      #
      # `source` carries whatever `viv` wrote: the declared string verbatim for an
      # ordinary mount, `${VAR}` and all, expanded against the host at launch
      # (ADR-0020) where a source that is unset, missing, or not a regular file or
      # directory is refused before any boot; and an already-expanded absolute path
      # for a workspace, whose guest location is its host location and is therefore
      # guest system configuration rather than launch data. The launcher recognises
      # a declared share by its not being the store constant.
      ++ map (mount: {
        inherit (mount) tag source;
        mountPoint = mount.internal;
        proto = "virtiofs";
        readOnly = mount.readonly;
        cache = "auto";
        posixAcl = false;
        # The same complete bidirectional identity translation the store share
        # carries, expanded by the launcher from the launch-time host ids.
        extraArgs = [
          "@UID_TRANSLATION@"
          "@GID_TRANSLATION@"
        ];
      }) declaredMounts;

      # Order is a correctness contract, not presentation: cloud-hypervisor assigns
      # /dev/vda, /dev/vdb in `--disk` order and microvm.nix's `withDriveLetters`
      # assigns guest drive letters by this list's order. The launcher reads the
      # list rather than a flat field per volume for exactly that reason. Home
      # stays first so it keeps the letter it already had, and everything a layer
      # declared is appended after the two reserved ones so adding a volume never
      # moves a letter an existing image is already mounted by.
      volumes = [
        {
          image = "${volumeDirSentinel}/${homeVolumeName}.img";
          mountPoint = "/home/vivarium";
          size = homeVolumeSizeMiB;
          autoCreate = false;
          label = volumeLabel;
          fsType = "ext4";
          imageType = "raw";
        }
        {
          # ADR-0087: the writable layer and the store database persist here.
          # mounts.nix grants `neededForBoot` to whichever volume mounts at
          # `writableStoreOverlay`, which is what settles the "can a volume be
          # attached early enough" premise at evaluation time rather than by boot.
          image = "${volumeDirSentinel}/${storeVolumeName}.img";
          mountPoint = upperRoot;
          size = storeVolumeSizeMiB;
          autoCreate = false;
          label = storeVolumeLabel;
          fsType = "ext4";
          imageType = "raw";
        }
      ]
      ++ map (volume: {
        image = "${volumeDirSentinel}/${volume.name}.img";
        mountPoint = volume.mount;
        size = volume.sizeMiB;
        # Like the two above: the host creates and formats the image, because it
        # is the side that knows where the bytes go and can refuse before writing
        # any. `autoCreate` would have the guest do it, over a device the guest
        # cannot size.
        autoCreate = false;
        inherit (volume) label;
        fsType = "ext4";
        imageType = "raw";
      }) declaredVolumes;
    };

    # spec/06:22 — a read-only share must be read-only on *both* sides: the host
    # daemon gets `--readonly` from `readOnly`, but upstream's mounts.nix emits
    # only `defaults` for the guest entry, so the guest flags must be set here or
    # the read-only store mounts rw,dev,suid,exec inside the guest.
    fileSystems = {
      "${lowerStoreDir}".options = [
        "ro"
        "nodev"
        "nosuid"
        "noexec"
      ];

      # The interposed overlay of ADR-0088: a private writable view of the
      # read-only lower store, existing only so `LocalStore` may create the
      # `.links` directory it makes whether or not it was opened read-only. Its
      # upper and work directories live on the store volume, so this mount depends
      # on the same volume the merged store does — `neededForBoot` for both.
      "${lowerStoreViewDir}" = {
        neededForBoot = true;
        overlay = {
          lowerdir = [ lowerStoreDir ];
          upperdir = "${upperRoot}/lower-store-view";
          workdir = "${upperRoot}/lower-store-view-work";
        };
      };

      # Measured on the first host boot of this design, and confirmed against
      # Nix's own source: `LocalStore::linksDir` is `realStoreDir/.links`, which
      # for a local-overlay store is the *merged* `/nix/store` — so it resolves
      # through the lower layer to the **host's** link farm. Every collection then
      # `lstat`s each of its millions of entries over virtiofs, which exhausted the
      # daemon's per-guest file-descriptor budget before the first collection
      # finished, and `unlink`s any entry whose link count is one, writing into the
      # guest's upper layer. Masking the path with an empty tmpfs makes the walk
      # constant-time and keeps the guest's link farm its own. Nothing is lost:
      # `.links` is used only by store optimisation, which upstream microvm.nix
      # already asserts is off whenever a writable overlay is configured, and the
      # collector skips the directory by name when it enumerates the store.
      "/nix/store/.links" = {
        device = "tmpfs";
        fsType = "tmpfs";
        options = [
          "size=1M"
          "mode=0755"
        ];
      };
    }
    # The same spec/06:22 rule the store entry above enacts by hand, held for
    # every read-only declared mount: `readOnly` gives the daemon `--readonly`,
    # and these flags are the guest half, on the internal point the bind unit
    # then carries to the target.
    // lib.listToAttrs (
      map (mount: {
        name = mount.internal;
        value.options = [
          "ro"
          "nodev"
          "nosuid"
          "noexec"
        ];
      }) (lib.filter (mount: mount.readonly) declaredMounts)
    );

    # ADR-0094: the guest's memory posture, and it is four decisions of which three
    # are "write nothing".
    #
    # zram at the distribution module's own defaults — one zstd device at half of
    # observed RAM, swap priority above any disk device. Sizing from *observed* RAM
    # rather than from `resources.mem_mib` is what keeps N19 intact: the
    # declaration is launch-channel and must never become a build input. And half
    # of RAM is a ceiling on compressed capacity, not a reservation — an unused
    # zram device costs ~0.1% of its size in metadata — so this agrees with N22
    # rather than straining it.
    #
    # Deliberately NOT set, each for a stated reason:
    #   * `vm.swappiness` / `vm.page-cluster` — no upstream source states a posture
    #     for a VM guest. Neither the kernel's zram documentation, nor this
    #     module, nor the swap generator's own manual says anything, so vivarium
    #     inherits the kernel defaults rather than dressing a guess as a citation.
    #     Policy by omission, and unverified: that is the honest label.
    #   * `page_reporting_order` — a no-op on the primary target. It resolves to
    #     `pageblock_order`, already 9 (2 MiB) on x86_64/4K; the balloon driver
    #     overrides it only under `CONFIG_ARM64 && CONFIG_ARM64_64K_PAGES`.
    #   * `vm.compaction_proactiveness` — already 20 by default, which is what
    #     makes `spec/17`'s "the guest handles fragmentation by compacting" true.
    #     Left alone deliberately; the kernel documents non-trivial system-wide
    #     cost and latency spikes for raising it.
    #
    # No disk-backed guest swap: it would consume the persistent sparse volume and
    # turn guest memory pressure into host block I/O, working directly against the
    # prompt memory return N22 promises. NixOS adds none by default, so this is a
    # property of the image rather than a line of configuration.
    zramSwap.enable = true;

    # Both flags, per ADR-0088: the lower store is opened with `read-only=true` in
    # its URI and *that parameter* sits behind the second one, so enabling only
    # `local-overlay-store` fails at daemon start rather than at evaluation.
    # `nix-command` and `flakes` are deliberately absent: the diagnostic passes
    # those per invocation, and globalising them is a base-image decision this
    # spike must not pre-empt.
    nix.settings = {
      experimental-features = [
        "local-overlay-store"
        "read-only-local-store"
      ];

      # ADR-0089: the guest store is bounded by Nix's own space-triggered
      # collection rather than by anything vivarium writes. Below `min-free` a
      # build collects until `max-free` is free again, so the volume degrades into
      # a cache under pressure instead of filling and failing the build.
      #
      # The *mechanism* is measured — the trigger fires strictly below `min-free`
      # and frees exactly `max-free` minus available — but the shipped **values**
      # remain argued, and reaching them needs a workload rather than a boot.
      #
      # They are parameters rather than literals because `LocalStore::autoGC` reads
      # `settings.minFree` **inside the daemon**: neither a client-side
      # `--option min-free` nor `NIX_USER_CONF_FILES` on the daemon unit reaches
      # it, both measured. So the only route to a scaled threshold is an image
      # built with one, which is what `nix/default.nix`'s variant seam exists for.
      min-free = storeMinFree;
      max-free = storeMaxFree;
    };

    # The overlay-generated nix-store.mount uses What=overlay. With a writable
    # lower store, upstream's What=store drop-in would silently replace it.
    # Initrd systemd is explicit above because this shutdown ordering relies on it.
    users.users.vivarium = {
      isNormalUser = true;
      uid = 1000;
      group = "vivarium";
      home = "/home/vivarium";
    };
    users.groups.vivarium.gid = 1000;

    systemd = {
      # The guest NIC's addressing, matched by the MAC the launcher gives the
      # device rather than by an interface name the kernel may spell differently.
      network = {
        enable = true;
        networks."10-vivarium-egress" = {
          matchConfig.MACAddress = networkLayout.guestMac;
          address = [ "${networkLayout.guestAddress}/${toString networkLayout.prefixLength}" ];
          gateway = [ networkLayout.gatewayAddress ];
        };
      };

      # The local-overlay backend keeps its writable metadata outside /nix/var,
      # which NixOS provisions, and gives the read-only lower store a writable view
      # NixOS knows nothing about. Both must exist and be owned by the daemon.
      tmpfiles.rules = [
        "d ${upperStateDir} 0755 ${config.nix.daemonUser} ${config.nix.daemonGroup} - -"
        "d ${lowerStoreViewDir} 0755 ${config.nix.daemonUser} ${config.nix.daemonGroup} - -"
        "d ${lowerStoreViewDir}/.links 0755 ${config.nix.daemonUser} ${config.nix.daemonGroup} - -"
      ];

      mounts = [
        {
          what = "overlay";
          where = "/nix/store";
          overrideStrategy = "asDropin";
          unitConfig.DefaultDependencies = false;
        }
        {
          # Keep the read-only lower store mounted until the overlay is released too.
          what = "store";
          where = "/nix/.ro-store";
          overrideStrategy = "asDropin";
          unitConfig.DefaultDependencies = false;
        }
      ];

      services = {
        # ADR-0020's guest half, and since ADR-0110 the only one: every declared
        # share mounts at a build-time internal point and this unit binds it at its
        # target, taking the dir-or-file plan from the kernel command line the
        # launcher wrote. A tree the manifest owns is a share whose target is its
        # own source, which is the whole of what makes it a workspace here.
        vivarium-mounts = lib.mkIf (declaredMounts != [ ]) {
          description = "Bind declared mounts at their declared targets";
          wantedBy = [ "multi-user.target" ];
          # Internal points AND targets. `RequiresMountsFor` rather than a bare
          # `after`, and the `Requires` half is what matters: a virtiofs mount that
          # FAILED leaves an empty directory behind, a bind onto it succeeds, and
          # every write a session makes then lands on the guest's own root
          # filesystem while looking exactly like the tree — lost at shutdown,
          # silently. Ordering alone does not exclude that; a requirement does. The
          # target half orders a home-relative target after the home volume's
          # mount.
          unitConfig.RequiresMountsFor =
            map (mount: mount.internal) declaredMounts ++ map (mount: mount.target) declaredMounts;
          # After the first-boot ownership pass, or the ancestors this unit
          # creates under `~` would land in a home the session user does not
          # own yet — and `Requires`, because binding into an unprepared home
          # is the same silent-loss shape as binding from a failed share.
          after = [ "vivarium-volume-prepare.service" ];
          requires = [ "vivarium-volume-prepare.service" ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            UMask = "0022";
            # No `PrivateMounts`, `ProtectSystem`, `ProtectHome`, `PrivateTmp`,
            # `ReadOnlyPaths` or any relative. Each of them puts this unit in its
            # own mount namespace, where the binds it makes are invisible to every
            # other process: the unit would succeed, log nothing, and change
            # nothing. `contract.sh` asserts their absence, because a later
            # blanket-hardening pass is exactly how this would come back.
          };
          path = [
            pkgs.coreutils
            pkgs.util-linux
            pkgs.gnugrep
          ];
          environment = {
            VIVARIUM_MOUNT_INTERNAL_ROOT = mountInternalRoot;
            VIVARIUM_MOUNT_TABLE = mountTable;
            VIVARIUM_GUEST_HOME = sessionUser.home;
            VIVARIUM_GUEST_OWNER = "${sessionUser.name}:${sessionUser.group}";
          };
          # Same `enableStrictShellChecks` posture as its siblings: the body is
          # covered by the repository's shellcheck hook.
          script = builtins.readFile ./mount-bind.sh;
        };

        vivarium-agent = {
          description = "Vivarium guest control and credential agent";
          wantedBy = [ "multi-user.target" ];
          # spec/12 starts every session in a declared workspace, so a bind that is
          # not yet in place is not a slow session but a missing cwd. `requires`
          # rather than `wants`: the failure is then one loud boot failure on the
          # console the host already captures, rather than one refusal per session
          # naming three possible causes.
          # spec/06 requires the first-boot home ownership applied before the agent
          # accepts a session: a session that wins the race gets a home its own user
          # cannot write. Both units were `wantedBy = multi-user.target` with no
          # ordering between them, so which one won was undefined — measured, by
          # reading `systemctl show -p After vivarium-agent.service` in a live guest,
          # where neither name appeared.
          after = [
            "vivarium-volume-prepare.service"
          ]
          # A session must see every declared mount, the workspaces among them
          # (ADR-0110): one loud boot failure beats one refusal per session
          # naming three possible causes.
          ++ lib.optional (declaredMounts != [ ]) "vivarium-mounts.service";
          requires = [
            "vivarium-volume-prepare.service"
          ]
          ++ lib.optional (declaredMounts != [ ]) "vivarium-mounts.service";
          serviceConfig = {
            Type = "simple";
            User = "vivarium";
            Group = "vivarium";
            RuntimeDirectory = "vivarium";
            RuntimeDirectoryMode = "0700";
            UMask = "0077";
            CapabilityBoundingSet = "";
            AmbientCapabilities = "";
            NoNewPrivileges = true;
            Restart = "on-failure";
            ExecStart = "${lib.getExe' guestAgentPackage "vivarium-guest-agent"}${
              lib.concatMapStrings (id: " --credential ${id}") config.vivarium.credentials.agents
            }";
          };
        };

        # ADR-0088's fifth prior-art detail: `nix-daemon` is the only process that
        # opens the overlay store, and every client reaches it through the socket.
        # microvm.nix's system.nix already enables both units; this only points the
        # daemon at the store and makes it wait for the mounts it is made of.
        nix-daemon = {
          serviceConfig.EnvironmentFile = nixDaemonEnvironment;
          unitConfig.RequiresMountsFor = [
            "/nix/store"
            "/nix/store/.links"
            lowerStoreViewDir
            upperStateDir
          ];
        };

        vivarium-volume-prepare = {
          description = "Give the guest user its volumes on their first boot";
          wantedBy = [ "multi-user.target" ];
          # `RequiresMountsFor` carries both halves — `Requires=` and `After=` on
          # each matching `.mount` unit — so no escaped unit name appears here or
          # anywhere else. `After=` alone would order without requiring, which
          # re-opens the failure this unit exists to avoid: a mount that did not
          # happen leaves an empty directory, and owning that produces a mount
          # point which looks correct and loses every write at shutdown.
          unitConfig.RequiresMountsFor = map (row: row.mount) preparedVolumes;
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          environment.VIVARIUM_VOLUME_TABLE = volumePrepareTable;
          path = [ pkgs.coreutils ];
          # The one extracted unit body without `enableStrictShellChecks`, and the
          # exemption is deliberate rather than an oversight: this unit is in the
          # SHIPPED image, and turning it on swaps `writeShellScriptBin` for
          # `writeShellApplication`, which moves the guest's derivation path. That
          # path's byte-identity across the product/verification split is the proof
          # that the split changed nothing a user receives (ADR-0098), so it is not
          # spent on a lint. The body is covered by the `shellcheck` pre-commit hook
          # like every other `.sh` here; what it lacks is the second, build-time pass.
          # Flip this in a change that is allowed to move the shipped closure.
          script = builtins.readFile ./volume-prepare.sh;
        };
      };
    };

    assertions = [
      {
        # The counterpart to the lowered `hypervisor` priority above. A layer may
        # set it; setting it to anything else must fail here rather than at the
        # moment a Cloud Hypervisor argv is handed to a different program.
        assertion = config.microvm.hypervisor == "cloud-hypervisor";
        message =
          "vivarium launches Cloud Hypervisor; microvm.hypervisor is "
          + "`${config.microvm.hypervisor}`, which no vivarium launch specification describes";
      }
      {
        assertion = !config.microvm.storeOnDisk;
        message = "the literal /nix/store share must disable storeOnDisk";
      }
      {
        assertion = config.nix.enable;
        message = "the local-overlay guest store requires Nix in the guest";
      }
      {
        # ADR-0088: Lix does not implement this store type at all, so swapping the
        # package is a boot-path regression that would otherwise appear only as a
        # daemon that refuses to start.
        assertion = (config.nix.package.pname or null) == "nix";
        message = "the local-overlay guest store requires CppNix, not Lix";
      }
      {
        # The lower store's database is the boot-closure registration and nothing
        # else. Without it the lower store is empty, every host path is copied up,
        # and ADR-0087's sharing claim quietly becomes false.
        assertion = config.microvm.registerClosure;
        message = "the local-overlay lower store needs microvm.registerClosure to describe the boot closure";
      }
      {
        assertion = config.microvm.socket != null && !(lib.hasPrefix "/" config.microvm.socket);
        message = "the upstream Cloud Hypervisor socket default must remain relative";
      }
      {
        # Read off the MERGED option rather than off the list this module builds,
        # which is what makes it catch the case worth catching: a composed layer
        # contributing its own `microvm.volumes` definition appends to the same
        # list, and a guest whose home and store swapped drive letters mounts the
        # wrong data at the right path without erroring anywhere.
        assertion =
          map (volume: volume.label) (lib.take 2 config.microvm.volumes) == [
            volumeLabel
            storeVolumeLabel
          ];
        message =
          "the home and store volumes must stay first and second in microvm.volumes; "
          + "found ${lib.concatMapStringsSep ", " (volume: volume.label) config.microvm.volumes}";
      }
    ]
    ++ lib.concatMap (volume: [
      {
        # The name becomes a filename under the project's state and a label the
        # guest resolves by, so it is not free text. `default` and `store` are
        # taken by the two volumes that exist without being declared (spec/06).
        assertion =
          volume.name != ""
          && volume.name != homeVolumeName
          && volume.name != storeVolumeName
          && builtins.match "[a-z0-9][a-z0-9-]*" volume.name != null;
        message =
          "volume name `${volume.name}` must be lowercase letters, digits, and dashes, "
          + "and may not be `${homeVolumeName}` or `${storeVolumeName}`, which are reserved";
      }
      {
        # ext4's own limit. `mkfs.ext4 -L` truncates past it and exits 0, so
        # without this the failure is a guest that waits for a by-label device
        # that will never appear, on a boot with nothing else wrong.
        assertion = builtins.stringLength volume.label <= 16;
        message =
          "volume label `${volume.label}` is ${toString (builtins.stringLength volume.label)} bytes; "
          + "ext4 allows 16, which leaves ${toString (16 - 4)} characters for a volume name";
      }
      {
        # Two layers may declare one name (lists concatenate), and that is legal
        # only while they agree about where it mounts. The Rust reader refuses the
        # same shape with a better message; this is the backstop for a `nix build`
        # that never went through `viv`.
        assertion = builtins.length volume.mounts == 1;
        message =
          "volume `${volume.name}` is declared at more than one mountpoint: "
          + lib.concatStringsSep ", " volume.mounts;
      }
      {
        assertion =
          lib.hasPrefix "/" volume.mount
          && volume.mount != "/"
          && builtins.match ".*[[:space:]].*" volume.mount == null
          && !(lib.hasPrefix "/nix" volume.mount)
          && volume.mount != sessionUser.home
          && volume.mount != upperRoot
          && volume.mount != mountInternalRoot
          # The whole subtree, not just its root: the shares mount one level below
          # it as `mntN`, so an equality test alone would leave every actual share
          # mount point free for a volume to shadow. The host-side
          # `GUEST_OWNED_PATHS` entry already reserves the subtree.
          && !(lib.hasPrefix "${mountInternalRoot}/" volume.mount);
        message =
          "volume `${volume.name}` mounts at `${volume.mount}`, which is not an absolute "
          + "whitespace-free path the guest leaves free (`/nix`, `${sessionUser.home}`, "
          + "`${upperRoot}`, and `${mountInternalRoot}` are the guest's own)";
      }
    ]) declaredVolumes
    ++ [
      {
        # Mount points and labels are each a key: two volumes at one mount point
        # is a shadowed disk, and two at one label is the wrong one mounted.
        assertion =
          let
            mounts = map (volume: volume.mount) declaredVolumes;
            labels = map (volume: volume.label) declaredVolumes ++ [
              volumeLabel
              storeVolumeLabel
            ];
          in
          mounts == lib.unique mounts && labels == lib.unique labels;
        message = "declared volumes must have distinct mountpoints and distinct labels";
      }
      {
        # spec/06:97 exempts the store volume from first-boot ownership, and the
        # table above holds it out by construction rather than by a filter. This
        # is that sentence made executable: it fails the build if the table is
        # ever rebuilt from the attached volumes instead of the declared ones.
        assertion = !(lib.any (row: row.mount == config.microvm.writableStoreOverlay) preparedVolumes);
        message =
          "the store volume at `${config.microvm.writableStoreOverlay}` must not be prepared on "
          + "first boot: it mounts in the initrd, before the guest has an identity to own it";
      }
    ]
    ++ lib.concatMap (mount: [
      {
        # The declaration is refused with the string it was written in, so the
        # message names the declared target; the checks run on the expansion.
        assertion = mount.source != "";
        message = "a declared mount targeting `${mount.target}` has an empty `source`";
      }
      {
        assertion =
          lib.hasPrefix "/" mount.target
          && mount.target != "/"
          && builtins.match ".*[[:space:]].*" mount.target == null
          && builtins.match "(.*/)?\\.\\.(/.*)?" mount.target == null;
        message =
          "mount target `${mount.target}` must be an absolute, whitespace-free path "
          + "without `..` (`~` expands to the guest home)";
      }
      {
        # Under the home is this surface's primary use; the home itself, or an
        # ancestor of it, would shadow the volume the session lives on.
        assertion =
          mount.target != sessionUser.home && !(lib.hasPrefix "${mount.target}/" sessionUser.home);
        message = "mount target `${mount.target}` would shadow the guest home `${sessionUser.home}`";
      }
      {
        assertion =
          !(lib.any (
            owned:
            mount.target == owned
            || lib.hasPrefix "${owned}/" mount.target
            || lib.hasPrefix "${mount.target}/" owned
          ) guestOwnedTargetRoots);
        message =
          "mount target `${mount.target}` collides with a path the guest owns "
          + "(or is an ancestor of one)";
      }
    ]) declaredMounts
    ++ [
      {
        # ADR-0020: declarations concatenate across layers and duplicate
        # targets fail evaluation. Volume mountpoints are in the same namespace
        # — one guest path cannot be both a disk and a share.
        assertion =
          let
            targets = map (mount: mount.target) declaredMounts;
            volumeMounts = map (volume: volume.mountPoint) config.microvm.volumes;
          in
          targets == lib.unique targets && lib.all (target: !(lib.elem target volumeMounts)) targets;
        message = "declared mounts must have distinct targets, distinct from every volume mountpoint";
      }
      {
        # The launcher derives each share's socket token by upper-casing its
        # tag and the runner recovers the socket name by lower-casing it back,
        # so the roundtrip is total only over this charset. Checked on the
        # MERGED list: a composed layer contributing its own share is exactly
        # the entry worth catching.
        assertion = lib.all (share: builtins.match "[a-z0-9]+" share.tag != null) config.microvm.shares;
        message =
          "every virtiofs share tag must match [a-z0-9]+; found "
          + lib.concatMapStringsSep ", " (share: "`${share.tag}`") config.microvm.shares;
      }
    ]
    ++ map (share: {
      assertion = share.socket != null && !(lib.hasPrefix "/" share.socket);
      message = "virtiofs socket defaults must remain non-null and relative";
    }) config.microvm.shares;

    # The user's to choose, and the one value where losing that choice silently
    # would be worst: it decides how existing state is interpreted, so a product
    # module overriding a declared value would reinterpret a volume the user
    # already has. Lowest priority, like the other user-facing values above.
    system.stateVersion = productDefault "25.11";
  };
}
