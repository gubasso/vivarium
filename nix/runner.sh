# shellcheck shell=bash
set -euo pipefail
contract=@launchArgumentsPath@
workspace=""
runtime_dir=""
volume_dir=""
uid=""
gid=""
memory_mib=""
vcpu=""
project_id=first-microvm
target=default
ssh_agent_socket=""
gpg_agent_socket=""
console_log=true
print_only=false
usage() {
  echo "usage: $0 --workspace ABS --runtime-dir ABS --volume-dir ABS --uid N --gid N --memory-mib N --vcpu N [--project-id ID] [--target NAME] [--ssh-agent-socket ABS] [--gpg-agent-socket ABS] [--no-console-log] [--print-static-arguments]" >&2
  # The one pairing `schemaVersion` cannot catch. A generated flake pins its own
  # `vivarium`, so a newer `viv` may drive an older runner, whose argument names
  # differ — and this refusal happens before any schema is read. `viv start`
  # surfaces this stderr verbatim, so naming the contract's version here is what
  # turns "usage error" into "you are holding two halves of different versions".
  echo "this launcher speaks launch contract schema $(jq -r '.schemaVersion' "$contract")" >&2
  exit 64
}
while (($#)); do
  case $1 in
    --workspace | --runtime-dir | --volume-dir | --uid | --gid | --memory-mib | --vcpu | --project-id | --target | --ssh-agent-socket | --gpg-agent-socket)
      (($# >= 2)) || usage
      name=${1#--}
      name=${name//-/_}
      printf -v "$name" '%s' "$2"
      shift 2
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
for value in workspace runtime_dir volume_dir; do
  candidate=${!value:-}
  [[ $candidate = /* ]] || usage
done
for value in uid gid memory_mib vcpu; do [[ ${!value:-} =~ ^[0-9]+$ ]] || usage; done
[[ -n $project_id && -n $target ]] || usage
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
workspace=$(realpath -m -- "$workspace")
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
  --arg workspace "$workspace" --arg volumeDir "$volume_dir" \
  --arg api "$api" --arg console "$console" --arg control "$control" --arg ready "$ready" \
  --arg storeSocket "$store_socket" --arg workspaceSocket "$workspace_socket" \
  --arg sshAgent "$ssh_agent_socket" --arg gpgAgent "$gpg_agent_socket" --argjson uid "$uid" --argjson gid "$gid" \
  --argjson memoryMiB "$memory_mib" --argjson vcpu "$vcpu" \
  --argjson landlock "$landlock" --argjson consoleLog "$console_log" '
  def token:
    if type != "string" then . else
      gsub("@WORKSPACE_SOURCE@"; $workspace)
      | gsub("@VOLUME_DIR@"; $volumeDir)
      | gsub("@STORE_SOCKET@"; $storeSocket)
      | gsub("@WORKSPACE_SOCKET@"; $workspaceSocket)
      | gsub("@API_SOCKET@"; $api)
      | gsub("@CONSOLE_SOCKET@"; $console)
      | gsub("@CONTROL_SOCKET@"; $control)
    end;
  . as $c |
  {
    schemaVersion: .schemaVersion,
    projectId: $project,
    target: $target,
    runtimePaths: { root: $runtime, launchSpec: ($runtime + "/launch.json"), lock: ($runtime + "/lock"),
      readySocket: $ready,
      apiSocket: $api, consoleSocket: $console, consoleLog: ($runtime + "/console.log"), controlSocket: $control,
      vmPid: ($runtime + "/vm.pid"), bootJson: ($runtime + "/boot.json"), vmCreateJson: ($runtime + "/vm-create.json") },
    backendPrograms: { cloudHypervisor: .cloudHypervisor, chRemote: .chRemote,
      virtiofsd: .virtiofsd, setpriv: .setpriv, truncate: .truncate,
      mkfsExt4: .mkfsExt4, systemdRun: .systemdRun, supervisor: .supervisor,
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
    shares: [.shareLaunch[] | { tag, source: (.sourceToken | token), mountPoint,
      socket: (.socketToken | token), cache,
      readOnly, extraArgs: [.extraArgs[]? | select(. != "@UID_TRANSLATION@" and . != "@GID_TRANSLATION@") | token] }],
    volumes: [.volumeLaunch[] | { label, path: (.imagePath | token), sizeMiB, imageType, inodeRatio }],
    vmCreate: (.vmCreate | walk(token)
      | .cpus.boot_vcpus = $vcpu | .cpus.max_vcpus = $vcpu
      | .memory.size = ($memoryMiB * 1048576)
      | .landlock_enable = $landlock),
    landlockAvailable: $landlock,
    consoleLogEnabled: $consoleLog
  }' "$contract" >"$output"
$print_only && exit 0
chmod 0600 "$spec"
exec @vivPath@ start --spec "$spec"
