# Launch and supervision

The secure launch seam is implemented in [`src/launch`](../../src/launch/mod.rs) and built into the product runner from the crate in its default Cargo layout. The product flake is entered as `path:$REPO_ROOT?dir=nix`, which leaves the flake and its lock at [`nix/flake.nix`](../../nix/flake.nix) while making the repository the source tree, so [`nix/default.nix`](../../nix/default.nix) reads `../Cargo.lock` and `../src` under pure evaluation. Public manifest resolution and guest-agent readiness remain later work; see [implementation status](../reference/implementation-status.md) for that boundary.

Nix builds a runner closure containing the selected virtual machine monitor, guest configuration, and helper binaries. The CLI hands runtime-only paths and declarations to that closure. vivarium owns the launch arguments and constructs a confinement profile for the monitor and every guest-facing helper rather than relying on ambient host configuration.

Confinement combines unprivileged execution, capability removal, seccomp, Landlock path rules, read-only sources where applicable, and one sharing daemon per share. Typed endpoint construction admits only the exact console socket and declared host credential socket; seccomp does not inspect pathname arguments, and their parent runtime directory is never mounted. Helper descriptor limits and service limits are declared together so diagnostic thresholds derive from configuration instead of host inheritance.

One manager-owned transient user service per `(project, target)` is the VM lifetime. It is nested under `vivarium.slice`; the Rust supervisor is its `MainPID`, and the monitor, one virtiofsd per share, stream drains, and console reader stay in that unit's control group. The unit carries accounting, CPU weight, `LimitNOFILE`, and `KillMode=control-group`; it carries no memory or I/O limit. [ADR-0097](../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) records why the owner is a `.service` rather than ADR-0036's former literal `.scope` wording.

Startup is split at Cloud Hypervisor's pinned API boundary. The supervisor starts the share daemons and observes every exact share socket, starts the VMM in API-only mode, submits `vm.create`, waits for the exact serial socket, and starts the sole console reader. The reader acknowledges a live connection before the supervisor may issue `vm.boot`. Only then does the handoff report process readiness. Guest boot-identity readiness remains an injected seam for slice 003 and is not inferred from a process or socket.

The console task writes raw bytes to the private bounded `console.log` sink and fans the same stream to bounded attach subscribers. Subscriber lag can discard only that subscriber's view; it cannot block or replace the file or draining sink. `--no-console-log` selects the drain while retaining the reader. Reader ownership extends through VMM exit.

Unexpected child or reader exit, explicit cancellation, a process signal, or unit stop enters the same cancellation path. The supervisor requests backend shutdown, bounds the grace period, kills and reaps survivors, joins stream and console tasks, and then removes only its allowlisted sockets, pid files, launch metadata, and console files. An unknown runtime entry prevents directory removal. Volume images are outside this cleanup set.

Both binaries report a failure exactly once, at their process boundary. The fallible program returns a named error that keeps the underlying [`LaunchError`](../../src/launch/error.rs) as a source, and `main` renders that chain to stderr — for the supervisor, the journal, since it is the unit's `MainPID` — before classifying it. The category comes from [`ExitKind`](../../src/exit.rs), which transcribes the [`14-exit-codes.md`](../reference/spec/14-exit-codes.md) legend once for the whole crate, and each binary chooses from it in one exhaustive match rather than at each failing call, so a new failure mode cannot compile without being classified. Both faces are load-bearing for different readers: the number is what a caller branches on, and the chain is the only account of which child died. `ExitKind` carries the full set because the taxonomy is closed and append-only, not because every category is reachable today; the rendering skeleton, diagnostic ids, and `--json` failure object that [ADR-0033](../decisions/ADR-0033-error-handling-and-exit-codes.md) also fixes are later work. Whether `start`'s runtime-directory and socket failures belong in `74` at all is [Q-008](../plan/open-questions.md).

The handoff between them runs over the readiness socket the launcher binds before submitting the unit. One [`ReadinessReport`](../../src/launch/readiness.rs) crosses it — a schema version and a status of `process-ready` or `failed` — written by the supervisor and read by the launcher, which then hangs up. It is a type rather than an object assembled on each side so the two halves cannot drift: a version the launcher does not know is refused rather than read as a failure, which would attribute the launcher's own ignorance to the supervisor. The report says only that supervision started; the diagnostic for a `failed` report is in the supervisor's own journal output above, never on this socket.

The private modes that make that handoff trustworthy are named once, in [`secure_fs`](../../src/launch/secure_fs.rs): the launcher creates the runtime directory and writes the specification through the same two constants the supervisor's trust check reads back, so producer and validator cannot disagree about what private means.

The optional agent leg is either absent or the exact declared Unix socket object. It never substitutes a parent directory and never creates a runtime-directory share. Actual agent traffic and its readiness protocol belong to slice 003.

The normative boundaries are in the [workspace](../reference/spec/06-workspace-and-project-environment.md), [invariants](../reference/spec/08-invariants-and-guarantees.md), [lifecycle](../reference/spec/10-vm-lifecycle.md), [exec and shell](../reference/spec/12-exec-and-shell.md), [doctor](../reference/spec/13-doctor-and-health-checks.md), and [logging](../reference/spec/16-logging-and-diagnostics.md) specifications. Exact pinned backend facts live in [backend capabilities](../reference/backend-capabilities.md).

## Governing decisions

- [ADR-0001](../decisions/ADR-0001-microvm-isolation-boundary.md) — fixes the hardware-virtualization boundary this path must produce.
- [ADR-0009](../decisions/ADR-0009-launch-time-workspace-path-injection.md) — injects the workspace path at launch so it never becomes a build input.
- [ADR-0024](../decisions/ADR-0024-backend-security-requirements.md) — fixes the security requirements a backend has to satisfy.
- [ADR-0025](../decisions/ADR-0025-default-hypervisor-cloud-hypervisor.md) — selects the default virtual machine monitor.
- [ADR-0026](../decisions/ADR-0026-global-flags-and-config-precedence.md) — fixes the global flags the launch path reads.
- [ADR-0027](../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the confinement profile applied to the monitor and every helper.
- [ADR-0033](../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes how a failure becomes a rendered message and one exit category.
- [ADR-0036](../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — puts admission at launch instead of a background arbitrator.
- [ADR-0048](../decisions/ADR-0048-guest-module-only-vivarium-owns-the-runner.md) — keeps the runner vivarium-owned, exposing only a guest module.
- [ADR-0049](../decisions/ADR-0049-backend-is-a-closure-member.md) — makes the monitor and helpers closure members rather than host packages.
- [ADR-0056](../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md) — bounds the supervised lifetime by the user session.
- [ADR-0070](../decisions/ADR-0070-guest-console-capture-and-rotation.md) — fixes console capture, its destination, and its rotation boundary.
- [ADR-0078](../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — makes a backend fix a released pin move, which is why the pin is a launch-path concern.
- [ADR-0079](../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) — records the staffing posture behind those windows.
- [ADR-0081](../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md) — routes console output around the guest log daemon.
- [ADR-0093](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) — declares each helper's descriptor budget rather than inheriting it.
- [ADR-0095](../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md) — keeps measurement services out of the shipped image the launch path builds.
- [ADR-0097](../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) — makes the transient user service the detached lifetime owner and converges cancellation.
