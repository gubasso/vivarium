# Security policy

## Reporting a vulnerability

Report privately, through this repository's own private vulnerability-reporting channel: open the repository's **Security** tab and use **Report a vulnerability**. That form is private to you and the maintainers, it needs no prior contact, and it is the only reporting route this policy recognizes — so the acknowledgement window below starts when you submit it. Do not open a public issue for a suspected vulnerability, and do not report one by any other channel expecting this policy's timelines to apply.

A useful report says what an attacker gains, which version or revision you observed it on, and how to reproduce it. If you are unsure whether something is a vulnerability, report it anyway — triage is our job, not yours.

We acknowledge a report within two business days and tell you whether it is in scope, what we believe the impact is, and when to expect a fix.

## Supported versions

The latest release. vivarium is before 1.0, and there is no backport stream — a fix ships in a new release, never as a patch to an older one. What the CLI surface promises between releases is in [`docs/reference/implementation-status.md`](docs/reference/implementation-status.md).

## What is in scope

vivarium's product claim is a **separate-kernel boundary**: a workspace runs behind a hardware-virtualization boundary with its own guest kernel (N1, [`docs/reference/spec/08-invariants-and-guarantees.md`](docs/reference/spec/08-invariants-and-guarantees.md)). The invariants on that page are the contract, so a defect that breaks one of them is a vulnerability. In particular:

- a guest reaching the host outside its sanctioned resources (N1, N20);
- a secret reaching the Nix store or a diagnostic surface (N10, and the redaction contract in [`docs/reference/spec/16-logging-and-diagnostics.md`](docs/reference/spec/16-logging-and-diagnostics.md));
- a host path or personal value entering a build input (N5, N11, N19);
- vivarium writing outside the roots it declares, or touching a project's own files (N9, N12, N13).

Out of scope: what a user deliberately puts inside their own guest, and the guest's own isolation from itself.

## The backend, and why a fix is a released pin move

The hypervisor, the filesystem daemon, and the guest kernel are **not host packages**. They are members of the closure a project builds, pinned by that project's lockfile ([`docs/decisions/ADR-0049-backend-is-a-closure-member.md`](docs/decisions/ADR-0049-backend-is-a-closure-member.md)). A host's package manager therefore cannot fix them, and neither can a vivarium release on its own.

A maintainer role — backend security owner — watches upstream releases and advisories for those closure members. vivarium is maintained by one person, so that role has **no backup**: the windows below are targets kept with one person's time, and an advisory that misses one says so. Two clocks run, and they are deliberately separate:

- **Routine currency** is scheduled. A periodic job moves vivarium's own pins and opens them for review, advisory scanning runs in CI, and the built runner's closure is scanned once a build exists. None of this is user-visible.
- **Advisory response** is triggered. When an advisory affects a closure member, we publish a release whose only content is the moved pin.

Response windows, measured from public disclosure — vivarium is not party to any upstream embargo, so our clock starts when yours does:

| Trigger                                                                      | Target release window |
| ---------------------------------------------------------------------------- | --------------------- |
| A flaw crossing the guest-to-host boundary, or one under active exploitation | 3 business days       |
| Another severe flaw                                                          | 14 days               |
| Everything else                                                              | next routine release  |

The trigger is the **boundary**, not a severity number. A flaw whose impact stays inside the guest is a different kind of thing from one that reaches the host, and scores routinely disagree with that distinction. A numeric severity is a secondary signal. We never publish a routine release that knowingly retains an applicable vulnerability, and if no fixed upstream revision exists we publish the exposure and any mitigation, and track it until it closes.

## What our advisories oblige you to do

A project's pins move **only** when that project runs `viv update` ([`docs/reference/spec/02-config-and-xdg-layout.md`](docs/reference/spec/02-config-and-xdg-layout.md)). Nothing moves them in the background — that is a deliberate guarantee (N3), and its cost is that a published fix reaches you only when you act.

So every vivarium advisory names three things:

1. the affected closure member,
2. the minimum input revision that carries the fix,
3. the command that adopts it.

And it names one asymmetry. If your team uses a **shared override lock** beside its manifest, `viv update` refuses and writes nothing — including for a security update, because urgency does not make an unannounced pin move safe ([`docs/decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md`](docs/decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)). In that case the remedy is outside vivarium: whoever maintains the shared lock moves the pin, and you rebuild against it.

The process is decided in [`docs/decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](docs/decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md), amended by [`docs/decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md`](docs/decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) for the single-maintainer case. Who currently holds the backend security owner role is recorded with the release process rather than here — see [`docs/PUBLISHING.md`](docs/PUBLISHING.md) — so a rotation does not require rewriting a decision.
