{
  pkgs,
  launchArguments,
  supervisorPackage,
}:

let
  launchArgumentsFile = pkgs.writeText "vivarium-first-microvm-launch-arguments.json" (
    builtins.toJSON launchArguments
  );
in
pkgs.writeShellApplication {
  name = "vivarium-first-microvm";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.gnugrep
    pkgs.jq
  ];
  text =
    builtins.replaceStrings
      [ "@launchArgumentsPath@" "@vivPath@" ]
      [
        (toString launchArgumentsFile)
        "${supervisorPackage}/bin/viv"
      ]
      ''
        set -euo pipefail
        contract=@launchArgumentsPath@
        workspace=""
        runtime_dir=""
        volume=""
        store_volume=""
        uid=""
        gid=""
        memory_mib=""
        vcpu=""
        project_id=first-microvm
        target=default
        agent_socket=""
        console_log=true
        print_only=false
        usage() {
          echo "usage: $0 --workspace ABS --runtime-dir ABS --volume ABS --store-volume ABS --uid N --gid N --memory-mib N --vcpu N [--project-id ID] [--target NAME] [--agent-socket ABS] [--no-console-log] [--print-static-arguments]" >&2
          exit 64
        }
        while (($#)); do
          case $1 in
            --workspace|--runtime-dir|--volume|--store-volume|--uid|--gid|--memory-mib|--vcpu|--project-id|--target|--agent-socket)
              (($# >= 2)) || usage
              name=''${1#--}; name=''${name//-/_}; printf -v "$name" '%s' "$2"; shift 2 ;;
            --no-console-log) console_log=false; shift ;;
            --print-static-arguments) print_only=true; shift ;;
            *) usage ;;
          esac
        done
        for value in workspace runtime_dir volume store_volume; do
          candidate=''${!value:-}; [[ $candidate = /* ]] || usage
        done
        for value in uid gid memory_mib vcpu; do [[ ''${!value:-} =~ ^[0-9]+$ ]] || usage; done
        [[ -n $project_id && -n $target ]] || usage
        [[ -z $agent_socket || $agent_socket = /* ]] || usage
        workspace=$(realpath -m -- "$workspace")
        runtime_dir=$(realpath -m -- "$runtime_dir")
        volume=$(realpath -m -- "$volume")
        store_volume=$(realpath -m -- "$store_volume")
        spec=$runtime_dir/launch.json
        api=$runtime_dir/api.sock
        console=$runtime_dir/console.sock
        ready=$runtime_dir/ready.sock
        store_socket=$runtime_dir/store.sock
        workspace_socket=$runtime_dir/workspace.sock
        # This is a host capability observation, not a user-selectable policy flag.
        # Unsupported Landlock is serialized as a visible soft degradation.
        landlock=false
        [[ -r /sys/kernel/security/lsm ]] && grep -qw landlock /sys/kernel/security/lsm && landlock=true
        output=$spec
        if $print_only; then
          output=/dev/stdout
        else
          umask 077
          mkdir -p "$runtime_dir"
        fi
        jq \
          --arg project "$project_id" --arg target "$target" --arg runtime "$runtime_dir" \
          --arg workspace "$workspace" --arg volume "$volume" --arg storeVolume "$store_volume" \
          --arg api "$api" --arg console "$console" --arg ready "$ready" \
          --arg storeSocket "$store_socket" --arg workspaceSocket "$workspace_socket" \
          --arg agent "$agent_socket" --argjson uid "$uid" --argjson gid "$gid" \
          --argjson memoryMiB "$memory_mib" --argjson vcpu "$vcpu" \
          --argjson landlock "$landlock" --argjson consoleLog "$console_log" '
          def token:
            if type != "string" then . else
              gsub("@WORKSPACE_SOURCE@"; $workspace)
              | gsub("@VOLUME_IMAGE@"; $volume)
              | gsub("@STORE_VOLUME_IMAGE@"; $storeVolume)
              | gsub("@STORE_SOCKET@"; $storeSocket)
              | gsub("@WORKSPACE_SOCKET@"; $workspaceSocket)
              | gsub("@API_SOCKET@"; $api)
              | gsub("@CONSOLE_SOCKET@"; $console)
            end;
          . as $c |
          {
            schemaVersion: .schemaVersion,
            projectId: $project,
            target: $target,
            runtimePaths: { root: $runtime, launchSpec: ($runtime + "/launch.json"), readySocket: $ready,
              apiSocket: $api, consoleSocket: $console, consoleLog: ($runtime + "/console.log"),
              vmPid: ($runtime + "/vm.pid"), bootJson: ($runtime + "/boot.json") },
            backendPrograms: { cloudHypervisor: .cloudHypervisor, chRemote: .chRemote,
              virtiofsd: .virtiofsd, setpriv: .setpriv, truncate: .truncate,
              mkfsExt4: .mkfsExt4, systemdRun: .systemdRun, supervisor: .supervisor },
            socketLegs: { api: $api, console: $console, agent: (if $agent == "" then null else $agent end) },
            resources: { vcpus: $vcpu, memoryMiB: $memoryMiB, cpuWeight: 100 },
            descriptorBudget: .descriptorBudget,
            identityTranslation: (.idTranslation + { hostUid: $uid, hostGid: $gid }),
            shares: [.shareLaunch[] | { tag, source: (.sourceToken | token), socket: (.socketToken | token), cache,
              readOnly, extraArgs: [.extraArgs[]? | select(. != "@UID_TRANSLATION@" and . != "@GID_TRANSLATION@") | token] }],
            volumes: [.volumeLaunch[] | { label, path: (if .argName == "store-volume" then $storeVolume else $volume end),
              sizeMiB, imageType, inodeRatio }],
            vmCreate: (.vmCreate | walk(token)
              | .cpus.boot_vcpus = $vcpu | .cpus.max_vcpus = $vcpu
              | .memory.size = ($memoryMiB * 1048576)
              | .landlock_enable = $landlock),
            landlockAvailable: $landlock,
            consoleLogEnabled: $consoleLog
          }' "$contract" > "$output"
        $print_only && exit 0
        chmod 0600 "$spec"
        exec @vivPath@ start --spec "$spec"
      '';
}
