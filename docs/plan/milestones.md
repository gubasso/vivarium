# Milestones

This is the single status surface. Each line follows `<id> <slug> — <status> — <appetite>[ — <note>]`. Status is one of `shaped`, `active`, `later`, `done`, `cut`, or `reshaped`; `later` means shaped and funded but sequenced behind the chain in flight, a `cut` note names what was cut, and a `reshaped` note names the successor id.

## in flight

- 005 cli-runtime-plumbing — shaped — 2 sessions — next to start

## later

- 006 generate-config-contract — later — 2 sessions
- 007 build-test-lanes — later — 4 sessions
- 008 enforce-implementation-status — later — 2 sessions
- 009 automate-backend-advisories — later — 2 sessions

## closed

- 001 close-host-measurement-gaps — done — 3 sessions
- 002 secure-launch-and-supervision — done — 4 sessions
- 003 guest-agent-and-credential-relay — done — 3 sessions
- 010 repair-the-host-runbooks — done — 3 sessions
- 011 resolve-and-evaluate — done — 3 sessions — appetite exceeded; see the slice `Revisions`
- 012 first-boot — done — 3 sessions — the guest module had to be composed into the generated flake; see the slice `Revisions`
- 013 exec-and-shell — done — 3 sessions — the launch specification had to carry the guest cwd, the startup lock, and the guest process environment; see the slice `Revisions`
- 004 enforce-egress-allowlist — done — 4 sessions — sessions 2 through 4 ran as one merged pass under the appetite; see the slice `Revisions`
- 014 workspace-and-persistence — done — 2 sessions — closed out of order: the coding-agent clause needed the guest connectivity slice 004 delivers, so the run that closed it was made after 004; see the slice `Revisions`
- 015 the-binary-supplies-itself — done — 3 sessions — enacted `ADR-0102` in one merged pass under the appetite; the host sweep and its measurement are in the slice `Revisions`
- 016 the-human-face — done — 3 sessions — enacted `ADR-0103` in one merged pass under the appetite; the acceptance rescope is in the slice `Revisions`
- 017 doctor-catalog — done — 4 sessions — the whole spec/13 catalog in one merged pass under the appetite; the host evidence is in the slice `Revisions`
