{
  config,
  lib,
  pkgs,
  volumeDirSentinel,
  storeCanaryExpression,
  gcInterlockCanaryExpression,
  gcInterlockControlExpression,
  supervisorPackage,
  networkLayout,
  # ADR-0096 takes the daemon's own default, uniform across shares — measured, on
  # a concurrent sweep in which no non-zero pool won a cell. The *value* lives
  # here rather than in `spec/06`, which states the property only, precisely so a
  # benchmark can move it without touching a specified sentence; that is what
  # happened. Parameterised so the sweep varies it without rebuilding the guest
  # (ADR-0095).
  virtiofsdThreadPoolSize ? 0,
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
  #
  # There is no per-volume image token and no per-role argument. The guest's own
  # `image` field already spells `<sentinel>/<name>.img`, so replacing the
  # sentinel yields the tokenized path and stripping it yields the name — which
  # keeps the number of disks a property of this list rather than of how many
  # flags the host remembered to pass. `name` is recovered rather than carried
  # because upstream requires `image` to be unique across volumes anyway
  # (microvm.nix's own `asserts.nix`), so it is the field that cannot collide.
  volumeLaunch = map (
    volume:
    let
      isStore = volume.mountPoint == config.microvm.writableStoreOverlay;
    in
    {
      inherit (volume) label imageType;
      name = lib.removeSuffix ".img" (baseNameOf volume.image);
      imagePath = lib.replaceStrings [ volumeDirSentinel ] [ "@VOLUME_DIR@" ] volume.image;
      sizeMiB = volume.size;
      # What this volume is, for a reader that must not re-derive the store
      # predicate above. `home` is the one the first-boot ownership rules and the
      # reserved-order assertion name; `declared` is everything a layer asked for.
      role =
        if isStore then
          "store"
        else if volume.mountPoint == sessionUser.home then
          "home"
        else
          "declared";
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
      # Where the guest mounts it. The guest's own fstab is what enacts this; the
      # launcher carries it so `viv exec` can name the workspace cwd a session
      # starts in without a second, silently divergent spelling of a path the
      # guest module alone decides.
      mountPoint
      ;
    socketToken = "@${lib.toUpper share.tag}_SOCKET@";
    # The store source is a machine-independent constant (mounts.nix selects on
    # it); the workspace source is launch-channel and stays a token (N5).
    sourceToken = if share.source == "/nix/store" then "/nix/store" else "@WORKSPACE_SOURCE@";
  }) shares;
  # What a guest process gets that no host variable could supply.
  #
  # The guest agent clears the environment before every spawn and inherits
  # nothing, and spec/12 makes host passthrough deny-by-default — so without
  # this a non-login `viv exec` has no PATH at all and cannot resolve a bare
  # program name. These are facts of the image, not forwarded host values, which
  # is why they are derived here rather than named on the host side.
  sessionUser = config.users.users.vivarium;
  # NixOS spells the session profile list with the shell placeholders that
  # `/etc/set-environment` leaves to the shell. A session started through the
  # control socket has no shell to expand them, so they are expanded here against
  # the environment that session actually has — while the list itself still comes
  # from the guest's own configuration.
  #
  # `XDG_STATE_HOME` expands to nothing, and that is not an oversight: spec/12
  # makes host passthrough deny-by-default and the variable is not on the
  # allowlist, so it is unset in every session and the shell would drop it too.
  # Guessing its conventional value instead produced a PATH entry the guest does
  # not have, which `tests/nix/contract.sh` caught by comparing this against the
  # guest's own script rather than against a second reading of these rules.
  expandProfile =
    lib.replaceStrings
      [
        "$HOME"
        "\${XDG_STATE_HOME}"
        "$USER"
      ]
      [
        sessionUser.home
        ""
        sessionUser.name
      ];
  guestSession = {
    user = sessionUser.name;
    inherit (sessionUser) home;
    # The option holds the shell *package*; `/etc/passwd` holds the executable
    # inside it, which is what a `SHELL` value has to name.
    shell = lib.getExe sessionUser.shell;
    # Deduplicated because two of NixOS's placeholders expand to the same
    # directory once `$HOME` is known, and a PATH that names one twice is a
    # difference the guest would show and nothing would explain.
    path = lib.concatStringsSep ":" (
      lib.unique (
        [ config.security.wrapperDir ] ++ map (p: "${expandProfile p}/bin") config.environment.profiles
      )
    );
  };
in
{
  # 6 since guest networking: the `egress` and `network` objects and six backend
  # programs joined, and the supervisor that parses this creates a namespace
  # pair, a tap, and — under allowlist mode — a ruleset and a resolver that an
  # older handoff never described. 5 was named volumes: `volumeLaunch` stopped
  # carrying a per-role image token and the launcher took one `--volume-dir`
  # from which each entry's `imagePath` is joined. The version pairs this JSON
  # with the `viv` that parses the specification rendered from it.
  #
  # It deliberately does not catch the other half of a bump. The runner's own
  # argument names can move, and a generated flake pins its own `vivarium`, so a
  # new `viv` can drive an older runner — which then refuses at `usage()` before
  # any schema is read. That refusal prints this number for exactly that reason.
  schemaVersion = 6;
  inherit guestSession;
  # The launch half of `sandbox.egress` (spec/05): carried across so host-side
  # enforcement needs no evaluation at start. `or`-defaulted because the shipped
  # diagnostic image composes no tool options — there "declared nothing" and
  # "cannot declare" are the same answer, and the default is spec/05's own.
  egress = {
    mode = config.sandbox.egress.mode or "open";
    allow = config.sandbox.egress.allow or [ ];
  };
  # The guest link's addressing, from the one declaration in `nix/default.nix`
  # that the guest module also reads.
  network = {
    inherit (networkLayout)
      tapName
      gatewayAddress
      prefixLength
      guestAddress
      dnsForwardAddress
      guestMac
      resolverPort
      ;
  };
  descriptorBudget = {
    limit = 524288;
    workerPoolSize = virtiofsdThreadPoolSize;
  };
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
  systemdRun = lib.getExe' pkgs.systemd "systemd-run";
  supervisor = lib.getExe' supervisorPackage "vivarium-supervisor";
  truncate = lib.getExe' pkgs.coreutils "truncate";
  mkfsExt4 = lib.getExe' pkgs.e2fsprogs "mkfs.ext4";
  # The six programs guest networking added (spec/05, schema 6): the pinned
  # util-linux pair that creates and joins the per-VM namespaces, `ip` for the
  # tap one-shots, `nft` for the allowlist ruleset, `pasta` as the one
  # unprivileged uplink process per VM, and `sleep` as the holder's body.
  unshare = lib.getExe' pkgs.util-linux "unshare";
  nsenter = lib.getExe' pkgs.util-linux "nsenter";
  ip = lib.getExe' pkgs.iproute2 "ip";
  nft = lib.getExe' pkgs.nftables "nft";
  pasta = lib.getExe' pkgs.passt "pasta";
  sleep = lib.getExe' pkgs.coreutils "sleep";
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
    "--vsock"
    "cid=3,socket=@CONTROL_SOCKET@"
    # The guest NIC: the VMM opens the tap by name inside the VM's own namespace,
    # where the supervisor created it. No token — both values are build-owned.
    "--net"
    "tap=${networkLayout.tapName},mac=${networkLayout.guestMac}"
  ]
  ++ lib.concatMap (volume: [
    "--disk"
    "path=${volume.imagePath},direct=off,readonly=off,image_type=${volume.imageType},sparse=on"
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
    controlSocket = "@CONTROL_SOCKET@";
    gid = "@GID@";
    memoryMiB = "@MEMORY_MIB@";
    storeSocket = "@STORE_SOCKET@";
    uid = "@UID@";
    vcpu = "@VCPU@";
    volumeDir = "@VOLUME_DIR@";
    workspaceSocket = "@WORKSPACE_SOCKET@";
    workspaceSource = "@WORKSPACE_SOURCE@";
  };
  socketLegs = {
    api = "@API_SOCKET@";
    console = "@CONSOLE_SOCKET@";
    credentials = [ ];
    shares = map (share: {
      inherit (share) tag;
      socket = share.socketToken;
    }) shareLaunch;
  };
  credentialIds = config.vivarium.credentials.agents;
  # Cloud Hypervisor v53.0's `VmConfig` JSON, submitted to `ch-remote create`
  # before the separate `boot` call. Field spellings are asserted in
  # contract.nix so a backend pin move cannot silently collapse this ordering.
  vmCreate = {
    cpus = {
      boot_vcpus = 1;
      max_vcpus = 1;
    };
    memory = {
      size = 536870912;
      shared = true;
    };
    payload = {
      kernel = kernelPath;
      initramfs = config.microvm.initrdPath;
      cmdline = "${kernelConsole} reboot=t panic=-1 ${toString config.microvm.kernelParams}";
    };
    disks = map (volume: {
      path = volume.imagePath;
      direct = false;
      readonly = false;
      image_type = if volume.imageType == "raw" then "Raw" else "Qcow2";
      sparse = true;
    }) volumeLaunch;
    fs = map (share: {
      inherit (share) tag;
      socket = share.socketToken;
    }) shareLaunch;
    balloon = {
      size = 0;
      free_page_reporting = true;
      deflate_on_oom = true;
    };
    console = {
      mode = "Off";
    };
    serial = {
      mode = "Socket";
      socket = "@CONSOLE_SOCKET@";
    };
    vsock = {
      cid = 3;
      socket = "@CONTROL_SOCKET@";
    };
    # The `--net` entry above, in `VmConfig` spelling: two renderings of one
    # device, like the serial and vsock pairs around it.
    net = [
      {
        tap = networkLayout.tapName;
        mac = networkLayout.guestMac;
      }
    ];
    watchdog = true;
    landlock_enable = true;
    landlock_rules = [ ];
  };
}
