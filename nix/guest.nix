{
  config,
  lib,
  pkgs,
  volumeLabel,
  storeVolumeLabel,
  workspaceSourceSentinel,
  volumeImageSentinel,
  storeVolumeImageSentinel,
  storeCanaryExpression,
  ...
}:

let
  # ADR-0087/ADR-0088. The layout mirrors the public prior art running this same
  # topology (shazow/agentspace), with one deliberate divergence recorded in
  # ADR-0088: that module forces `microvm.writableStoreOverlay` to null and
  # hand-writes `fileSystems."/nix/store"`. Here the option stays set, so
  # upstream's mounts.nix generates the identical overlay, marks the store volume
  # `neededForBoot` on its own, and leaves the `What=overlay` shutdown drop-in
  # uncontested — which forcing the option null would silently replace with
  # `What=store`.
  lowerStoreDir = "/nix/.ro-store";
  # LocalStore creates `<real>/.links` unconditionally, read-only or not, and the
  # lower layer is a read-only share. Give the metadata reader a private writable
  # view instead of making the immutable lowerdir writable.
  lowerStoreViewDir = "/nix/.local-overlay-lower-store";
  # The lower store's *state* is the tmpfs root's own `/nix/var/nix`, which
  # microvm.nix's `registerClosure` fills from `regInfo` in `boot.postBootCommands`
  # — stage 2, before systemd — so it describes exactly the boot closure and is
  # rebuilt from scratch every boot. The upper store's state is on the volume and
  # is the half that persists. That split is what makes upstream's warning about
  # `registerClosure` and a persistent overlay not apply.
  lowerStateDir = "/nix/var/nix";
  upperRoot = "/nix/.rw-store";
  upperLayer = "${upperRoot}/store";
  upperStateDir = "${upperRoot}/state";
  lowerStoreUri = "local?real=${lowerStoreViewDir}&state=${lowerStateDir}&read-only=true";
  remountHook = pkgs.writeShellScript "vivarium-local-overlay-remount" ''
    set -eu
    # util-linux's new mount API cannot remount overlayfs (#2528, #2576), and
    # upstream Nix's own test harness sets this globally for the same reason.
    # Load-bearing here, not vestigial: overlayfs runs in the *guest*, whose
    # kernel is below the 6.19 at which upstream stops reproducing the stale
    # handle — an observation upstream records without accounting for it.
    export LIBMOUNT_FORCE_MOUNT2=always
    exec ${pkgs.util-linux}/bin/mount -o remount "$1"
  '';
  storeUri =
    "local-overlay://?lower-store=${lib.escapeURL lowerStoreUri}"
    + "&upper-layer=${lib.escapeURL upperLayer}"
    + "&state=${lib.escapeURL upperStateDir}"
    + "&remount-hook=${lib.escapeURL (toString remountHook)}"
    # The initrd assembles the overlay below /sysroot; after switch-root the mount
    # is correct but /proc/self/mounts still records the initrd lowerdir prefix,
    # which the check string-matches. ADR-0088 replaces it with a vivarium-side
    # assertion — the store-spike service below reads the merged mount itself.
    + "&check-mount=false";
  # An EnvironmentFile, never `Environment=`: systemd reads percent escapes in a
  # unit-file value as unit specifiers, and this URI is percent-encoded throughout.
  nixDaemonEnvironment = pkgs.writeText "vivarium-local-overlay-environment" ''
    NIX_REMOTE=${storeUri}
  '';
in

{
  networking.hostName = "vivarium-first";
  boot.initrd.systemd.enable = true;

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
        size = 32768;
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
        size = 32768;
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

  # Both flags, per ADR-0088: the lower store is opened with `read-only=true` in
  # its URI and *that parameter* sits behind the second one, so enabling only
  # `local-overlay-store` fails at daemon start rather than at evaluation.
  # `nix-command` and `flakes` are deliberately absent: the diagnostic passes
  # those per invocation, and globalising them is a base-image decision this
  # spike must not pre-empt.
  nix.settings.experimental-features = [
    "local-overlay-store"
    "read-only-local-store"
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
        script = ''
          if ! test -e /home/vivarium/.vivarium-first-boot; then
            chown vivarium:vivarium /home/vivarium
            chmod 0700 /home/vivarium
            ${pkgs.coreutils}/bin/touch /home/vivarium/.vivarium-first-boot
            chown vivarium:vivarium /home/vivarium/.vivarium-first-boot
          fi
        '';
      };

      # ADR-0087/ADR-0088's unmeasured mechanism, measured. Ordered *before* the
      # diagnostic because the diagnostic powers the machine off; ordered *after*
      # the daemon socket because every probe here must reach the overlay store
      # through it. Never `Requires=`: a failed probe is this service's finding,
      # and must not strand the machine by suppressing the unit that powers it off.
      vivarium-store-spike = {
        description = "Persistent local-overlay guest store spike";
        wantedBy = [ "multi-user.target" ];
        after = [
          "nix-daemon.socket"
          "systemd-tmpfiles-setup.service"
        ];
        before = [ "vivarium-first-microvm-diagnostic.service" ];
        serviceConfig = {
          Type = "oneshot";
          StandardOutput = "journal";
          StandardError = "journal";
          # Bounded so a wedged probe cannot outlive the diagnostic's own ceiling.
          TimeoutStartSec = "600s";
        };
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
          # Same transport argument as the diagnostic: journald's console forwarder
          # drops bytes under congestion, so write to the tty directly.
          exec >/dev/console 2>&1
          echo 'VIVARIUM_STORE_SPIKE_BEGIN'

          # Every client below must reach the overlay store through the daemon.
          # Root can write the merged /nix/store, so with NIX_REMOTE unset `nix`
          # opens it directly as a plain LocalStore backed by the *lower* database
          # — measuring a store nothing in this design uses, and reporting
          # confidently about it. This is ADR-0088's "daemon-only" detail as an
          # operational fact rather than a configuration one.
          export NIX_REMOTE=daemon
          export HOME=/root

          # Absolute paths for every Nix tool. The unit's PATH was enough for
          # nix-store and nix-build but not for nix-collect-garbage, which cost a
          # whole boot to a bare exit 127 — a `[FAIL]` for a collection that
          # never ran, which reads exactly like a collection that failed.
          nix_store=${pkgs.nix}/bin/nix-store
          nix_build=${pkgs.nix}/bin/nix-build
          nix_collect_garbage=${pkgs.nix}/bin/nix-collect-garbage

          upper_root=${upperRoot}
          upper_layer=${upperLayer}
          lower_dir=${lowerStoreDir}
          lower_view=${lowerStoreViewDir}
          lower_store='${lowerStoreUri}'
          # The same lower store opened writable. Legitimate: its state directory
          # is the tmpfs root's own /nix/var/nix, rebuilt from `regInfo` on every
          # boot, and adding a row for a path whose bytes really are in the lower
          # layer is the same operation `registerClosure` performs at boot.
          lower_store_rw='local?real=${lowerStoreViewDir}&state=${lowerStateDir}'
          canary_expr=${storeCanaryExpression}
          persist_record=$upper_root/.vivarium-spike-persistence-canary
          roots_dir=${upperStateDir}/gcroots

          # The boot counter is on the volume, so it *is* a persistence
          # observation as well as a branch: on a cold volume it cannot exist.
          boots_file=$upper_root/.vivarium-spike-boots
          boot=1
          if test -r "$boots_file"; then boot=$(( $(cat "$boots_file") + 1 )); fi
          echo "$boot" >"$boots_file" || true
          echo "VIVARIUM_STORE_SPIKE_BOOT=$boot"
          # overlayfs runs in the guest, so the guest's kernel is the one that
          # governs the stale-handle behaviour — not the host's.
          echo "VIVARIUM_STORE_SPIKE_GUEST_KERNEL=$(uname -r)"
          echo "VIVARIUM_STORE_SPIKE_NIX_VERSION=$($nix_store --version 2>&1 | head -n1)"

          # --- premise (b): a volume backs the writable layer, from the initrd ---
          mount_field() { awk -v mp="$1" -v f="$2" '$2 == mp { print $f; exit }' /proc/self/mounts; }
          mount_id() { awk -v mp="$1" '$5 == mp { print $1; exit }' /proc/self/mountinfo; }
          echo "VIVARIUM_STORE_SPIKE_UPPER_MOUNT=$(mount_field "$upper_root" 1) $(mount_field "$upper_root" 3) $(mount_field "$upper_root" 4)"
          echo "VIVARIUM_STORE_SPIKE_STORE_MOUNT=$(mount_field /nix/store 1) $(mount_field /nix/store 3)"
          echo "VIVARIUM_STORE_SPIKE_LOWER_VIEW_MOUNT=$(mount_field "$lower_view" 1) $(mount_field "$lower_view" 3)"
          case "$(mount_field "$upper_root" 1)" in
            /dev/*) echo 'VIVARIUM_STORE_SPIKE_UPPER_DEVICE=block' ;;
            *) echo 'VIVARIUM_STORE_SPIKE_UPPER_DEVICE=unexpected' ;;
          esac
          # Mount IDs are allocated in mount order, so the writable layer having a
          # lower id than the overlay built from it is direct evidence it was
          # established first — which is what `neededForBoot` is supposed to buy.
          upper_id=$(mount_id "$upper_root"); store_id=$(mount_id /nix/store)
          echo "VIVARIUM_STORE_SPIKE_MOUNT_IDS=upper=$upper_id view=$(mount_id "$lower_view") store=$store_id"
          if test -n "$upper_id" && test -n "$store_id" && test "$upper_id" -lt "$store_id"; then
            echo 'VIVARIUM_STORE_SPIKE_UPPER_BEFORE_STORE=yes'
          else
            echo 'VIVARIUM_STORE_SPIKE_UPPER_BEFORE_STORE=no'
          fi
          # ADR-0067's stage-2 first-boot machinery must be *absent* here, not
          # merely assumed away: this volume mounts before stage 2 exists.
          echo "VIVARIUM_STORE_SPIKE_UPPER_OWNER=$(stat -c '%U:%G %a' "$upper_root")"
          if test -e "$upper_root/.vivarium-first-boot"; then
            echo 'VIVARIUM_STORE_SPIKE_STAGE2_OWNERSHIP=present'
          else
            echo 'VIVARIUM_STORE_SPIKE_STAGE2_OWNERSHIP=absent'
          fi

          # --- premise (c): the registered closure and the daemon compose ---
          system_path=$(readlink -f /run/current-system)
          echo "VIVARIUM_STORE_SPIKE_SYSTEM_PATH=$system_path"
          if $nix_store --store "$lower_store" --check-validity "$system_path" >/dev/null 2>&1; then
            echo 'VIVARIUM_STORE_SPIKE_LOWER_DB_HAS_SYSTEM=yes'
          else
            echo 'VIVARIUM_STORE_SPIKE_LOWER_DB_HAS_SYSTEM=no'
          fi
          # The daemon's own environment is the least deniable evidence that the
          # overlay store is the one it opened. Two things had to be learned the
          # expensive way: `systemctl show -p MainPID` reads 0 for a
          # socket-activated unit, and nothing above this line had yet spoken to
          # the daemon, so on the first two runs there was no process to find.
          # Contact it first, then look for it.
          $nix_store --check-validity "$system_path" >/dev/null 2>&1 || true
          daemon_remote=unavailable
          for environ in /proc/[0-9]*/environ; do
            proc=''${environ%/environ}
            case "$(tr '\0' ' ' <"$proc/cmdline" 2>/dev/null || true)" in
              *nix-daemon*--daemon*)
                found=$(tr '\0' '\n' <"$environ" 2>/dev/null | grep '^NIX_REMOTE=' | head -n1 || true)
                test -n "$found" && daemon_remote=$found && break ;;
            esac
          done
          echo "VIVARIUM_STORE_SPIKE_DAEMON_NIX_REMOTE=$daemon_remote"

          # --- upstream's `check-post-init` analogue: lower and merged agree ---
          sample=$($nix_store -q --requisites "$system_path" 2>/dev/null | sed -n '3p')
          echo "VIVARIUM_STORE_SPIKE_SAMPLE_PATH=$sample"
          if test -n "$sample"; then
            merged=no; lower=no; verified=no
            $nix_store --check-validity "$sample" >/dev/null 2>&1 && merged=yes
            $nix_store --store "$lower_store" --check-validity "$sample" >/dev/null 2>&1 && lower=yes
            $nix_store --verify-path "$sample" >/dev/null 2>&1 && verified=yes
            echo "VIVARIUM_STORE_SPIKE_CHECK_POST_INIT=merged=$merged lower=$lower verified=$verified"
          else
            echo 'VIVARIUM_STORE_SPIKE_CHECK_POST_INIT=indeterminate-no-sample'
          fi

          # --- upstream's `redundant-add` analogue: ADR-0087's sharing claim ---
          # A path already valid in the lower database must never be copied up.
          # This count is what decides whether the volume grows by a closure or by
          # what the guest actually fetched.
          copied=0; total=0
          while IFS= read -r requisite; do
            total=$((total + 1))
            test -e "$upper_layer/$(basename "$requisite")" && copied=$((copied + 1))
          done < <($nix_store -q --requisites "$system_path" 2>/dev/null)
          echo "VIVARIUM_STORE_SPIKE_BOOT_CLOSURE_COPIED_UP=$copied/$total"

          # `-i` and `--output` are mutually exclusive in coreutils, and the
          # combination silently produced two empty measurements on the first run.
          df_report() {
            echo "VIVARIUM_STORE_SPIKE_DF_H_$1=$(df -h "$upper_root" 2>&1 | tail -n1)"
            echo "VIVARIUM_STORE_SPIKE_DF_I_$1=$(df -i "$upper_root" 2>&1 | tail -n1)"
          }
          df_report EARLY

          # --- the persistence canary: the database, not only the bytes ---
          # A path added by the guest, with no twin below, so its validity can
          # only come from the *upper* store's database. On a warm boot it must
          # already be valid before anything is built or added. A single boot
          # cannot show this, which is why two is the floor for ADR-0087.
          if test -r "$persist_record"; then
            recorded=$(cat "$persist_record")
            echo "VIVARIUM_STORE_SPIKE_PERSIST_RECORDED=$recorded"
            valid=no; upper=absent
            $nix_store --check-validity "$recorded" >/dev/null 2>&1 && valid=yes
            test -e "$upper_layer/$(basename "$recorded")" && upper=present
            echo "VIVARIUM_STORE_SPIKE_PERSIST_VALID_BEFORE_WRITE=$valid"
            echo "VIVARIUM_STORE_SPIKE_PERSIST_UPPER_BEFORE_WRITE=$upper"
          else
            echo 'VIVARIUM_STORE_SPIKE_PERSIST_RECORDED=none-cold-volume'
          fi
          printf 'vivarium-persistence-canary\n' >/run/vivarium-persistence-canary
          persist_status=0
          persist=$($nix_store --add /run/vivarium-persistence-canary 2>/run/vivarium-persist.log) || persist_status=$?
          echo "VIVARIUM_STORE_SPIKE_PERSIST_STATUS=$persist_status"
          echo "VIVARIUM_STORE_SPIKE_PERSIST_PATH=$persist"
          if test -n "$persist" && test -e "$persist"; then
            printf '%s' "$persist" >"$persist_record"
            mkdir -p "$roots_dir"
            ln -sfn "$persist" "$roots_dir/vivarium-persistence-canary"
          else
            echo "VIVARIUM_STORE_SPIKE_PERSIST_ERROR=$(tail -n 3 /run/vivarium-persist.log | tr '\n' '|')"
          fi

          # --- the delete-duplicate scenario, in the two shapes it really has ---
          # Upstream's own `stale-file-handle` test is unrunnable here: it
          # garbage-collects the *lower* store three times, and ours is served
          # --readonly. This is the reachable substitute, and the first host run
          # showed the design checklist had it half wrong. `deleteStorePath`
          # guards on `lowerStore->isValidPath(storePath)` — validity in the lower
          # *database*, not presence of bytes below — so there are two branches
          # and only one of them avoids a whiteout.
          duplicate_status=0
          duplicate=$(timeout 300 $nix_build --no-out-link "$canary_expr" 2>"/run/vivarium-canary.log") || duplicate_status=$?
          echo "VIVARIUM_STORE_SPIKE_DUPLICATE_STATUS=$duplicate_status"
          echo "VIVARIUM_STORE_SPIKE_DUPLICATE_PATH=$duplicate"
          if ((duplicate_status != 0)); then
            echo "VIVARIUM_STORE_SPIKE_DUPLICATE_ERROR=$(tail -n 5 /run/vivarium-canary.log | tr '\n' '|')"
          fi

          delete_arm() {
            # $1 store-path base name, $2 marker suffix. Prints what the two
            # layers and the merged view look like after deleting the duplicate.
            local name=$1 suffix=$2 out upper merged whiteout
            out=$(timeout 300 $nix_store --delete "$duplicate" 2>&1) || true
            upper=present; test -e "$upper_layer/$name" || upper=absent
            merged=absent; test -e "/nix/store/$name" && merged=present
            whiteout=none; test -c "$upper_layer/$name" && whiteout=char-device
            echo "VIVARIUM_STORE_SPIKE_DELETE_$suffix=upper=$upper merged=$merged whiteout=$whiteout"
            echo "VIVARIUM_STORE_SPIKE_DELETE_OUTPUT_$suffix=$(printf '%s' "$out" | tr '\n' '|' | tail -c 300)"
          }

          if test -n "$duplicate" && test -e "$duplicate"; then
            name=$(basename "$duplicate")
            lower_present=no; upper_present=no
            test -e "$lower_dir/$name" && lower_present=yes
            test -e "$upper_layer/$name" && upper_present=yes
            lower_valid=no
            $nix_store --store "$lower_store" --check-validity "$duplicate" >/dev/null 2>&1 && lower_valid=yes
            echo "VIVARIUM_STORE_SPIKE_DUPLICATE_LAYERS=lower_bytes=$lower_present lower_valid=$lower_valid upper=$upper_present"

            if test "$lower_present" = yes && test "$upper_present" = yes; then
              # Arm 1 — bytes below, unknown to the lower database. This is the
              # ordinary write path ADR-0087 describes, and upstream's `else`
              # branch: deleted through the merged view, so a whiteout is the
              # *expected* outcome rather than a defect. It is not the
              # catastrophic case, because the path is not one the guest's store
              # considers valid and a rebuild replaces the whiteout outright.
              delete_arm "$name" UNREGISTERED
              rebuild_status=0
              timeout 300 $nix_build --no-out-link "$canary_expr" >/dev/null 2>"/run/vivarium-rebuild.log" || rebuild_status=$?
              echo "VIVARIUM_STORE_SPIKE_REBUILD_AFTER_UNREGISTERED=$rebuild_status"
              if ((rebuild_status != 0)) || grep -qi 'stale file handle' /run/vivarium-rebuild.log 2>/dev/null; then
                echo "VIVARIUM_STORE_SPIKE_REBUILD_ERROR=$(tail -n 5 /run/vivarium-rebuild.log | tr '\n' '|')"
              fi

              # Arm 2 — the branch the remount hook exists for. Register the path
              # in the lower database first, which asserts nothing false: its
              # bytes really are in the lower layer, and this is the state a later
              # boot closure produces on its own for anything the guest had
              # already copied up. Now `deleteStorePath` must delete through the
              # upper layer, remount, and leave the merged view resolving to the
              # lower inode with no whiteout at all.
              register_status=0
              $nix_store --dump-db "$duplicate" >/run/vivarium-canary-db 2>/dev/null || register_status=$?
              $nix_store --store "$lower_store_rw" --load-db </run/vivarium-canary-db >/dev/null 2>&1 || register_status=$?
              lower_valid_now=no
              $nix_store --store "$lower_store" --check-validity "$duplicate" >/dev/null 2>&1 && lower_valid_now=yes
              echo "VIVARIUM_STORE_SPIKE_LOWER_REGISTER=status=$register_status lower_valid=$lower_valid_now"
              if test "$lower_valid_now" = yes && test -e "$upper_layer/$name"; then
                merged_inode_before=$(stat -c %i "/nix/store/$name" 2>/dev/null || echo unknown)
                lower_inode=$(stat -c %i "$lower_dir/$name" 2>/dev/null || echo unknown)
                delete_arm "$name" REGISTERED
                merged_inode_after=$(stat -c %i "/nix/store/$name" 2>/dev/null || echo unknown)
                echo "VIVARIUM_STORE_SPIKE_DELETE_INODES=merged_before=$merged_inode_before merged_after=$merged_inode_after lower=$lower_inode"
              else
                echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-registration-did-not-take'
              fi
            else
              echo 'VIVARIUM_STORE_SPIKE_DELETE_UNREGISTERED=skipped-no-duplicate'
              echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-no-duplicate'
            fi
          else
            echo 'VIVARIUM_STORE_SPIKE_DUPLICATE_LAYERS=indeterminate-no-duplicate'
            echo 'VIVARIUM_STORE_SPIKE_DELETE_UNREGISTERED=skipped-no-duplicate'
            echo 'VIVARIUM_STORE_SPIKE_DELETE_REGISTERED=skipped-no-duplicate'
          fi

          # --- add-on: does a collection's remount disturb a running process? ---
          # Expected safe: a remount reconfigures the superblock, and open fds and
          # mappings survive it. Cheap enough to stop being an expectation.
          sleep 120 >/dev/null 2>&1 &
          sleeper=$!
          sleeper_exe=$(readlink "/proc/$sleeper/exe" 2>/dev/null || true)

          # --- ADR-0088's core claim: a rooted collection is non-destructive ---
          # `deleteStorePath` returns immediately unless the path exists in the
          # upper layer, so the thousands of host paths the guest never copied up
          # cannot be whiteed out. The lower layer's entry count either side of a
          # collection is the host-visible statement of that.
          lower_before=$(find "$lower_dir" -mindepth 1 -maxdepth 1 | wc -l)
          whiteouts_before=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c 2>/dev/null | wc -l)
          gc_status=0
          timeout 300 $nix_collect_garbage >/run/vivarium-gc.log 2>&1 || gc_status=$?
          lower_after=$(find "$lower_dir" -mindepth 1 -maxdepth 1 | wc -l)
          whiteouts=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c 2>/dev/null | wc -l)
          system_still=no
          $nix_store --check-validity "$system_path" >/dev/null 2>&1 && system_still=yes
          system_present=no
          test -e "$system_path" && system_present=yes
          persist_survived=no
          test -n "''${persist:-}" && test -e "$persist" && persist_survived=yes
          echo "VIVARIUM_STORE_SPIKE_GC=status=$gc_status lower_before=$lower_before lower_after=$lower_after whiteouts_before=$whiteouts_before whiteouts=$whiteouts system_valid=$system_still system_present=$system_present rooted_canary=$persist_survived"
          # Name them, and — the question that actually decides whether ADR-0088
          # holds — say how many sit over a path the *lower database* considers
          # valid. Upstream's `else` branch writes a whiteout whenever an
          # upper-layer path is absent from the lower database, so a whiteout over
          # an unregistered path is specified behaviour. A whiteout over a
          # registered one would be the guest blinding itself to its own boot
          # closure, and that must be zero.
          whiteout_names=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -type c -printf '%f\n' 2>/dev/null || true)
          echo "VIVARIUM_STORE_SPIKE_WHITEOUT_NAMES=$(printf '%s' "$whiteout_names" | head -n 10 | tr '\n' ' ')"
          whiteout_over_valid=0
          while IFS= read -r whiteout; do
            test -n "$whiteout" || continue
            if $nix_store --store "$lower_store" --check-validity "/nix/store/$whiteout" >/dev/null 2>&1; then
              whiteout_over_valid=$((whiteout_over_valid + 1))
              echo "VIVARIUM_STORE_SPIKE_WHITEOUT_OVER_VALID_NAME=$whiteout"
            fi
          done <<<"$whiteout_names"
          echo "VIVARIUM_STORE_SPIKE_WHITEOUT_OVER_VALID=$whiteout_over_valid"
          echo "VIVARIUM_STORE_SPIKE_GC_OUTPUT=$(tail -n 3 /run/vivarium-gc.log | tr '\n' '|' | tail -c 300)"

          if kill -0 "$sleeper" 2>/dev/null && test "$(readlink "/proc/$sleeper/exe" 2>/dev/null || true)" = "$sleeper_exe"; then
            echo 'VIVARIUM_STORE_SPIKE_EXEC_ACROSS_REMOUNT=intact'
          else
            echo 'VIVARIUM_STORE_SPIKE_EXEC_ACROSS_REMOUNT=disturbed'
          fi
          kill "$sleeper" 2>/dev/null || true

          # --- add-on: a stray name in the share root aborts a guest collection ---
          # `deleteStorePath` parses a store-path name *before* reaching the guard
          # above, so one non-store entry in the lower layer is a hard stop. The
          # same question applies to the upper layer, where LocalStore's own
          # `.links` lands.
          stray_lower=$(find "$lower_dir" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vcE '^[0-9a-z]{32}-' || true)
          stray_upper=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vcE '^[0-9a-z]{32}-' || true)
          echo "VIVARIUM_STORE_SPIKE_STRAY_NAMES=lower=$stray_lower upper=$stray_upper"
          echo "VIVARIUM_STORE_SPIKE_STRAY_UPPER_NAMES=$(find "$upper_layer" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null | grep -vE '^[0-9a-z]{32}-' | head -n 10 | tr '\n' ' ')"

          df_report LATE
          echo "VIVARIUM_STORE_SPIKE_UPPER_USAGE=$(du -sk "$upper_layer" 2>/dev/null | awk '{print $1}')KiB"
          echo 'VIVARIUM_STORE_SPIKE_COMPLETE'
        '';
      };

      vivarium-first-microvm-diagnostic = {
        description = "Deterministic first-microVM diagnostic";
        wantedBy = [ "multi-user.target" ];
        after = [
          "vivarium-store-spike.service"
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
}
