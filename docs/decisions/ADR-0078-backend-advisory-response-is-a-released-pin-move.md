# ADR-0078: Backend advisory response is a released pin move

## Context and Problem Statement

The hypervisor, filesystem daemon, and guest kernel are closure members pinned by the lockfile ([`ADR-0049`](./ADR-0049-backend-is-a-closure-member.md)), not host packages, so a host's package manager cannot fix them. That ADR named the cost and left the process open: nothing records who watches upstream advisories, on what cadence, or what an advisory obliges.

## Considered Options

- Move the pin automatically when an advisory lands
- A named owner, plus separate scheduled and triggered clocks
- An online `viv doctor` vulnerability-feed check

## Decision Outcome

Chosen option: **a named owner and two separate clocks** — conflating them is the failure this prevents.

A maintainer role owns backend security, with a named backup; who holds it is recorded in the release process, so rotation costs no rewrite. Reports arrive through the repository's private reporting channel.

**Routine currency** is scheduled and invisible to users: a periodic job moves vivarium's own pins for review, advisory scanning runs in CI, and the built runner's closure is scanned once it exists.

**Advisory response** is triggered: an advisory affecting a closure member obliges a release whose only content is the moved pin. A flaw crossing the guest-to-host boundary, or one under active exploitation, ships within three business days; another severe flaw within fourteen; the rest with the next routine release. Numeric severity is secondary; the boundary is the product claim.

Because a pin moves only under `viv update`, a fix reaches a user only when that user runs it. Every advisory therefore names the affected closure member, the minimum revision carrying the fix, and the command — plus the asymmetry: under a team override lock `viv update` refuses, so the remedy is the team's.

Automatic movement was rejected as the unannounced input jump N3 forbids; a feed check because `doctor` probes host conditions.

## Consequences

- Good: a user reading an advisory learns the exact command; a team under a shared lock learns it is not that command.
- Bad: remediation is opt-in, so a fix reaches nobody who does not update.
- Bad: the response window is a commitment kept with volunteer time.

## Status

Accepted

Specified in [`../../SECURITY.md`](../../SECURITY.md) and [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md).
