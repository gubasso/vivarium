# 13 — Doctor and health checks

The contract for `viv doctor`: the shared probe catalog, severity and status model, flags, output shapes, and exit codes. The governing decisions are [`../../decisions/ADR-0023-doctor-check-catalog-and-contract.md`](../../decisions/ADR-0023-doctor-check-catalog-and-contract.md) (this contract), [`../../decisions/ADR-0022-config-inspection-namespace.md`](../../decisions/ADR-0022-config-inspection-namespace.md) (`doctor` is a pure checker — it never renders configuration; that is `viv config`'s job), and [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md) (streams, `--json`, sysexits).

`viv doctor` diagnoses; it changes nothing. It is read-only, side-effect-free, and never modifies project, config, or VM state. One probe catalog serves three call sites: `doctor` runs the whole catalog, each command's preflight guard runs its hard subset before any side effect ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)), and setup flows reuse individual probes — so the checks can never drift apart.

## Severity, scope, and status

Each check carries three classifications:

- **Severity** — `hard` or `soft`. Hard checks form the **preflight subset**: a hard failure makes `viv start` (and a cold-starting `exec`/`shell`) refuse before any side effect. Soft checks only warn. There is no per-invocation waiver of a hard check — a check that could legitimately be ignored is soft by definition, so preflight remains an unconditional guarantee.
- **Scope** — `host` (always runnable), `project` (needs a bound manifest), or `network` (needs `--online`). Out-of-scope checks are **skipped**, never failed.
- **Status** (per run) — `pass`, `warn` (a soft check tripped), `fail` (a hard check tripped), or `skipped` (out of scope, with a reason). Skips never affect the exit code.

## Check catalog

Stable kebab-case ids; catalog order is cheapest-and-most-fundamental first, so the earliest failure is the most actionable.

### Hard — host scope (the preflight subset)

| Id                        | Category       | Passes when                                                                                                                         | Failure code |
| ------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| `nix-present`             | tooling        | `nix` is on `$PATH`                                                                                                                 | `69`         |
| `nix-version`             | tooling        | Nix meets the minimum version — the latest stable release at decision time (2.34); the implementation pins the exact floor          | `69`         |
| `nix-flakes-enabled`      | tooling        | `experimental-features` includes `nix-command flakes`                                                                               | `78`         |
| `kvm-device-present`      | virtualization | `/dev/kvm` exists                                                                                                                   | `69`         |
| `kvm-device-accessible`   | permissions    | the user can read and write `/dev/kvm`                                                                                              | `77`         |
| `hardware-virt-available` | virtualization | CPU virtualization extensions are present and enabled                                                                               | `69`         |
| `host-userns-available`   | permissions    | unprivileged user namespaces are available — the per-share filesystem daemon's namespace sandbox (N20) cannot be built without them | `69`         |

`kvm-device-present` and `kvm-device-accessible` are split because remediation differs: enable virtualization in firmware vs join the `kvm` group. `host-userns-available` is **hard** rather than soft because the N20 profile is by-construction and unwaivable: a host that cannot provide the namespace sandbox cannot launch at all, and failing at preflight is far more actionable than failing mid-launch.

**Nix is the only tool this catalog looks for on the host.** The hypervisor, its control client, and every filesystem daemon arrive in the built runner's closure, pinned by the project's lockfile, so their presence and version are settled at build time rather than probed ([`../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../decisions/ADR-0049-backend-is-a-closure-member.md)). That decision draws the line this catalog now follows: **`viv doctor` probes host conditions only; anything the lockfile determines is an evaluation-time assertion, not a check.**

### Soft — host scope

| Id                         | Category       | Warns when                                                                                                                                                                                                                                                                                                                  |
| -------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nix-store-disk-space`     | disk           | less than 10 GB free on the store filesystem (roughly one build cycle plus headroom)                                                                                                                                                                                                                                        |
| `state-dir-free-space`     | disk           | free space on the **state** filesystem is below the headroom the project's volumes could still claim — a sparse volume that meets host exhaustion surfaces inside the guest as an I/O error                                                                                                                                 |
| `host-memory-headroom`     | capacity       | available host memory is below the minimum reserve, or below the ceiling of the bound project — the same reading `viv start`'s admission check acts on ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md))                                                                                                   |
| `host-cgroup2-delegation`  | permissions    | the user's cgroup hierarchy does not have the memory and CPU controllers delegated, so per-VM cost reporting and CPU weighting are unavailable and admission falls back to host-level readings — **soft**, because the model degrades rather than breaks ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)) |
| `host-fd-limit-sufficient` | permissions    | the host's file-descriptor ceiling is below what a large workspace share will need — each per-share filesystem daemon holds one open descriptor per referenced inode and, running unprivileged, cannot raise its own limit, so a large tree can exhaust it mid-session                                                      |
| `kernel-version-supported` | virtualization | the host kernel is too old for the required virtio features                                                                                                                                                                                                                                                                 |
| `host-landlock-available`  | virtualization | the kernel lacks Landlock — the VMM's seccomp + capability-drop sandbox (N20) still applies, but the launch profile's filesystem-path allowlist is skipped                                                                                                                                                                  |
| `state-dir-writable`       | permissions    | the state root is not writable                                                                                                                                                                                                                                                                                              |
| `cache-dir-writable`       | permissions    | the cache root is not writable                                                                                                                                                                                                                                                                                              |
| `data-dir-writable`        | permissions    | the data root is not writable                                                                                                                                                                                                                                                                                               |
| `runtime-dir-writable`     | permissions    | `$XDG_RUNTIME_DIR` is not writable                                                                                                                                                                                                                                                                                          |

### Soft — project scope (skipped when no manifest is bound)

| Id                            | Category | Warns when                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ----------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config-parses`               | config   | the bound manifest does not parse. Parse only — `doctor` never evaluates or renders the merge; that is `viv config eval`                                                                                                                                                                                                                                                                                                                                                                                               |
| `manifest-resolves`           | config   | the binding does not resolve to exactly one defined manifest                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `shared-layer-paths-portable` | config   | a shared image or piece in the resolved composition declares a literal personal path where a portable variable belongs (N11, [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). A **textual** lint over the declarations — like `config-parses` it never evaluates, so it is only an early warning; the authoritative check runs at evaluation and returns `65` ([`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)) |

Two properties of the project's own guest are deliberately absent from this table: whether the guest kernel carries the balloon driver that makes declared memory a ceiling (N22), and whether each share names a cache policy the shipped daemon accepts. Both are settled by the composition and the lockfile, so both are evaluation-time assertions returning `65`, not probes ([`../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../decisions/ADR-0049-backend-is-a-closure-member.md), [`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)). Neither ever reads the declared memory figure, which is launch-channel and never a build input (N19).

When no manifest is bound, `doctor` still runs every host-scope check, notes once on stderr that project-scope checks are skipped (with `viv init` guidance), and marks each as `skipped` / `no-manifest-bound`. All host checks passing still exits `0`.

### Soft — network scope (skipped unless `--online`)

| Id                         | Category | Warns when                                                                                                                                                                                                                                                                           |
| -------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `nix-version-currency`     | network  | the installed Nix trails the latest stable release                                                                                                                                                                                                                                   |
| `substituter-reachability` | network  | a configured substituter is unreachable                                                                                                                                                                                                                                              |
| `egress-allowlist-dns`     | network  | an **allowlisted** hostname does not resolve — probed only when the bound manifest sets `sandbox.egress.mode = "allowlist"` (also project scope). Distinct from a _denied_ host, whose fail-fast contract is owned by [`05-networking-and-egress.md`](./05-networking-and-egress.md) |

The default run is fully offline. Open egress is **not** a check and never renders as a finding — it is policy, not a health defect (N8, [`05-networking-and-egress.md`](./05-networking-and-egress.md)).

### Guaranteed by construction (not probed)

Some facts about a launch are settled by how vivarium builds and launches, not by any host condition a probe could observe — the boundary [`../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../decisions/ADR-0049-backend-is-a-closure-member.md) draws for the whole catalog. Two of them are security properties: the separate guest kernel (N1) and the host-side sandbox around the VMM and every virtiofsd (N20). vivarium's launch wrapper enacts N20 as a fixed **hardened launch profile**, so the confinement cannot be disabled at runtime ([`../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)):

- **VMM (Cloud Hypervisor).** Launched unprivileged with `PR_SET_NO_NEW_PRIVS`, capabilities dropped toward zero, built-in seccomp enabled (`--seccomp true`), a Landlock path allowlist where the kernel supports it, and placement in the target's host resource scope ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)). The memory arguments the elastic model requires — shared guest memory, a zero-size balloon with free page reporting and deflate-on-OOM, and a per-VM control socket — are part of the same fixed profile.
- **virtiofsd (one process per share).** Launched unprivileged, `--sandbox=namespace`, seccomp enabled with the **killing** action — a logging or trapping action would leave the filter advisory and void N20 — with only its own share writable, never as root and never `--sandbox none`, the configuration behind the guest-root-to-host-root escape CVE-2026-47243. Unprivileged execution also costs each daemon the capability that would let it reference inodes by handle, so it falls back to holding one open descriptor per referenced inode against a limit it cannot raise for itself; the wrapper therefore sets the daemons' descriptor limit before `exec` (`host-fd-limit-sufficient` warns when the host ceiling is too low). Every daemon joins the same scope as the VMM, so a stopped VM leaves none behind. A share's **cache policy is coherency, not confinement**, and is set per share in [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md), as is the daemon's worker-pool sizing for the same reason; both are asserted at evaluation but are no part of this guarantee.
- **QEMU fallback is documentation-only** — admissible under N2 but not hardened; use Cloud Hypervisor for untrusted workloads.

Several of the values this profile fixes are also the upstream defaults today, and stating them anyway is the point: a guarantee that rests on an inherited default is one an upstream release can withdraw without a diff in this repository. The wrapper therefore names every value it depends on, whether or not it currently differs from the default.

These are guarantees of construction; `viv doctor` probes only the host _prerequisites_ (the soft-host checks, including `host-landlock-available`), never whether the sandbox "is on." See [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) and [`../../decisions/ADR-0024-backend-security-requirements.md`](../../decisions/ADR-0024-backend-security-requirements.md).

## Flags

```text
viv doctor [--json] [--strict] [--list] [--online]
```

- `--json` — one machine record on stdout (shape below).
- `-v` / `--verbose` — the **global** verbosity flag (see [`01-command-surface.md`](./01-command-surface.md) "Global flags"); on `doctor` it surfaces deeper per-check detail (observed versions, paths, permissions, free bytes).
- `--strict` — any `warn` fails the run: exit `1` (the one sanctioned use of `1`, per the ADR-0023 amendment to ADR-0015). For CI gates.
- `--list` — enumerate the catalog (id, category, scope, severity, title) **without running any probe**; combines with `--json`.
- `--online` — enable the network-scope checks. There is no `--offline`; offline is the default.

Color and TTY behavior follow ADR-0015 (`NO_COLOR > FORCE_COLOR > isatty`); there is no per-command color flag.

## Exit codes

Codes draw from the program-wide taxonomy in [`14-exit-codes.md`](./14-exit-codes.md); `doctor` maps its outcomes onto it as:

| Outcome                                                        | Exit                                   |
| -------------------------------------------------------------- | -------------------------------------- |
| No hard check failed (warns and skips allowed)                 | `0`                                    |
| Hard failure(s) — first failing check in catalog order decides | that check's code (`69` / `77` / `78`) |
| `--strict` and at least one `warn` (no hard failure)           | `1`                                    |
| Internal error in `doctor` itself                              | `70`                                   |

## Output

The report is the command's **result**: human report or `--json` record on stdout; progress and the skip note on stderr, so `viv doctor --json 2>/dev/null | jq` is always clean (ADR-0015).

Human output groups by category, uses bracketed word markers — never unicode glyphs — and ends with a summary line naming the exit:

```text
tooling
  [pass]     nix-present              nix 2.18 on PATH
  [fail]     nix-version              nix 2.18, minimum 2.34
                 hint: upgrade nix to 2.34 or newer, then re-run viv doctor
  [pass]     nix-flakes-enabled       nix-command flakes

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
      "id": "nix-version",
      "category": "tooling",
      "scope": "host",
      "severity": "hard",
      "status": "fail",
      "message": "nix 2.18, minimum 2.34",
      "hint": "upgrade nix to 2.34 or newer, then re-run viv doctor",
      "doc_url": "/doctor/checks/nix-version"
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
- `doc_url` is optional and **omitted when no page exists** — never emitted as null — so consumers test key presence and pages can ship incrementally. The path scheme is stable and predictable: `/doctor/checks/<id>`; the host is bound when the documentation site exists.
- `summary.hard_failures` lets scripts gate without re-deriving severity from the array.
