{
  config,
  lib,
  pkgs,
  volumeLabel,
  workspaceSourceSentinel,
  volumeImageSentinel,
  ...
}:

{
  networking.hostName = "vivarium-first";
  boot.initrd.systemd.enable = true;

  microvm = {
    hypervisor = "cloud-hypervisor";
    mem = 2048;
    vcpu = 2;
    balloon = true;
    initialBalloonMem = 0;
    writableStoreOverlay = "/nix/.rw-store";

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

    volumes = [
      {
        image = volumeImageSentinel;
        mountPoint = "/home/vivarium";
        size = 32768;
        autoCreate = false;
        label = volumeLabel;
        fsType = "ext4";
        imageType = "raw";
      }
    ];
  };

  # spec/06:22 — a read-only share must be read-only on *both* sides: the host
  # daemon gets `--readonly` from `readOnly`, but upstream's mounts.nix emits
  # only `defaults` for the guest entry, so the guest flags must be set here or
  # the read-only store mounts rw,dev,suid,exec inside the guest.
  fileSystems."/nix/.ro-store".options = [
    "ro"
    "nodev"
    "nosuid"
    "noexec"
  ];

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

    services.vivarium-volume-prepare = {
      description = "Prepare the first diagnostic volume";
      wantedBy = [ "multi-user.target" ];
      after = [ "home-vivarium.mount" ];
      requires = [ "home-vivarium.mount" ];
      serviceConfig.Type = "oneshot";
      script = ''
        if ! test -e /home/vivarium/.vivarium-first-boot; then
          chown vivarium:vivarium /home/vivarium
          chmod 0700 /home/vivarium
          ${pkgs.coreutils}/bin/touch /home/vivarium/.vivarium-first-boot
          chown vivarium:vivarium /home/vivarium/.vivarium-first-boot
        fi
      '';
    };

    services.vivarium-first-microvm-diagnostic = {
      description = "Deterministic first-microVM diagnostic";
      wantedBy = [ "multi-user.target" ];
      after = [
        "vivarium-volume-prepare.service"
        "workspaces-vivarium.mount"
      ];
      requires = [
        "vivarium-volume-prepare.service"
        "workspaces-vivarium.mount"
      ];
      serviceConfig = {
        Type = "oneshot";
        StandardOutput = "journal+console";
        StandardError = "journal+console";
      };
      path = [
        pkgs.coreutils
        pkgs.findutils
        pkgs.gnugrep
        pkgs.nix
        pkgs.util-linux
      ];
      script = ''
        set -u
        # NixOS wraps a unit `script` in `set -e`. Every probe below is an
        # experiment whose negative result is the finding, so a non-zero exit
        # must never strand the VM: poweroff is unconditional from here on.
        trap 'systemctl poweroff' EXIT
        echo 'VIVARIUM_DIAGNOSTIC_FIRST_MARKER'
        # A deterministic early burst lets a late host reader record replay/buffering behaviour.
        yes VIVARIUM_CONSOLE_BURST | head -c 1048576 || true
        echo
        echo 'VIVARIUM_CONSOLE_BYTES_CLAIMED=1048576'
        sleep 10

        mount_line="$(awk '$2 == "/workspaces/vivarium" { print; exit }' /proc/self/mounts)"
        echo "VIVARIUM_WORKSPACE_MOUNT=$mount_line"
        if printf '%s\n' "$mount_line" | grep -Eq ' virtiofs (.*,)?rw(,| )'; then
          echo 'VIVARIUM_WORKSPACE_CONTRACT=virtiofs-rw'
        else
          echo 'VIVARIUM_WORKSPACE_CONTRACT=unexpected'
        fi

        echo "VIVARIUM_STORE_PING_BEGIN"
        nix store ping 2>&1 || true
        # The discriminator must ask about a path *in the store* — a
        # /nix/.ro-store path is outside it and Nix rejects it outright, which
        # would report DB_VALID=no on every run regardless of the real answer.
        lower_path="$(find /nix/.ro-store -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null || true)"
        echo "VIVARIUM_STORE_LOWER_PATH=$lower_path"
        if test -n "$lower_path"; then
          known_path="/nix/store/$(basename "$lower_path")"
          echo "VIVARIUM_STORE_KNOWN_PATH=$known_path"
          nix path-info "$known_path" 2>&1 || true
          if nix-store --check-validity "$known_path" >/dev/null 2>&1; then
            echo 'VIVARIUM_STORE_DB_VALID=yes'
          else
            echo 'VIVARIUM_STORE_DB_VALID=no'
          fi
        else
          echo 'VIVARIUM_STORE_KNOWN_PATH='
          echo 'VIVARIUM_STORE_DB_VALID=indeterminate-empty-lowerdir'
        fi
        echo "VIVARIUM_STORE_PING_END"

        before="$(awk '/MemAvailable:/ { print $2 }' /proc/meminfo)"
        if dd if=/dev/zero of=/home/vivarium/.vivarium-memory-probe bs=1M count=256 status=none && sync; then
          echo 'VIVARIUM_MEMORY_PROBE=ok'
        else
          echo 'VIVARIUM_MEMORY_PROBE=failed'
        fi
        rm -f /home/vivarium/.vivarium-memory-probe || true
        after="$(awk '/MemAvailable:/ { print $2 }' /proc/meminfo)"
        echo "VIVARIUM_FREE_PAGE_NO_COMPACTION_KIB_BEFORE=$before"
        echo "VIVARIUM_FREE_PAGE_NO_COMPACTION_KIB_AFTER=$after"

        # A non-zero exit here IS the spike's answer, not an error.
        inner_status=0
        timeout 120 nix --offline develop /workspaces/vivarium --command true \
          > /home/vivarium/inner-nix-develop.log 2>&1 || inner_status=$?
        echo "VIVARIUM_INNER_NIX_DEVELOP_STATUS=$inner_status"
        tail -n 40 /home/vivarium/inner-nix-develop.log || true
        echo 'VIVARIUM_DIAGNOSTIC_COMPLETE'
      '';
    };
  };

  assertions = [
    {
      assertion = !config.microvm.storeOnDisk;
      message = "the literal /nix/store share must disable storeOnDisk";
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
}
