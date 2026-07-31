{
  config,
  lib,
  pkgs,
  volumeLabel,
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
  volume = builtins.head volumes;
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
  inherit volumeLabel;
  volumeSizeMiB = volume.size;
  volumeImageType = volume.imageType;
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
    "--disk"
    "path=@VOLUME_IMAGE@,direct=off,readonly=off,image_type=${volume.imageType},sparse=on"
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
  virtiofsdThreadPoolSize = 4;
  virtiofsdInodeFileHandles = "never";
  tokens = {
    apiSocket = "@API_SOCKET@";
    consoleSocket = "@CONSOLE_SOCKET@";
    gid = "@GID@";
    memoryMiB = "@MEMORY_MIB@";
    storeSocket = "@STORE_SOCKET@";
    uid = "@UID@";
    vcpu = "@VCPU@";
    volumeImage = "@VOLUME_IMAGE@";
    workspaceSocket = "@WORKSPACE_SOCKET@";
    workspaceSource = "@WORKSPACE_SOURCE@";
  };
}
