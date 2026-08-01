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
        # Deliberately *not* `journal+console`. journald forwards to the console
        # with one best-effort `writev(2)` per line and no short-write retry
        # (`src/journal/journald-console.c`), so a congested tty truncates a line
        # mid-byte and reports nothing — measured here as ~0.1% silent loss with a
        # host reader attached for the whole run. The script re-points its own
        # stdout at /dev/console instead, which is a blocking fd written by
        # coreutils' own retry loops. The journal keeps a copy for post-mortem.
        StandardOutput = "journal";
        StandardError = "journal";
      };
      # `systemd.services.<name>.path` *replaces* the unit's PATH rather than
      # extending the system one, so every command the script calls must be
      # named here. Only coreutils, findutils, gnugrep, gnused and systemd are
      # added by NixOS itself — gawk is not, and its absence killed the whole
      # diagnostic at the first `awk` under the `set -e` NixOS wraps a `script`
      # in. It is already in the system closure, so naming it costs nothing.
      path = [
        pkgs.coreutils
        pkgs.findutils
        pkgs.gawk
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
        # Write straight to the console device rather than through journald. This
        # is the transport under test, so every byte below is a direct blocking
        # write to the same tty Cloud Hypervisor exports as `--serial socket=`.
        # CH 52.0 attaches the accepted socket to the UART without
        # `set_nonblocking`, so a slow host reader stalls the writer instead of
        # losing bytes — which makes this path lossless once journald is out of it.
        exec >/dev/console 2>&1
        # PID 1 writes its own status lines to this same console, and a status
        # message landing mid-write splits a marker across two lines — all bytes
        # present, but the token broken. That is interleaving, not loss, and it is
        # the difference between a transport defect and cosmetic noise. Silence the
        # competing writer for the duration rather than teaching the host to guess:
        # systemd(1) documents SIGRTMIN+21 as "disable status messages on console"
        # and SIGRTMIN+20 as its inverse.
        kill -s RTMIN+21 1 2>/dev/null || true
        restore_status() { kill -s RTMIN+20 1 2>/dev/null || true; }
        trap 'restore_status; systemctl poweroff' EXIT
        echo 'VIVARIUM_DIAGNOSTIC_FIRST_MARKER'
        # A deterministic early burst measures lossless delivery, now that the
        # replay question is answered (Cloud Hypervisor does not replay to a late
        # client). The host counts marker occurrences and never bytes. State the
        # expected count *before* the burst so a lost tail cannot also lose the
        # number it should be judged against.
        burst=/run/vivarium-console-burst
        yes VIVARIUM_CONSOLE_BURST | head -c 1048576 >"$burst" || true
        echo "VIVARIUM_CONSOLE_LINES_EXPECTED=$(grep -c VIVARIUM_CONSOLE_BURST "$burst")"
        cat "$burst"
        echo
        echo 'VIVARIUM_CONSOLE_BYTES_CLAIMED=1048576'
        rm -f "$burst"
        sleep 10

        mount_line="$(awk '$2 == "/workspaces/vivarium" { print; exit }' /proc/self/mounts)"
        echo "VIVARIUM_WORKSPACE_MOUNT=$mount_line"
        if printf '%s\n' "$mount_line" | grep -Eq ' virtiofs (.*,)?rw(,| )'; then
          echo 'VIVARIUM_WORKSPACE_CONTRACT=virtiofs-rw'
        else
          echo 'VIVARIUM_WORKSPACE_CONTRACT=unexpected'
        fi

        echo "VIVARIUM_STORE_PING_BEGIN"
        # Same feature gate as the inner-develop spike below: without it these two
        # report the guest's default feature set rather than anything about the
        # shared store. `nix-store --check-validity` is stable CLI and needs none
        # of this, which is why it stayed a usable answer even when these did not.
        nix_unstable="nix --extra-experimental-features nix-command"
        $nix_unstable store ping 2>&1 || true

        # A single "is an arbitrary lowerdir path valid?" question cannot be
        # interpreted, because "no" is the *expected* answer and says nothing
        # about whether registration works at all. microvm.nix's registerClosure
        # loads only `system.build.toplevel`'s closure via `nix-store --load-db`
        # at boot (nixos-modules/microvm/store-disk.nix), so the honest probe is
        # two questions: a path inside that closure must be valid, and a path
        # outside it must not be. The pair is what measures ADR-0038's reach.
        check_validity() {
          if nix-store --check-validity "$1" >/dev/null 2>&1; then echo yes; else echo no; fi
        }
        system_path="$(readlink -f /run/current-system)"
        echo "VIVARIUM_STORE_SYSTEM_PATH=$system_path"
        echo "VIVARIUM_STORE_DB_VALID_INCLOSURE=$(check_validity "$system_path")"

        # The discriminator must ask about a path *in the store* — a
        # /nix/.ro-store path is outside it and Nix rejects it outright, which
        # would report invalid on every run regardless of the real answer.
        lower_path="$(find /nix/.ro-store -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null || true)"
        echo "VIVARIUM_STORE_LOWER_PATH=$lower_path"
        if test -n "$lower_path"; then
          known_path="/nix/store/$(basename "$lower_path")"
          echo "VIVARIUM_STORE_KNOWN_PATH=$known_path"
          $nix_unstable path-info "$known_path" 2>&1 || true
          # Whether the sampled path happens to sit inside the registered closure
          # decides how its validity reads, so state it rather than inferring it.
          if nix-store -q --requisites "$system_path" 2>/dev/null | grep -qxF "$known_path"; then
            echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=yes'
          else
            echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=no'
          fi
          echo "VIVARIUM_STORE_DB_VALID_SAMPLE=$(check_validity "$known_path")"
        else
          echo 'VIVARIUM_STORE_KNOWN_PATH='
          echo 'VIVARIUM_STORE_SAMPLE_IN_CLOSURE=indeterminate-empty-lowerdir'
          echo 'VIVARIUM_STORE_DB_VALID_SAMPLE=indeterminate-empty-lowerdir'
        fi
        echo "VIVARIUM_STORE_PING_END"

        # Free page reporting can only return pages the guest actually frees, so
        # the probe has to dirty *guest RAM*. The previous form wrote to the ext4
        # volume, whose pages are host-file page cache rather than guest anonymous
        # memory — freeing them need not produce anything reportable. /dev/shm is
        # tmpfs, i.e. guest RAM, so deleting the file frees guest pages outright.
        #
        # The host cannot sample a transition it cannot see, so each phase is
        # announced and then held long enough to be sampled. The host sampler
        # aligns its series on these markers; without the hold there is no
        # post-free window at all, which is why the earlier reading could only
        # ever corroborate.
        mem_kib() { awk -v k="$1:" '$1 == k { print $2 }' /proc/meminfo; }
        echo "VIVARIUM_MEM_PHASE=baseline MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
        sleep 10
        if dd if=/dev/zero of=/dev/shm/vivarium-memory-probe bs=1M count=512 status=none; then
          echo 'VIVARIUM_MEMORY_PROBE=ok'
        else
          echo 'VIVARIUM_MEMORY_PROBE=failed'
        fi
        echo "VIVARIUM_MEM_PHASE=allocated MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
        sleep 10
        rm -f /dev/shm/vivarium-memory-probe || true
        echo "VIVARIUM_MEM_PHASE=freed MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"
        # Free page reporting is asynchronous: the balloon walks free pages and
        # hints them to the VMM over the reporting virtqueue. Give it a window
        # before declaring the reading settled.
        sleep 20
        echo "VIVARIUM_MEM_PHASE=settled MEMFREE=$(mem_kib MemFree) MEMAVAIL=$(mem_kib MemAvailable)"

        # A non-zero exit here IS the spike's answer, not an error — but only once
        # the command can actually reach the store question. `nix develop` is
        # gated on nix-command/flakes, so without these the spike exits 1 having
        # measured nothing but the guest's default feature set. Passed per
        # invocation rather than set in `nix.settings`, because whether the base
        # image enables these globally is a base-image decision this spike must
        # not pre-empt.
        # Run as the mapped project user, not root. The workspace share maps host
        # uid 1000 to guest uid 1000, so to guest root every file in it is owned
        # by someone else and libgit2's ownership check refuses to open the
        # repository at all — the spike then measures that refusal instead of the
        # store. Running as `vivarium` makes the check pass on its merits and
        # keeps the ADR-0066 translation contract intact; `safe.directory` would
        # only suppress the symptom. HOME must be set explicitly because
        # `runuser -u` does not start a login shell.
        inner_status=0
        runuser -u vivarium -- env HOME=/home/vivarium \
          timeout 120 nix --extra-experimental-features 'nix-command flakes' \
          --offline develop /workspaces/vivarium --command true \
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
