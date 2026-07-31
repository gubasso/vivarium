{ pkgs, launchArguments }:

let
  launchArgumentsFile = pkgs.writeText "vivarium-first-microvm-launch-arguments.json" (
    builtins.toJSON launchArguments
  );
in
pkgs.writeShellApplication {
  name = "vivarium-first-microvm";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.jq
  ];
  text =
    builtins.replaceStrings
      [ "@launchArgumentsPath@" ]
      [
        (toString launchArgumentsFile)
      ]
      ''
        set -euo pipefail

        json_file=@launchArgumentsPath@
        cloud_hypervisor=$(jq -r .cloudHypervisor "$json_file")
        virtiofsd=$(jq -r .virtiofsd "$json_file")
        setpriv=$(jq -r .setpriv "$json_file")
        truncate_bin=$(jq -r .truncate "$json_file")
        mkfs_ext4=$(jq -r .mkfsExt4 "$json_file")
        volume_label=$(jq -r .volumeLabel "$json_file")
        volume_size_mib=$(jq -r .volumeSizeMiB "$json_file")

        workspace=
        runtime_dir=
        volume=
        uid=
        gid=
        memory_mib=
        vcpu=
        print_only=false
        landlock=true
        usage() {
          echo "usage: $0 --workspace ABS --runtime-dir ABS --volume ABS --uid N --gid N --memory-mib N --vcpu N [--no-landlock] [--print-static-arguments]" >&2
          exit 64
        }
        while (($#)); do
          case $1 in
            --workspace|--runtime-dir|--volume|--uid|--gid|--memory-mib|--vcpu)
              (($# >= 2)) || usage
              name=''${1#--}; name=''${name//-/_}; printf -v "$name" '%s' "$2"; shift 2 ;;
            --print-static-arguments) print_only=true; shift ;;
            --no-landlock) landlock=false; shift ;;
            *) usage ;;
          esac
        done
        for value in workspace runtime_dir volume; do
          candidate=''${!value:-}
          [[ $candidate = /* ]] || { echo "$value must be an absolute path" >&2; exit 64; }
        done
        for value in uid gid memory_mib vcpu; do
          [[ ''${!value:-} =~ ^[0-9]+$ ]] || { echo "$value must be an unsigned integer" >&2; exit 64; }
        done
        workspace=$(realpath -m -- "$workspace")
        runtime_dir=$(realpath -m -- "$runtime_dir")
        volume=$(realpath -m -- "$volume")
        is_same_or_below() { [[ $1 == "$2" || $1 == "$2"/* ]]; }
        # N24 forbids a mount source that resolves to /tmp, /var/tmp or the
        # runtime dir *or to any ancestor of them* — mounting / exposes strictly
        # more than mounting /tmp does, so both directions must be rejected.
        violates_n24() {
          local candidate=$1 forbidden
          for forbidden in /tmp /var/tmp "$runtime_dir"; do
            is_same_or_below "$candidate" "$forbidden" && return 0
            is_same_or_below "$forbidden" "$candidate" && return 0
          done
          return 1
        }
        if violates_n24 "$workspace"; then
          echo "workspace violates N24" >&2
          exit 64
        fi
        if violates_n24 "$volume"; then
          echo "volume violates N24" >&2
          exit 64
        fi

        store_socket=$runtime_dir/store.sock
        workspace_socket=$runtime_dir/workspace.sock
        api_socket=$runtime_dir/api.sock
        console_socket=$runtime_dir/console.sock
        mapfile -t argv < <(jq -r '.staticArguments[]' "$json_file")
        for index in "''${!argv[@]}"; do
          argv[index]=''${argv[index]//@VCPU@/$vcpu}
          argv[index]=''${argv[index]//@MEMORY_MIB@/$memory_mib}
          argv[index]=''${argv[index]//@VOLUME_IMAGE@/$volume}
          argv[index]=''${argv[index]//@STORE_SOCKET@/$store_socket}
          argv[index]=''${argv[index]//@WORKSPACE_SOCKET@/$workspace_socket}
          argv[index]=''${argv[index]//@API_SOCKET@/$api_socket}
          argv[index]=''${argv[index]//@CONSOLE_SOCKET@/$console_socket}
        done
        if $landlock; then argv+=(--landlock); fi
        if $print_only; then
          printf '%q\n' "$cloud_hypervisor" "''${argv[@]}"
          exit 0
        fi

        umask 077
        [[ -d $workspace && -w $workspace ]] || { echo "workspace must be a writable directory" >&2; exit 66; }
        mkdir -p "$runtime_dir"
        if [[ ! -e $volume ]]; then
          "$truncate_bin" -s "''${volume_size_mib}M" "$volume"
          "$mkfs_ext4" -q -L "$volume_label" "$volume"
        fi
        [[ -f $volume ]] || { echo "volume must be a regular file" >&2; exit 66; }

        pids=()
        cleanup() {
          local pid
          for pid in "''${pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
          wait "''${pids[@]:-}" 2>/dev/null || true
          rm -f "$store_socket" "$workspace_socket" "$api_socket" "$console_socket"
        }
        trap cleanup EXIT INT TERM
        ulimit -n "$(ulimit -Hn)"
        thread_pool_size=$(jq -r .virtiofsdThreadPoolSize "$json_file")
        inode_file_handles=$(jq -r .virtiofsdInodeFileHandles "$json_file")
        # Every per-share parameter below comes from the guest module's own
        # `microvm.shares` declaration via launch-arguments.nix. Only the socket
        # path, the workspace source and the UID/GID are launch-channel (N5/N19).
        resolve_token() {
          local value=$1
          value=''${value//@STORE_SOCKET@/$store_socket}
          value=''${value//@WORKSPACE_SOCKET@/$workspace_socket}
          value=''${value//@WORKSPACE_SOURCE@/$workspace}
          value=''${value//@UID@/$uid}
          value=''${value//@GID@/$gid}
          printf '%s' "$value"
        }
        guest_uid=$(jq -r .idTranslation.guestUid "$json_file")
        guest_gid=$(jq -r .idTranslation.guestGid "$json_file")
        overflow_uid=$(jq -r .idTranslation.overflowUid "$json_file")
        overflow_gid=$(jq -r .idTranslation.overflowGid "$json_file")
        id_max=$(jq -r .idTranslation.idMax "$json_file")
        # spec/06: "Guest identities outside the map are forbidden rather than
        # passed through" and host identities outside it "appear under the
        # conventional overflow identity, visible and inert". virtiofsd maps an
        # unmapped id to itself, so the single `map:` range alone would silently
        # pass every other identity through in both directions. One line per
        # emitted argument so the caller can read them into an array.
        id_translation() {
          local flag=$1 guest_id=$2 host_id=$3 overflow=$4
          local -a out=("$flag" "map:$guest_id:$host_id:1")
          if ((guest_id > 0)); then out+=("$flag" "forbid-guest:0:$guest_id"); fi
          if ((guest_id < id_max)); then
            out+=("$flag" "forbid-guest:$((guest_id + 1)):$((id_max - guest_id))")
          fi
          if ((host_id > 0)); then out+=("$flag" "squash-host:0:$overflow:$host_id"); fi
          if ((host_id < id_max)); then
            out+=("$flag" "squash-host:$((host_id + 1)):$overflow:$((id_max - host_id))")
          fi
          printf '%s\n' "''${out[@]}"
        }
        share_count=$(jq -r '.shareLaunch | length' "$json_file")
        for share_index in $(seq 0 $((share_count - 1))); do
          share=$(jq -c ".shareLaunch[$share_index]" "$json_file")
          socket=$(resolve_token "$(jq -r .socketToken <<<"$share")")
          source=$(resolve_token "$(jq -r .sourceToken <<<"$share")")
          cache=$(jq -r .cache <<<"$share")
          readonly_share=$(jq -r .readOnly <<<"$share")
          args=(--socket-path "$socket" --shared-dir "$source" --sandbox namespace --seccomp kill
            --inode-file-handles="$inode_file_handles" --thread-pool-size "$thread_pool_size" --cache "$cache")
          [[ $readonly_share = true ]] && args+=(--readonly)
          while IFS= read -r extra; do
            [[ -n $extra ]] || continue
            case $extra in
              @UID_TRANSLATION@)
                mapfile -t translation < <(id_translation --translate-uid "$guest_uid" "$uid" "$overflow_uid")
                args+=("''${translation[@]}") ;;
              @GID_TRANSLATION@)
                mapfile -t translation < <(id_translation --translate-gid "$guest_gid" "$gid" "$overflow_gid")
                args+=("''${translation[@]}") ;;
              *) args+=("$(resolve_token "$extra")") ;;
            esac
          done < <(jq -r '.extraArgs[]?' <<<"$share")
          "$setpriv" --no-new-privs "$virtiofsd" "''${args[@]}" &
          pids+=("$!")
        done
        for socket in "$store_socket" "$workspace_socket"; do
          for _ in {1..200}; do [[ -S $socket ]] && break; sleep 0.05; done
          [[ -S $socket ]] || { echo "timed out waiting for $socket" >&2; exit 70; }
        done
        "$setpriv" --no-new-privs "$cloud_hypervisor" "''${argv[@]}" &
        pids+=("$!")
        wait "''${pids[-1]}"
      '';
  excludeShellChecks = [ "SC2016" ];
}
