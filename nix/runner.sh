# shellcheck shell=bash
set -euo pipefail
contract=@launchArgumentsPath@
runtime_dir=""
volume_dir=""
supervisor=""
uid=""
gid=""
memory_mib=""
vcpu=""
sandbox_id=base-image
target=default
ssh_agent_socket=""
gpg_agent_socket=""
console_log=true
print_only=false
mounts_json='{}'
usage() {
  echo "usage: $0 --runtime-dir ABS --volume-dir ABS --supervisor ABS --uid N --gid N --memory-mib N --vcpu N [--sandbox-id ID] [--target NAME] [--ssh-agent-socket ABS] [--gpg-agent-socket ABS] [--mount TAG dir|tree|file ABS ENTRY]... [--no-console-log] [--print-static-arguments]" >&2
  # The belt for a hand-invoked runner. `viv` refuses a build whose contract
  # schema differs from its own before executing this script, but a runner from
  # an old generation can still be run by hand, and its argument names differ —
  # this refusal happens before any schema is read. Naming the contract's version
  # here is what turns "usage error" into "you are holding two halves of
  # different versions".
  echo "this launcher speaks launch contract schema $(jq -r '.schemaVersion' "$contract")" >&2
  exit 64
}
while (($#)); do
  case $1 in
    --runtime-dir | --volume-dir | --supervisor | --uid | --gid | --memory-mib | --vcpu | --sandbox-id | --target | --ssh-agent-socket | --gpg-agent-socket)
      (($# >= 2)) || usage
      name=${1#--}
      name=${name//-/_}
      printf -v "$name" '%s' "$2"
      shift 2
      ;;
    --mount)
      # Four values per flag — tag, kind, expanded absolute source, entry —
      # separate words rather than one delimited string, because a source path
      # may hold any delimiter. The entry is the percent-encoded basename a
      # `file` mount serves through its parent directory, and `-` for a `dir` or
      # `tree` mount. `tree` is a directory the manifest declared it owns: served
      # exactly as `dir` is, and named apart only so the rendered specification
      # records which rows came from `[[workspaces]]` rather than leaving a reader
      # to infer it from the target matching the source (ADR-0110). `viv` expands
      # and validates the declared source before invoking this script; the checks
      # here are the belt, not the diagnostic.
      (($# >= 5)) || usage
      mount_tag=$2
      mount_kind=$3
      mount_source=$4
      mount_entry=$5
      [[ $mount_tag =~ ^[a-z0-9]+$ ]] || usage
      [[ $mount_source = /* ]] || usage
      case $mount_kind in
        dir | tree) [[ $mount_entry = - ]] || usage ;;
        file) [[ $mount_entry =~ ^([A-Za-z0-9._~-]|%[0-9A-F]{2})+$ ]] || usage ;;
        *) usage ;;
      esac
      jq -e --arg tag "$mount_tag" 'has($tag) | not' <<<"$mounts_json" >/dev/null || usage
      mounts_json=$(jq --arg tag "$mount_tag" --arg kind "$mount_kind" \
        --arg source "$mount_source" --arg entry "$mount_entry" \
        '. + {($tag): {kind: $kind, source: $source,
          entry: (if $kind == "file" then $entry else null end)}}' <<<"$mounts_json")
      shift 5
      ;;
    --no-console-log)
      console_log=false
      shift
      ;;
    --print-static-arguments)
      print_only=true
      shift
      ;;
    *) usage ;;
  esac
done
for value in runtime_dir volume_dir supervisor; do
  candidate=${!value:-}
  [[ $candidate = /* ]] || usage
done
# The one program the contract does not build: the supervisor comes from the
# running installation (ADR-0102), so the invoking `viv` names it and this guard
# only holds it to the shape every other program already has.
[[ -x $supervisor ]] || usage
for value in uid gid memory_mib vcpu; do [[ ${!value:-} =~ ^[0-9]+$ ]] || usage; done
[[ -n $sandbox_id && -n $target ]] || usage
for value in ssh_agent_socket gpg_agent_socket; do
  candidate=${!value}
  [[ -z $candidate || $candidate = /* ]] || usage
done
declared_ssh=false
declared_gpg=false
while IFS= read -r id; do
  case $id in ssh) declared_ssh=true ;; gpg) declared_gpg=true ;; *) usage ;; esac
done < <(jq -r '.credentialIds[]' "$contract")
if $declared_ssh; then
  [[ -n $ssh_agent_socket && -S $ssh_agent_socket && ! -L $ssh_agent_socket ]] || usage
else
  [[ -z $ssh_agent_socket ]] || usage
fi
if $declared_gpg; then
  [[ -n $gpg_agent_socket && -S $gpg_agent_socket && ! -L $gpg_agent_socket ]] || usage
else
  [[ -z $gpg_agent_socket ]] || usage
fi
# The `--mount` set must be exactly the contract's declared shares — the same
# declared-in-contract-supplied-by-value shape as the credential sockets above.
declared_mount_tags=$(jq -r '[.shareLaunch[] | select(.origin == "declared") | .tag] | sort | join(" ")' "$contract")
supplied_mount_tags=$(jq -r 'keys | sort | join(" ")' <<<"$mounts_json")
[[ $declared_mount_tags == "$supplied_mount_tags" ]] || usage
runtime_dir=$(realpath -m -- "$runtime_dir")
# `-m`, so a directory that does not exist yet still resolves: the images are
# created by the supervisor, and `--print-static-arguments` renders a contract
# against paths no host has ever had. A `-d` test here would refuse that.
volume_dir=$(realpath -m -- "$volume_dir")
spec=$runtime_dir/launch.json
api=$runtime_dir/api.sock
console=$runtime_dir/console.sock
control=$runtime_dir/control.sock
ready=$runtime_dir/ready.sock
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
  --arg sandbox "$sandbox_id" --arg target "$target" --arg runtime "$runtime_dir" \
  --arg volumeDir "$volume_dir" \
  --arg api "$api" --arg console "$console" --arg control "$control" --arg ready "$ready" \
  --arg supervisor "$supervisor" \
  --arg sshAgent "$ssh_agent_socket" --arg gpgAgent "$gpg_agent_socket" --argjson uid "$uid" --argjson gid "$gid" \
  --argjson memoryMiB "$memory_mib" --argjson vcpu "$vcpu" --argjson mountSources "$mounts_json" \
  --argjson landlock "$landlock" --argjson consoleLog "$console_log" '
  def token:
    if type != "string" then . else
      gsub("@VOLUME_DIR@"; $volumeDir)
      # One rule for every share: the guest module asserts each tag is
      # [a-z0-9]+, which is what makes this case roundtrip exact. The two
      # reserved shares keep their historical socket names by construction:
      # @SHARE_SOCKET_STORE@ lands at store.sock as it always has.
      | gsub("@SHARE_SOCKET_(?<t>[A-Z0-9]+)@"; "\($runtime)/\(.t | ascii_downcase).sock")
      | gsub("@API_SOCKET@"; $api)
      | gsub("@CONSOLE_SOCKET@"; $console)
      | gsub("@CONTROL_SOCKET@"; $control)
    end;
  . as $c |
  {
    schemaVersion: .schemaVersion,
    sandboxId: $sandbox,
    target: $target,
    runtimePaths: { root: $runtime, launchSpec: ($runtime + "/launch.json"), lock: ($runtime + "/lock"),
      readySocket: $ready,
      apiSocket: $api, consoleSocket: $console, consoleLog: ($runtime + "/console.log"), controlSocket: $control,
      vmPid: ($runtime + "/vm.pid"), bootJson: ($runtime + "/boot.json"), vmCreateJson: ($runtime + "/vm-create.json") },
    backendPrograms: { cloudHypervisor: .cloudHypervisor, chRemote: .chRemote,
      virtiofsd: .virtiofsd, setpriv: .setpriv, truncate: .truncate,
      mkfsExt4: .mkfsExt4, systemdRun: .systemdRun, supervisor: $supervisor,
      unshare: .unshare, nsenter: .nsenter, ip: .ip, nft: .nft,
      pasta: .pasta, sleep: .sleep },
    egress: .egress,
    network: .network,
    socketLegs: { api: $api, console: $console, credentials: [.credentialIds[] as $id |
      { id: $id, hostSocket: (if $id == "ssh" then $sshAgent else $gpgAgent end) }] },
    resources: { vcpus: $vcpu, memoryMiB: $memoryMiB, cpuWeight: 100 },
    descriptorBudget: .descriptorBudget,
    identityTranslation: (.idTranslation + { hostUid: $uid, hostGid: $gid }),
    guestSession: .guestSession,
    shares: [.shareLaunch[] | { tag,
      source: (if .origin == "declared" then $mountSources[.tag].source
        else (.sourceToken | token) end),
      mountPoint,
      socket: (.socketToken | token), cache, readOnly,
      # `target` comes from the contract rather than from the argument group: the
      # bind table is build output, and it is what tells a mirrored tree from any
      # other mount (ADR-0110). The kind and the entry are the launch half, which
      # `viv` resolved against this host.
      mountPlan: (if .origin == "declared"
        then { kind: $mountSources[.tag].kind, entry: $mountSources[.tag].entry, target: .target }
        else null end),
      extraArgs: [.extraArgs[]? | select(. != "@UID_TRANSLATION@" and . != "@GID_TRANSLATION@") | token] }],
    volumes: [.volumeLaunch[] | { label, path: (.imagePath | token), sizeMiB, imageType, inodeRatio }],
    vmCreate: (.vmCreate | walk(token)
      | .cpus.boot_vcpus = $vcpu | .cpus.max_vcpus = $vcpu
      | .memory.size = ($memoryMiB * 1048576)
      | .landlock_enable = $landlock),
    landlockAvailable: $landlock,
    consoleLogEnabled: $consoleLog
  }' "$contract" >"$output"
$print_only && exit 0
# The runner's job ends here (ADR-0102): the invoking `viv` reads the rendered
# specification back and launches from the running installation.
chmod 0600 "$spec"
