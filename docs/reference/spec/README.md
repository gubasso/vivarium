# vivarium specification

The product contract: what vivarium is and does. These pages are reference material — organized for lookup, not narrative. They describe the intended design; for what is actually implemented today, see [`../implementation-status.md`](../implementation-status.md).

The why behind these choices lives in the decision records under [`../../decisions/`](../../decisions/); each page links to the ADRs that govern it rather than restating their reasoning.

## Pages

- [`00-goals-and-non-goals.md`](./00-goals-and-non-goals.md) — what vivarium is for, what it is not, and who it serves.
- [`01-command-surface.md`](./01-command-surface.md) — the command-line verbs, flags, and exit behavior.
- [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md) — how each XDG root resolves, manifest-keyed state and runtime layouts, the derived workspace-owner index, and the reserved target component.
- [`03-artifact-model.md`](./03-artifact-model.md) — images, pieces, and manifests, with example shapes.
- [`04-composition-and-determinism.md`](./04-composition-and-determinism.md) — how layers merge and what makes a build reproducible.
- [`05-networking-and-egress.md`](./05-networking-and-egress.md) — the network model and the open/allowlist egress knob.
- [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md) — the mounted workspace and the independent inner project environment.
- [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md) — sharing config across a team and injecting secrets safely.
- [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) — the numbered normative guarantees the product must uphold.
- [`09-glossary.md`](./09-glossary.md) — defined terms used throughout the docs.
- [`10-vm-lifecycle.md`](./10-vm-lifecycle.md) — how `viv start` builds and boots the sandbox and how `viv stop`/`viv destroy` bring it down: lifecycle states, idempotency, staleness, preflight, and the Nix validation ladder.
- [`11-generations-and-build-history.md`](./11-generations-and-build-history.md) — the numbered history of a project's builds, pinned as GC roots so a past build can be booted or pruned.
- [`12-exec-and-shell.md`](./12-exec-and-shell.md) — how `viv exec` and `viv shell` enter a running or newly-started guest: argv boundary, stdio/TTY behavior, control socket, workspace cwd, environment, and exit-status boundary.
- [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md) — the `viv doctor` contract: the shared probe catalog, hard/soft severity and the preflight subset, statuses, flags, output shapes, and exit codes.
- [`14-exit-codes.md`](./14-exit-codes.md) — the single source of truth for exit codes: the program-wide sysexits legend, the guest pass-through boundary, the append-only stability guarantee, and the per-command exit-code matrix.
- [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md) — the diagnostic logging model: the three output faces (stdout/stderr/file), the always-on log file and its XDG-state path, levels, `logfmt`/`json` format, and the `--log-*` flags and `VIV_LOG*` env vars.
- [`17-resources-and-capacity.md`](./17-resources-and-capacity.md) — why declared resources are ceilings rather than reservations: the auto-sizing policy, the per-VM host scope, admission control at `start`, `viv memory trim` and the `viv trim` fan-out, the used-against-ceiling reporting, and the multi-project capacity story.
