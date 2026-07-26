# 13 — Doctor and health checks

The contract for `viv doctor`: the shared probe catalog, severity and status model, flags, output
shapes, and exit codes. The governing decisions are
[`../../decisions/ADR-0023-doctor-check-catalog-and-contract.md`](../../decisions/ADR-0023-doctor-check-catalog-and-contract.md)
(this contract),
[`../../decisions/ADR-0022-config-inspection-namespace.md`](../../decisions/ADR-0022-config-inspection-namespace.md)
(`doctor` is a pure checker — it never renders configuration; that is `viv config`'s job), and
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md)
(streams, `--json`, sysexits).

`viv doctor` diagnoses; it changes nothing. It is read-only, side-effect-free, and never modifies
project, config, or VM state. One probe catalog serves three call sites: `doctor` runs the whole
catalog, each command's preflight guard runs its hard subset before any side effect
([`10-vm-lifecycle.md`](10-vm-lifecycle.md)), and setup flows reuse individual probes — so the
checks can never drift apart.

## Severity, scope, and status

Each check carries three classifications:

- **Severity** — `hard` or `soft`. Hard checks form the **preflight subset**: a hard failure makes
  `viv start` (and a cold-starting `exec`/`shell`) refuse before any side effect. Soft checks only
  warn. There is no per-invocation waiver of a hard check — a check that could legitimately be
  ignored is soft by definition, so preflight remains an unconditional guarantee.
- **Scope** — `host` (always runnable), `project` (needs a bound manifest), or `network` (needs
  `--online`). Out-of-scope checks are **skipped**, never failed.
- **Status** (per run) — `pass`, `warn` (a soft check tripped), `fail` (a hard check tripped), or
  `skipped` (out of scope, with a reason). Skips never affect the exit code.

## Check catalog

Stable kebab-case ids; catalog order is cheapest-and-most-fundamental first, so the earliest
failure is the most actionable.

### Hard — host scope (the preflight subset)

| Id | Category | Passes when | Failure code |
| -- | -------- | ----------- | ------------ |
| `nix-present` | tooling | `nix` is on `$PATH` | `69` |
| `nix-version` | tooling | Nix meets the minimum version — the latest stable release at decision time (2.34); the implementation pins the exact floor | `69` |
| `nix-flakes-enabled` | tooling | `experimental-features` includes `nix-command flakes` | `78` |
| `kvm-device-present` | virtualization | `/dev/kvm` exists | `69` |
| `kvm-device-accessible` | permissions | the user can read and write `/dev/kvm` | `77` |
| `hardware-virt-available` | virtualization | CPU virtualization extensions are present and enabled | `69` |
| `backend-binary-present` | tooling | the hypervisor binary the launch will use — the selected backend, resolved to its binary name — is on `$PATH` | `69` |

`kvm-device-present` and `kvm-device-accessible` are split because remediation differs: enable
virtualization in firmware vs join the `kvm` group.

For the shipped default backend — Cloud Hypervisor, per
[`../../decisions/ADR-0025-default-hypervisor-cloud-hypervisor.md`](../../decisions/ADR-0025-default-hypervisor-cloud-hypervisor.md)
— `backend-binary-present` resolves to the `cloud-hypervisor` binary plus the `virtiofsd`
shared-filesystem daemon that serves the workspace share; the QEMU fallback resolves to the
arch-specific `qemu-system-<arch>` binary. The default names a backend, never the contract — the
isolation class stays backend-agnostic (N2).

### Soft — host scope

| Id | Category | Warns when |
| -- | -------- | ---------- |
| `nix-store-disk-space` | disk | less than 10 GB free on the store filesystem (roughly one build cycle plus headroom) |
| `backend-version` | tooling | the selected backend is older than the recommended version — for the default backend, the latest stable release at decision time (Cloud Hypervisor v53); the implementation pins the exact floor |
| `kernel-version-supported` | virtualization | the host kernel is too old for the required virtio features |
| `host-landlock-available` | virtualization | the kernel lacks Landlock — the VMM's seccomp + capability-drop sandbox (N20) still applies, but the launch profile's filesystem-path allowlist is skipped |
| `state-dir-writable` | permissions | the state root is not writable |
| `cache-dir-writable` | permissions | the cache root is not writable |
| `data-dir-writable` | permissions | the data root is not writable |
| `runtime-dir-writable` | permissions | `$XDG_RUNTIME_DIR` is not writable |

### Soft — project scope (skipped when no manifest is bound)

| Id | Category | Warns when |
| -- | -------- | ---------- |
| `config-parses` | config | the bound manifest does not parse. Parse only — `doctor` never evaluates or renders the merge; that is `viv config eval` |
| `manifest-resolves` | config | the binding does not resolve to exactly one defined manifest |

When no manifest is bound, `doctor` still runs every host-scope check, notes once on stderr that
project-scope checks are skipped (with `viv init` guidance), and marks each as
`skipped` / `no-manifest-bound`. All host checks passing still exits `0`.

### Soft — network scope (skipped unless `--online`)

| Id | Category | Warns when |
| -- | -------- | ---------- |
| `nix-version-currency` | network | the installed Nix trails the latest stable release |
| `backend-version-currency` | network | the selected backend trails its latest release |
| `substituter-reachability` | network | a configured substituter is unreachable |
| `egress-allowlist-dns` | network | an allowlist hostname does not resolve — probed only when the bound manifest sets `sandbox.egress.mode = "allowlist"` (also project scope) |

The default run is fully offline. Open egress is **not** a check and never renders as a finding —
it is policy, not a health defect (N8, [`05-networking-and-egress.md`](05-networking-and-egress.md)).

### Guaranteed by construction (not probed)

Two security properties are guarantees of how vivarium builds and launches, not host conditions a
probe could observe: the separate guest kernel (N1) and the host-side sandbox around the VMM and
every virtiofsd (N20). vivarium's launch wrapper enacts N20 as a fixed **hardened launch profile**,
so the confinement cannot be disabled at runtime
([`../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)):

- **VMM (Cloud Hypervisor).** Launched unprivileged with `PR_SET_NO_NEW_PRIVS`, capabilities dropped
  toward zero, built-in seccomp enabled (`--seccomp true`), a Landlock path allowlist where the
  kernel supports it, and cgroup limits.
- **virtiofsd (one process per share).** Launched unprivileged, `--sandbox=namespace`, seccomp on,
  `cache=none`, with only its own share writable — never as root and never `--sandbox none`, the
  configuration behind the guest-root-to-host-root escape CVE-2026-47243.
- **QEMU fallback is documentation-only** — admissible under N2 but not hardened; use Cloud
  Hypervisor for untrusted workloads.

These are guarantees of construction; `viv doctor` probes only the host *prerequisites* (the
soft-host checks, including `host-landlock-available`), never whether the sandbox "is on." See
[`08-invariants-and-guarantees.md`](08-invariants-and-guarantees.md) and
[`../../decisions/ADR-0024-backend-security-requirements.md`](../../decisions/ADR-0024-backend-security-requirements.md).

## Flags

```text
viv doctor [--json] [--strict] [--list] [--online]
```

- `--json` — one machine record on stdout (shape below).
- `-v` / `--verbose` — the **global** verbosity flag (see
  [`01-command-surface.md`](01-command-surface.md) "Global flags"); on `doctor` it surfaces deeper
  per-check detail (observed versions, paths, permissions, free bytes).
- `--strict` — any `warn` fails the run: exit `1` (the one sanctioned use of `1`, per the
  ADR-0023 amendment to ADR-0015). For CI gates.
- `--list` — enumerate the catalog (id, category, scope, severity, title) **without running any
  probe**; combines with `--json`.
- `--online` — enable the network-scope checks. There is no `--offline`; offline is the default.

Color and TTY behavior follow ADR-0015 (`NO_COLOR > FORCE_COLOR > isatty`); there is no
per-command color flag.

## Exit codes

| Outcome | Exit |
| ------- | ---- |
| No hard check failed (warns and skips allowed) | `0` |
| Hard failure(s) — first failing check in catalog order decides | that check's code: `69` unavailable, `77` permission, `78` config |
| `--strict` and at least one `warn` (no hard failure) | `1` |
| Internal error in `doctor` itself | `70` |

## Output

The report is the command's **result**: human report or `--json` record on stdout; progress and
the skip note on stderr, so `viv doctor --json 2>/dev/null | jq` is always clean (ADR-0015).

Human output groups by category, uses bracketed word markers — never unicode glyphs — and ends with
a summary line naming the exit:

```text
tooling
  [pass]     nix-present              nix 2.34 on PATH
  [pass]     nix-flakes-enabled       nix-command flakes
  [fail]     backend-binary-present   backend binary not found on PATH
                 hint: install the selected backend, then re-run viv doctor

disk
  [warn]     nix-store-disk-space     8.3 GB free (warn under 10 GB)
                 hint: reclaim space with viv gc

config
  [skipped]  manifest-resolves        no manifest bound - run viv init

4 pass - 1 warn - 1 fail - 1 skipped -> exit 69
```

`--json` emits one enveloped record:

```json
{
  "status": "fail",
  "checks": [
    {
      "id": "backend-binary-present",
      "category": "tooling",
      "scope": "host",
      "severity": "hard",
      "status": "fail",
      "message": "backend binary not found on PATH",
      "hint": "install the selected backend, then re-run viv doctor",
      "doc_url": "/doctor/checks/backend-binary-present"
    },
    {
      "id": "manifest-resolves",
      "category": "config",
      "scope": "project",
      "severity": "soft",
      "status": "skipped",
      "reason": "no-manifest-bound"
    }
  ],
  "summary": { "total": 7, "passed": 4, "warned": 1, "failed": 1, "skipped": 1, "hard_failures": 1 },
  "schema_version": "1"
}
```

- `reason` appears only on skipped checks (`no-manifest-bound`, `offline-mode`, `not-applicable`).
- `doc_url` is optional and **omitted when no page exists** — never emitted as null — so consumers
  test key presence and pages can ship incrementally. The path scheme is stable and predictable:
  `/doctor/checks/<id>`; the host is bound when the documentation site exists.
- `summary.hard_failures` lets scripts gate without re-deriving severity from the array.
