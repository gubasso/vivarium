# ADR-0099: The capability bounding-set drop is best effort

## Context and Problem Statement

[`ADR-0027`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md), as amended by [`ADR-0049`](./ADR-0049-backend-is-a-closure-member.md), says the launch wrapper "execs unprivileged with an empty bounding and ambient set". Emptying the bounding set is `PR_CAPBSET_DROP`, which the kernel permits only to a process holding `CAP_SETPCAP`. An unprivileged launcher holds no such capability, and `setpriv` treats the refusal as fatal: it reports `apply bounding set: Operation not permitted`, exits `127`, and never execs the child. Both requirements cannot hold, so as written the profile does not harden a launch — it prevents one. Booting a guest through the launcher rather than through a fake is what found it.

## Considered Options

- Render the bounding-set drop only when the launcher holds `CAP_SETPCAP`.
- Grant the supervisor `CAP_SETPCAP` through an ambient or file capability.
- Keep the profile and require a privileged launcher.

## Decision Outcome

Chosen option: `Render the drop only when the launcher holds CAP_SETPCAP` — it is a host capability, like Landlock availability, rather than a policy switch.

The launcher reads its own effective set from `/proc/self/status` and adds `--bounding-set=-all` only when the capability is present; an unreadable status answers no. The other three legs — `--no-new-privs`, `--ambient-caps=-all`, `--inh-caps=-all` — need no privilege and stay mandatory in both renderings. The rendered-command check is told which shape to expect, so a rendering that silently lost the flag still fails.

Why the fallback is safe: with `PR_SET_NO_NEW_PRIVS` set, no descendant gains privilege through a `setuid` or file-capability exec, and the launcher's permitted and effective sets are already empty. The bounding set therefore bounds capabilities no descendant could acquire.

## Consequences

- Good: an unprivileged launcher can boot a guest, which is what [`ADR-0027`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)'s own unprivileged-execution clause requires.
- Good: a launcher holding the capability still empties the bounding set, so nothing is lost where it was achievable.
- Bad: the profile has two renderings, so a reader must consult the host to know which ran.
- Bad: defence in depth is weaker if the launcher is later granted capabilities, because the drop is conditional rather than asserted.

## Status

Accepted

Amends [`ADR-0027`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — the bounding-set drop is best effort rather than unconditional.
