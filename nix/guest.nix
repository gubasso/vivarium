{
  config,
  lib,
  pkgs,
  storeLayout,
  volumeLabel,
  storeVolumeLabel,
  workspaceSourceSentinel,
  volumeImageSentinel,
  storeVolumeImageSentinel,
  guestAgentPackage,
  # Variant scalars (ADR-0095). The defaults in `nix/default.nix` reproduce the
  # shipped image; a measurement variant scales them at build time, which is the
  # only route that reaches the Nix daemon's own `autoGC` — measured, and the
  # reason a scaled *image* exists rather than a runtime override.
  homeVolumeSizeMiB,
  storeVolumeSizeMiB,
  storeMinFree,
  storeMaxFree,
  ...
}:

let
  inherit (storeLayout)
    lowerStoreDir
    lowerStoreViewDir
    upperRoot
    upperStateDir
    nixDaemonEnvironment
    ;
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

  config = {
    networking.hostName = "vivarium-first";
    boot.initrd.systemd.enable = true;
    # Naming these as required initrd modules makes evaluation/build fail if the
    # selected guest kernel ceases to provide AF_VSOCK or its virtio transport.
    boot.initrd.availableKernelModules = [
      "vsock"
      "vmw_vsock_virtio_transport_common"
      "vmw_vsock_virtio_transport"
    ];

    microvm = {
      hypervisor = "cloud-hypervisor";
      mem = 2048;
      vcpu = 2;
      balloon = true;
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
        {
          tag = "workspace";
          source = workspaceSourceSentinel;
          mountPoint = "/workspaces/vivarium";
          proto = "virtiofs";
          cache = "auto";
          posixAcl = false;
          # spec/06 requires a *complete* bidirectional 1:1 translation: guest ids
          # outside the map are forbidden, host ids outside it appear as overflow.
          # virtiofsd's default for an unmapped id is identity, so a lone
          # `map:` range is not the contract — the forbid/squash ranges have to be
          # spelled out. They depend on the launch-time host uid/gid, so the
          # launcher expands each token into the full argument run (N19).
          extraArgs = [
            "@UID_TRANSLATION@"
            "@GID_TRANSLATION@"
          ];
        }
      ];

      # Order is a correctness contract, not presentation: cloud-hypervisor assigns
      # /dev/vda, /dev/vdb in `--disk` order and microvm.nix's `withDriveLetters`
      # assigns guest drive letters by this list's order. The launcher reads the
      # list rather than a flat field per volume for exactly that reason. Home
      # stays first so it keeps the letter it already had.
      volumes = [
        {
          image = volumeImageSentinel;
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
          image = storeVolumeImageSentinel;
          mountPoint = upperRoot;
          size = storeVolumeSizeMiB;
          autoCreate = false;
          label = storeVolumeLabel;
          fsType = "ext4";
          imageType = "raw";
        }
      ];
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
    };

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
        vivarium-agent = {
          description = "Vivarium guest control and credential agent";
          wantedBy = [ "multi-user.target" ];
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
          description = "Prepare the first diagnostic volume";
          wantedBy = [ "multi-user.target" ];
          after = [ "home-vivarium.mount" ];
          requires = [ "home-vivarium.mount" ];
          serviceConfig.Type = "oneshot";
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
    ]
    ++ map (share: {
      assertion = share.socket != null && !(lib.hasPrefix "/" share.socket);
      message = "virtiofs socket defaults must remain non-null and relative";
    }) config.microvm.shares;

    system.stateVersion = "25.11";
  };
}
