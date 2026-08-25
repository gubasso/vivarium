# macOS

Record whether the tool runs on macOS, and what boundary it provides there.[^read]

1. On macOS, follow the documented install.
2. Run the default invocation and record the boundary reported inside.

## vivarium

No: the tool targets a Linux host with KVM. The specification takes no position on other hosts, so this is an open gap rather than a refusal.

## glaipnir

Container only: `_macos_adjust_microvm` sets `USE_MICROVM=0` on Darwin, treating Podman Machine's VM as the boundary. The Linux egress guarantee costs a LaunchAgent, an SSH channel into the machine VM, and an nftables ruleset there.

## bunkerbox

No: `bunkerbox setup` refuses anything but Ubuntu 22.04 or 24.04 on `x86_64`, and what it installs — containerd, CNI plugins, and a Kata shim over `/dev/kvm` — is Linux either way.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
