# Security policy

## Reporting a vulnerability

Report privately through this repository's Security tab with Report a vulnerability. The form is private to you and the maintainers, needs no prior contact, and is the only reporting route this policy recognizes. Do not open a public issue.

A useful report says what an attacker gains, which version or revision you observed, and how to reproduce it. If uncertain, report it for triage.

We acknowledge a report within two business days and state whether it is in scope, the believed impact, and when to expect a fix.

## Supported versions

Only the latest release is supported. Before 1.0 there is no backport stream; fixes ship in new releases. [Implementation status](docs/reference/implementation-status.md) owns what the CLI currently promises.

## What is in scope

vivarium's product claim is a separate-kernel boundary. The [normative invariants](docs/reference/spec/08-invariants-and-guarantees.md) are the security contract. Defects that break them include:

- a guest reaching the host outside sanctioned resources (N1, N20);
- a secret reaching the Nix store or a diagnostic surface (N10);
- a host path or personal value entering a build input (N5, N11, N19);
- vivarium writing outside declared roots or modifying project-authored files (N9, N12, N13).

What a user deliberately puts inside a guest, and the guest's isolation from itself, are out of scope.

## The backend and released pin moves

The hypervisor, filesystem daemon, and guest kernel are closure members pinned by a project's lockfile, not host packages. [ADR-0049](docs/decisions/ADR-0049-backend-is-a-closure-member.md) records the rationale. A host package update therefore cannot fix them.

The backend security owner watches upstream releases and advisories. This single-maintainer project currently has no backup, so the windows below are targets:

- Routine currency is manual today. `cargo audit` and `cargo deny check advisories` run as pre-push hooks in this repository; nothing schedules pin moves, and no scan of the built runner closure exists. Scheduling all three is the decided model and not yet the operating one; [slice 009](docs/plan/slices/009-automate-backend-advisories/README.md) tracks the remaining work.
- Advisory response is triggered: an applicable advisory leads to a release carrying the moved pin.

Windows are measured from public disclosure. vivarium is party to no upstream embargo, so its clock starts when the reporter's does.

| Trigger                                            | Target release window |
| -------------------------------------------------- | --------------------- |
| Guest-to-host boundary flaw or active exploitation | 3 business days       |
| Another severe flaw                                | 14 days               |
| Everything else                                    | next routine release  |

The boundary impact, not a numeric severity alone, selects the window. A routine release never knowingly retains an applicable vulnerability. If no fixed revision exists, the release reports exposure and mitigation and tracks the issue until closure.

## What an advisory requires from users

Project pins move only when that project runs `viv update`; nothing updates them in the background. Every advisory therefore names the affected closure member, the minimum fixed revision, and the command that adopts it.

If a team uses a shared override lock, `viv update` refuses and writes nothing. The shared-lock maintainer must move the pin and rebuild. [ADR-0062](docs/decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md) owns that constraint.

[ADR-0078](docs/decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) records the response model and [ADR-0079](docs/decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) records the single-maintainer amendment. The [publishing guide](docs/guides/publishing.md) owns the current role assignment.
