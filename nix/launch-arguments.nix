{
  config,
  lib,
  pkgs,
  storeCanaryExpression,
  gcInterlockCanaryExpression,
  gcInterlockControlExpression,
  # ADR-0051 pins a small non-zero pool, uniform across shares. The *value* lives
  # here rather than in `spec/06`, which states the property only, precisely so a
  # benchmark can move it without touching a specified sentence. Parameterised so
  # the sweep varies it without rebuilding the guest (ADR-0095).
  virtiofsdThreadPoolSize ? 4,
}:

let
  inherit (config.microvm) shares volumes;
  inherit (pkgs.stdenv.hostPlatform) system;
  kernelPath =
    if system == "x86_64-linux" then
      "${config.microvm.kernel.dev}/vmlinux"
    else
      "${config.microvm.kernel}/${config.system.boot.loader.kernelFile}";
  # The serial device the guest logs to is arch-specific, and it must agree with
  # the `--serial socket=` wiring below or the console carries nothing. Upstream's
  # own Cloud Hypervisor runner makes the same split.
  kernelConsole =
    if system == "x86_64-linux" then
      "earlyprintk=ttyS0 console=ttyS0"
    else if system == "aarch64-linux" then
      "console=ttyAMA0"
    else
      throw "vivarium first microVM: unsupported system ${system}";
  shareByTag = tag: lib.findFirst (share: share.tag == tag) (throw "missing ${tag} share") shares;
  # A list, in `config.microvm.volumes` order, rather than a flat field set per
  # volume. The reason is correctness, not tidiness: cloud-hypervisor assigns
  # /dev/vda, /dev/vdb in `--disk` order and microvm.nix's `withDriveLetters`
  # assigns guest drive letters by the same list's order, so the ordering is a
  # contract between the two halves that parallel fields cannot express.
  #
  # The store volume is identified by upstream's *own* predicate — the volume
  # whose mount point is the writable store overlay — which is the same test
  # mounts.nix uses to grant it `neededForBoot`. Matching a label string here
  # would be a second, silently divergent definition of the same role.
  volumeLaunch = map (
    volume:
    let
      isStore = volume.mountPoint == config.microvm.writableStoreOverlay;
    in
    {
      inherit (volume) label imageType;
      sizeMiB = volume.size;
      argName = if isStore then "store-volume" else "volume";
      imageToken = if isStore then "@STORE_VOLUME_IMAGE@" else "@VOLUME_IMAGE@";
      # ADR-0091: the store volume is the one provisioned for file count as well
      # as for size, because ADR-0089's collection trigger reads free *blocks*
      # and structurally cannot see inode exhaustion. Every other volume keeps
      # the filesystem default.
      inodeRatio = if isStore then 8192 else null;
    }
  ) volumes;
  # Per-share virtiofsd policy is declared once, in the guest module, and read
  # from there. Duplicating it in the launcher would make the guest declaration
  # dead — a change to `cache` would alter no behaviour and raise no error.
  shareLaunch = map (share: {
    inherit (share)
      tag
      cache
      readOnly
      extraArgs
      ;
    socketToken = "@${lib.toUpper share.tag}_SOCKET@";
    # The store source is a machine-independent constant (mounts.nix selects on
    # it); the workspace source is launch-channel and stays a token (N5).
    sourceToken = if share.source == "/nix/store" then "/nix/store" else "@WORKSPACE_SOURCE@";
  }) shares;
in
{
  inherit volumeLaunch;
  # The host realises this same expression before booting, so the canary's bytes
  # reach the lower layer without its output path entering the guest's boot
  # closure. Carried here so the harness reads it from the launcher's own JSON
  # rather than from a second copy of the constant.
  storeCanaryExpression = toString storeCanaryExpression;
  # ADR-0085's two measurement paths, carried by the same route and for the same
  # reason. The guest never learns either output path from the closure — the
  # harness passes them through the workspace share at run time — because a store
  # reference to an output is exactly what would stop the host deleting it.
  gcInterlockCanaryExpression = toString gcInterlockCanaryExpression;
  gcInterlockControlExpression = toString gcInterlockControlExpression;
  cloudHypervisor = lib.getExe config.microvm.cloud-hypervisor.package;
  chRemote = lib.getExe' config.microvm.cloud-hypervisor.package "ch-remote";
  virtiofsd = lib.getExe config.microvm.virtiofsd.package;
  setpriv = lib.getExe' pkgs.util-linux "setpriv";
  truncate = lib.getExe' pkgs.coreutils "truncate";
  mkfsExt4 = lib.getExe' pkgs.e2fsprogs "mkfs.ext4";
  staticArguments = [
    "--cpus"
    "boot=@VCPU@"
    "--watchdog"
    "--kernel"
    kernelPath
    "--initramfs"
    config.microvm.initrdPath
    "--cmdline"
    "${kernelConsole} reboot=t panic=-1 ${toString config.microvm.kernelParams}"
    "--seccomp"
    "true"
    "--memory"
    "size=@MEMORY_MIB@M,shared=on"
    "--balloon"
    "size=0M,free_page_reporting=on,deflate_on_oom=on"
    "--console"
    "null"
    "--serial"
    "socket=@CONSOLE_SOCKET@"
  ]
  ++ lib.concatMap (volume: [
    "--disk"
    "path=${volume.imageToken},direct=off,readonly=off,image_type=${volume.imageType},sparse=on"
  ]) volumeLaunch
  ++ [
    "--fs"
    "tag=${(shareByTag "store").tag},socket=@STORE_SOCKET@"
    "--fs"
    "tag=${(shareByTag "workspace").tag},socket=@WORKSPACE_SOCKET@"
    "--api-socket"
    "@API_SOCKET@"
  ];
  inherit shareLaunch;
  # The guest half of the share identity contract is build-channel (spec/06: the
  # guest uid is fixed at image build); the host half is launch-channel, so the
  # launcher does the arithmetic. `idMax` is one below u32::MAX because virtiofsd
  # rejects a range whose base + count exceeds u32::MAX.
  idTranslation = {
    guestUid = config.users.users.vivarium.uid;
    guestGid = config.users.groups.vivarium.gid;
    # Linux's conventional overflow identities (/proc/sys/kernel/overflow{uid,gid}).
    overflowUid = 65534;
    overflowGid = 65534;
    idMax = 4294967294;
  };
  # Launcher-owned virtiofsd constants, kept here so the launcher reads every
  # daemon parameter from one place rather than from bash literals.
  #
  # `never` is justified as **determinism**, not as descriptor economy: the
  # daemon's own default is `prefer`, and it downgrades `Prefer -> Never` at
  # startup whenever a handle cannot be opened, so under the N20 profile the two
  # are behaviourally identical today. Pinning `never` is what stops behaviour
  # changing silently if the capability is ever granted.
  virtiofsdInodeFileHandles = "never";
  inherit virtiofsdThreadPoolSize;
  tokens = {
    apiSocket = "@API_SOCKET@";
    consoleSocket = "@CONSOLE_SOCKET@";
    gid = "@GID@";
    memoryMiB = "@MEMORY_MIB@";
    storeSocket = "@STORE_SOCKET@";
    uid = "@UID@";
    vcpu = "@VCPU@";
    volumeImage = "@VOLUME_IMAGE@";
    storeVolumeImage = "@STORE_VOLUME_IMAGE@";
    workspaceSocket = "@WORKSPACE_SOCKET@";
    workspaceSource = "@WORKSPACE_SOURCE@";
  };
}
