# ADR-0049: The backend is a closure member, not a host dependency

## Context and Problem Statement

`spec/13` gated `viv start` on finding the hypervisor binary on `$PATH`, and warned when its version trailed a floor. But vivarium builds the VM with `nix build`, and the runner that build produces names the backend by absolute store path; `spec/01` already records the backend per generation, beside the generation's own lock record. The probe catalog and the build model disagreed about where the backend comes from.

## Considered Options

- **Host-provided** — resolved from `$PATH`, presence and version probed at runtime.
- **Closure-provided** — a member of the built runner's closure, pinned by the lockfile.
- Closure by default, with a host-provided binary substitutable per invocation.

## Decision Outcome

Chosen option: **closure-provided**.

- The hypervisor (`cloud-hypervisor`), its control client (`ch-remote`), and every host-side filesystem daemon (`virtiofsd`) are members of the built runner's closure, pinned by the project's lockfile and recorded in the generation record beside the lock that built it. None is ever resolved from `$PATH`.
- The rule this sets for the catalog: **`viv doctor` probes host conditions only; anything the lockfile determines is an evaluation-time assertion, not a check.** Nix stays probed — it genuinely is host-provided.
- Backend feature floors therefore become assertions that fail at evaluation with `65` ([`ADR-0042-evaluation-time-content-defects.md`](./ADR-0042-evaluation-time-content-defects.md)): free page reporting on the balloon device, Landlock in the hardened profile, and block-device discard with sparse images. The recommended version in `spec/13` was always a tested baseline, never the first release carrying what vivarium needs.
- **The default still names a backend, never the contract.** Pinning one hypervisor into the closure does not narrow N2: the isolation class stays backend-agnostic, and substituting a backend becomes a Nix option rather than a `$PATH` lookup.

## Consequences

- Good: five runtime probes disappear, and with them a hard preflight gate that could never pass meaningfully; version skew stops being a user-visible failure.
- Bad: the closure grows, and a backend security fix arrives by updating the lock rather than from the host's package manager.

## Status

Accepted

Amended by [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — a generation retains a snapshot of the whole lockfile it was built against plus its digest, replacing the bare lock revision this ADR named. The backend is still pinned by that lock and identified through the generation record; only the record's shape changes.

Amended by [`ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — the Bad consequence here, that a backend security fix arrives by updating the lock rather than from the host's package manager, now has a named owner, a cadence, and a stated obligation on what a published advisory must say.

Amends [`ADR-0023-doctor-check-catalog-and-contract.md`](./ADR-0023-doctor-check-catalog-and-contract.md) with a membership criterion the catalog contract lacked, and amends ADR-0025, ADR-0035, ADR-0037, ADR-0039, ADR-0041, and ADR-0042 where each named a probe this rule retires. Applied in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md), [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md), and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
