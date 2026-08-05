# ADR-0055: The runtime directory is required, not synthesized

## Context and Problem Statement

vivarium puts every running VM's per-target files — the `flock`, `control.sock`, `vm.pid`, `boot.json` — under `$XDG_RUNTIME_DIR` ([`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md)). Config, data, state, and cache each have a literal `$HOME` default; the runtime root has none, because it is a session-scoped, `0700`, tmpfs-backed directory a login manager creates and removes. [`ADR-0005-xdg-user-config-layout.md`](./ADR-0005-xdg-user-config-layout.md) named the exposure without saying what vivarium does when the variable is unset or hostile.

## Considered Options

- A fallback root under the state root, keyed by boot id.
- `/tmp/vivarium-$UID`, or a directory under the cache root.
- Hardcode `/run/user/$UID`, ignoring the variable.
- Require the variable, validate it, and fail closed.

## Decision Outcome

Chosen option: require it — a host without a usable runtime directory is one vivarium cannot serve, so refusing is more honest than synthesizing.

Unset, empty, non-absolute, or failing owner/mode validation is a hard preflight failure exiting `77`, naming the variable and the fault. There is no fallback root.

A fallback would not help. [`ADR-0036-host-resource-scoping-and-admission-control.md`](./ADR-0036-host-resource-scoping-and-admission-control.md) places every VM's processes in one transient systemd user scope, and that manager is reached over a socket under `$XDG_RUNTIME_DIR`; a host missing the directory is missing the manager, so N23's scoping is already unsatisfiable and a synthesized directory would only buy a launch that then violates an invariant. Hosts without one — non-init containers, CI runners, `cron`, `su` without a PAM session — also lack `/dev/kvm` or unprivileged user namespaces.

Hardcoding `/run/user/$UID` was rejected separately: ignoring a variable that is set is a worse deviation than failing when it is unset.

## Consequences

- Good: the root is `0700`, tmpfs-backed, and reboot-clean by construction — no sweeper, no stale-directory reaping, no boot-id keying.
- Good: the socket path stays short, leaving `<project-id>` a comfortable `sun_path` budget.
- Bad: deviates from the convention's fall-back-and-warn recommendation, so the diagnosis has to carry the remediation instead.
- Bad: refuses where ADR-0036 degraded for a missing delegated cgroup hierarchy — degradation had somewhere to fall back to; this does not.

## Status

Accepted

Amends [`ADR-0005-xdg-user-config-layout.md`](./ADR-0005-xdg-user-config-layout.md): the XDG reliance it records still holds, but is now a specified, validated precondition rather than an assumption. Root resolution and the failure live in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md); the check is `runtime-dir-usable` in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md). The session boundary this root inherits is [`ADR-0056-vm-lifetime-bounded-by-user-session.md`](./ADR-0056-vm-lifetime-bounded-by-user-session.md).
