# Install

Record what a user must already have before the tool can be installed.[^read]

1. On a machine with the tool absent, follow its documented install.
2. Record every prerequisite and every command.

## vivarium

No: installation is Nix-native — the user needs Nix and a host with KVM. flake-pilot and glaipnir install with one `zypper` or `apt` line. An open gap: nothing forecloses distribution packaging, and none exists.

## bunkerbox

No, and it is the only other column here that also says no. The documented path is `make dev` from a clone, then `bunkerbox setup`, which refuses anything but Ubuntu 22.04 or 24.04 on `x86_64`, apt-installs containerd, CNI plugins, podman, and zstd, and then symlinks a Kata shim and its configuration into `/usr/local/bin` and `/etc/kata-containers` — expecting the Kata install to be present already. Everything it does uses `sudo`, as does every later run: `ctr`, `iptables`, `mount`, and `systemctl` are all invoked that way. Packaging is described as the destination rather than the state, and it is the packaged form that would make [the native-command row](./native-command.md#bunkerbox) ordinary.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
