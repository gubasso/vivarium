# 09 — Glossary

Defined terms used across the vivarium documentation. Each term is defined once here; other pages use it without redefining it.

- Sandbox — the isolated microVM vivarium builds and runs for a project: a guest kernel behind a hardware-virtualization boundary, plus its mounts, network, and configuration.

- Guest — the operating system running inside the sandbox VM, distinct from the host it runs on.

- Host — the machine on which vivarium and the virtualization backend run.

- microVM — a lightweight virtual machine with a minimal device model and its own kernel, used as the isolation unit.

- Isolation boundary — the security boundary between guest and host. In vivarium it is hardware virtualization with a separate guest kernel, specified by capability class rather than a named tool (see [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)).

- Backend — the concrete virtual machine monitor that realizes the isolation boundary. Any backend satisfying the boundary class is admissible; the specific backend is an implementation choice.

- Guest agent — the small vivarium process inside the guest that receives control requests, launches exec/shell sessions, allocates PTYs, forwards signals/window-size changes, and reports exit status.

- Control socket — the host-side Unix socket in the per-project runtime directory that vivarium connects to for guest-agent sessions.

- vsock-class control transport — a host-local, network-independent guest/host transport capability used for control messages; the concrete backend device is an implementation detail.

- Image — a composable VM base, expressed as a NixOS module, capturing a toolchain and base system. See [`03-artifact-model.md`](./03-artifact-model.md).

- Piece — a small, single-purpose configuration fragment, expressed as a NixOS module, layered onto an image. See [`03-artifact-model.md`](./03-artifact-model.md).

- Manifest — the unifier that names one image plus an ordered set of pieces and policy knobs; the single source of truth a project binds to. Authored as TOML, compiled to a generated flake. See [`03-artifact-model.md`](./03-artifact-model.md).

- Leaf — the highest-specificity layer in a composition: the manifest's own settings, which use normal priority and so override image and piece defaults. Because the manifest is the personal layer ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)), the leaf is where one user's own choices win. See [`04-composition-and-determinism.md`](./04-composition-and-determinism.md).

- Workspace ownership — the association declared by a manifest's `[[workspaces]]` rows and used by the derived resolution rung to select one sandbox for an invoking directory. See [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).

- Workspace — the user's working directory, mounted read-write into the guest at a fixed location. See [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).

- Session — one active `exec` command or interactive `shell` attached through the guest agent.

- Sandbox key — the selected manifest name, which scopes per-sandbox state, data, cache, runtime directories, volumes, and systemd unit names. See [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).

- Target — the named VM instance within a sandbox, the `<target>` component of every per-sandbox state and runtime path. A sandbox has exactly one target, named `default`, in this version. See [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).

- Inner environment — the project's own development environment, owned by the repository and run inside the sandbox, independent of vivarium.

- Outer / inner evaluation — the two Nix evaluations: the outer builds the sandbox from the manifest at build time; the inner builds the project's development environment inside the guest at shell time.

- Launch channel — the runtime portion of the merged configuration (mounts, runtime environment, and resource ceilings, declared as `vivarium.mounts`, `vivarium.env`, and `vivarium.resources`) that the tool extracts by pure evaluation and applies when the VM launches; never a build input. Membership is per option, not per namespace: `vivarium.volumes` is a tool-owned option that belongs to the build channel. See [`04-composition-and-determinism.md`](./04-composition-and-determinism.md).

- Portable variable — an unexpanded, machine-independent host variable — `${HOME}` or an `${XDG_*}` directory — the only host reference permitted in shared mount declarations; resolved against the host environment at launch. See [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md).

- Egress — outbound network traffic from the guest, governed by the `sandbox.egress.mode` knob. See [`05-networking-and-egress.md`](./05-networking-and-egress.md).

- Generation — a retained, numbered past build output for a project, pinned as a garbage-collector root so it can be booted or rolled back to. See [`11-generations-and-build-history.md`](./11-generations-and-build-history.md).

- Stale — the state of a running VM whose booted build no longer matches the current build's store output because a layer changed. See [`10-vm-lifecycle.md`](./10-vm-lifecycle.md).

- Preflight — the hard subset of the shared `doctor` probe catalog a command runs at entry, refusing before any side effect. See [`10-vm-lifecycle.md`](./10-vm-lifecycle.md) and [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md).

- Hard / soft check — the severity of a probe in the shared `doctor` catalog: hard checks form the preflight subset and gate side-effect commands; soft checks only warn and never block. See [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md).

- Closure — the complete set of store paths a build depends on; what ships with a built VM.

- Store — the content-addressed Nix store holding build outputs. It is world-readable, which is why secrets must never enter it (see [`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)).

- Ceiling — the most a sandbox may use of a resource, as distinct from a reservation: nothing is committed to the sandbox up front, and the host pays only for what is used (N22). Every `[resources]` figure and every volume size is a ceiling. See [`17-resources-and-capacity.md`](./17-resources-and-capacity.md).

- Free page reporting — the guest's cooperative return of memory: the guest tells the host which pages it has finished with, and the host reclaims them, so a sandbox's resident cost tracks its working set instead of climbing to its ceiling. It requires no host action and takes nothing from the guest.

- Balloon — the guest device through which memory can be handed back on demand. vivarium runs it at zero size purely to enable free page reporting; the only time it is inflated is the bounded, user-invoked `viv trim`.

- Trim — reclaiming memory or volume space that a sandbox is holding but no longer needs. Memory is reclaimed only on explicit request (`viv trim`), never automatically (N23); volume space is also reclaimed by a periodic in-guest trim, with `viv volume trim` as the on-demand path. See [`17-resources-and-capacity.md`](./17-resources-and-capacity.md).

- Admission control — the host-capacity check `viv start` runs before building or booting: refuse below a minimum free-memory reserve, warn when the running fleet's measured use makes the new sandbox a risk (N23). See [`17-resources-and-capacity.md`](./17-resources-and-capacity.md).

- Resource scope — the host-side accounting group holding every process vivarium runs for one sandbox: the VMM, each per-share filesystem daemon, and launch helpers. It is what makes "what this sandbox costs" a single readable figure and what makes teardown one operation.

- Apparent / allocated size — a sparse volume image's declared virtual size against the space it actually occupies on the host. `viv status` and `viv volume list` report both.
