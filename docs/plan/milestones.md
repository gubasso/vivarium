# Milestones

This is the single status surface. Each line follows `<id> <slug> — <status> — <appetite>[ — <note>]`. Status is one of `shaped`, `active`, `later`, `done`, `cut`, or `reshaped`; `later` means shaped and funded but sequenced behind the chain in flight, a `cut` note names what was cut, and a `reshaped` note names the successor id.

## in flight

## later

- 032 a-build-roots-itself — later — 3 sessions — second of that chain; moved out of the decided-not-funded phase by slice 015's measurement that the current build was reachable from no root, and by the hand-added gcroot it found beside it
- 025 the-fleet-is-visible — later — 3 sessions — needs 021 (done; the rekey every row carries)
- 027 the-stop-ladder-is-whole — later — 3 sessions — needs 025 (the reader the sweep enumerates through); carries `Q-018` and `Q-021`, and was moved ahead of 026 because ending a day is a daily act and refusing a launch is not
- 026 a-start-checks-the-room — later — 3 sessions — needs 025 (the fleet term admission reads)
- 018 the-user-owns-the-image — later — 4 sessions — moved ahead of 028 and 024: it owns `viv update` and the shipped base a user takes over, which is what a first daily-driver image is built from
- 028 memory-comes-back-without-a-stop — later — 4 sessions — needs 025 (the readings its record joins)
- 024 installing-vivarium-takes-one-command — later — 5 sessions
- 023 a-declared-port-crosses-inward — later — 3 sessions — deprioritized 2026-08-25 by the owner, whose sandbox use does not involve reaching a service from the host; the slice stays shaped and funded, and `Q-033` stays open until it closes
- 005 cli-runtime-plumbing — later — 2 sessions
- 006 generate-config-contract — later — 2 sessions
- 007 build-test-lanes — later — 4 sessions
- 008 enforce-implementation-status — later — 2 sessions
- 009 automate-backend-advisories — later — 2 sessions

## closed

- 031 a-declared-agent-channel-reaches-the-guest — done — 2 sessions — one pass under the appetite; the option-surface move, the two-site refusal, the host evidence, and the one cut (`gpg` end-to-end trial) are in the slice `Revisions`

- 030 the-base-image-is-the-base — done — 5 sessions — one pass under the appetite; `Q-030` proved to name four dead lanes rather than one, and the three wrong-assumption checks that surfaced are in the slice `Revisions`

- 029 a-workspace-is-just-a-mount — done — 4 sessions — one pass under the appetite; the host evidence, the three unplanned moves, and the `Q-034` regression are in the slice `Revisions`

- 021 the-manifest-is-the-sandbox — done — 4 sessions — phases 4 through 7 ran as one merged pass under the appetite, remainder included; the host evidence, gate timings, and deviations are in the slice `Revisions`

- 020 many-workspaces-in-one-sandbox — done — 3 sessions — one merged pass under the appetite, remainder included; the host evidence, fixture repair, and timings are in the slice `Revisions`

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
- 019 declared-mounts-reach-the-guest — done — 3 sessions — one merged pass under the appetite, remainder included; the descriptor-read measurement and the file-mount design are in the slice `Revisions`
- 022 a-file-mount-serves-only-its-file — done — 2 sessions — one merged pass under the appetite; the acceptance reshape and the measurements are in the slice `Revisions`
