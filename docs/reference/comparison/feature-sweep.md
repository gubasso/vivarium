# Feature sweep

How the row set in [`README.md`](./README.md) was derived. Each project was swept alone, in its own vocabulary, before any comparison was attempted; the four inventories were then merged. The order matters: deriving rows from vivarium and then asking what the others do produces a matrix that vivarium wins, and the win would be about row selection rather than about the tools.

Provenance for every entry is in [`sources.md`](./sources.md).

## 1. `vivarium`, swept alone

Marked `built` where [`implementation-status.md`](../implementation-status.md) says the command runs, `spec` where the specification fixes it and the code does not do it yet, and `refused` where a rule forecloses it. The rules are the binding product invariants in [`08-invariants-and-guarantees.md`](../spec/08-invariants-and-guarantees.md); this sweep names them in words rather than by number.

### Boundary

- `built` — a separate guest kernel behind a hardware-virtualization boundary, always
- `built` — the backend is named by capability class, not by product
- `refused` — a shared-kernel mode; the separate-kernel rule states there is none
- `refused` — degrading to a weaker boundary when the host cannot provide one
- `refused` — choosing a named hypervisor or engine from the manifest
- `refused` — snapshot, suspend, or saved machine state; `spec/10`'s ladder reaches `built` and saved state would persist runtime-injected secrets

### Definition and composition

- `built` — one TOML manifest resolved per project, compiled to a generated flake
- `built` — composition through the NixOS module system, no bespoke merge
- `built` — images and pieces as the two artifact kinds, with `extends` on manifests
- `built` — a shared artifact declares its own flake inputs through `inputs.toml`
- `built` — a pure build: same manifest closure and lock, same store output anywhere
- `built` — `viv config eval` and `viv config sources`, the merged view with provenance
- `built` — refusal of literal personal paths in shared artifacts, exit `65`
- `refused` — a guest built from a distribution other than NixOS; composition runs through the NixOS module system
- `refused` — pulling a prebuilt image from an OCI registry
- `refused` — arbitrary build steps running as root at build time

### Filesystem

- `built` — the workspace mounted at the absolute path it occupies on the host
- `built` — `[[mounts]]`, each its own confined virtiofsd, `readonly` enforced both sides
- `built` — a regular-file mount staged into its own export root, so neighbours never cross
- `built` — a linked worktree reaching its main repository
- `built` — `[[volumes]]`, persisting across stop, reboot, and rebuild
- `built` — the project tree is never written to by the tool
- `spec` — `[volume].persist` and `[volume].size_gib`, declarable and inert
- `spec` — raising a volume ceiling after its image exists
- `refused` — mounting `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`, or any ancestor
- `refused` — a backup or durability layer; `spec/00` refuses it and names the workaround

### Network

- `built` — a per-VM user and net namespace pair, tap, and one unprivileged `pasta` uplink
- `built` — egress open by default
- `built` — `egress.mode = "allowlist"`, default-deny in the kernel before any packet path
- `built` — a gating resolver that releases an answer only after installing its addresses
- `spec` — IPv6 addressing on the guest link
- open question — anything outside reaching a guest service; `spec/05` specifies outbound only

### Environment and credentials

- `built` — host environment deny-by-default against a fixed allowlist
- `built` — explicit per-invocation `--env`
- `built` — SSH and GPG relays over a second vsock port, the private key never entering the guest
- `spec` — a declarable agent channel in the manifest grammar
- `refused` — vivarium decrypting, holding an identity, or brokering a login
- `refused` — mounting a display or session socket; a share conveys an inode, not a listener

### Lifecycle and operation

- `built` — `viv init`, `viv start`, `viv stop`, `viv destroy`, `viv status`
- `built` — `viv exec` and `viv shell` against a running VM, over a private guest agent
- `built` — an interactive PTY sized before it starts, job control, resize forwarding
- `built` — project identity anchored by a marker, surviving a rename
- `built` — `viv volume list` and `viv volume prune`
- `built` — `viv doctor`, 31 probes, `--json`, `--strict`, `--list`, `--online`
- `built` — declared resources as ceilings, auto-sized from the host when undeclared
- `spec` — `viv unbind`, `viv images list`, `viv update`, `viv trim`
- `spec` — `viv generations list` / `activate` / `rollback` / `prune`
- `spec` — `viv start --generation <n>`, `--no-rebuild`, `--attach`
- `spec` — `viv volume rm`, `viv volume trim`
- `spec` — `viv gc`; the grammar runs and the store sweep does not
- `spec` — the capacity admission check before launch
- `spec` — `viv status -g`, and a live-session count
- `spec` — `viv stop --all`, and the first rung of `spec/10`'s ladder
- `spec` — `--json` records for `viv stop` and `viv destroy`
- `refused` — classifying the software inside as trusted or untrusted

## 2. `flake-pilot`, swept alone

From the registration schema, the manual pages, the two pilots, and the upstream `README.md`. Nothing here was chosen for having a vivarium counterpart.

### Boundary

- Two pilots: `podman-pilot`, whose OCI runtime is podman's own selection and reaches `krun` for a KVM boundary; firecracker under `firecracker-pilot`
- Three named isolation levels for the same application, chosen at registration time
- The engine is the user's choice, and upstream states plainly that the rungs are not equivalent — `krun` "gives isolation based on KVM and should be preferred for AI workloads"
- `sci` as firecracker guest init: evaluate `run=`, mount the overlay, exec, then reboot

### Definition

- A registration command materialized into `/usr/share/flakes/<app>.yaml`
- Drop-in directory `<app>.d/*.yaml`, alpha-sorted, last key wins, no merge semantics
- Registration is per-application, not per-project
- `--user` registers into the calling user's flake directory rather than system-wide
- A separate podman storage root, so the flake registry is invisible to ordinary `podman ps`

### Composition

- OCI layering: `--base` for a delta container, `--layer` repeatable and ordered
- `include.tar` and `include.path`, host payload transferred into the instance at provisioning
- Images built by any means the user likes: KIWI, podman, mkosi, OBS, koji
- A runtime store of prebuilt images, pulled on first use

### The launcher

- A registered application is a symlink to a pilot binary; the pilot reads `argv[0]`
- The sandboxed tool is therefore an ordinary command on `PATH` and looks native
- Call-time pseudo-arguments the pilot consumes: `@NAME`, `%remove`, `%interactive`, `%ignore_sync_error`, `%ignore_missing_volume_path`, `%progress`, `%port:number`
- `PILOT_DEBUG=1` for internals
- `flake-ctl list --format table|json|csv` enumerates every registration
- `%VAR` placeholders expanded against the caller's environment, degrading to the literal name

### Runtime

- Verbatim engine options through `--opt`, all-or-nothing: specifying any option drops the `-ti` and `--rm` defaults
- `--resume` keeps the instance; `--attach` joins a running one, podman rung only
- `runas` through sudo, governed by `/etc/sudoers`
- `overlay_size` for firecracker, an ext2 write layer of a declared size
- `mem_size_mib`, `vcpu_count`, `cache_type` for firecracker
- `force_vsock` for host and guest communication

### Network

- Whatever the engine options say; the published AI example is `--net host`
- Firecracker networking is the operator's job: `ip_forward`, MASQUERADE, a hand-edited `boot_args`, one TAP per instance
- `--no-net` to disable networking for a firecracker registration

### Known costs

- SELinux frequently blocks pilot operations
- Under `krun` there is no in-guest agent, so `podman exec` does not work and resume is unavailable
- Image tags are `:latest`, rebuilt daily
- Installed from an OBS repository, openSUSE-centric

## 3. `glaipnir`, swept alone

From `glaipnir.sh`, the image tree, and the macOS scripts.

### Boundary

- Rootless podman by default; a libkrun microVM when the host can provide one
- The probe: `/usr/bin/krun`, `libkrun.so.1` above 1.18.0, `/dev/kvm`, and `kvm` group membership
- Failure degrades to a plain container with a warning rather than refusing
- Missing `kvm` group prompts, runs `usermod`, and re-executes the script under `sg`
- `copilot` and `opencode` are forced out of microVM mode unconditionally, by a known libkrun vsock bug
- macOS never gets a microVM; Podman Machine is treated as the boundary

### Agents as a product concept

- A fixed roster of five trusted agents and one untrusted one
- The untrusted agent triggers an interactive disclaimer on `build` and `run`, and exits on anything but yes
- Per-agent credential directories, so `run claude` cannot see the `gh` token
- Per-agent authentication predicates in the banner, each printing the exact fix command
- Per-agent slim images, and one all-in-one image

### Filesystem

- One host cache directory holding every agent's credentials, seeded from the image with `cp -rn`
- The workspace mounted at `/home/aiuser/<path relative to $HOME>`, a mirror of the tail
- A workspace equal to `$HOME` is refused and falls back
- `--tmpfs /tmp:rw,nosuid,noexec,size=1g`
- `--userns keep-id`, so files the agent writes stay owned by the invoking user
- Everything durable lives on the host, so `clean` is cheap and every run is a fresh rootfs

### Network

- `pasta` bound to the first default-route interface that is not a VPN or bridge
- If no non-VPN interface exists, the run aborts rather than proceeding
- `--dns`, defaulting to `1.1.1.1` and `8.8.8.8`
- On macOS, a LaunchAgent daemon installing an nftables table inside Podman Machine, refreshed on VPN connect and disconnect
- Full-tunnel VPN refuses the run outright

### Runtime and extension

- `--cap-drop ALL`, `--security-opt no-new-privileges`, `--pids-limit 1024`
- `krun.ram_mib=8192`, `krun.cpus=4` as annotations
- Build hooks as root at build time, run hooks as `aiuser` on every start
- `NN-*.sh` ordered drop-in convention, `shellcheck`-validated before use
- `PACKAGES=(...)` in a config file, interpolated into the base install line
- The base system is openSUSE Tumbleweed, fixed in the `Containerfile`
- A hand-written config parser rather than sourcing, and config overrides the command line

### Operation

- `build`, `run`, `clean`, `status`
- `run` resumes: `podman exec` into a running container, or `podman start -ai`
- Under krun, re-entry is impossible, so a numbered sibling container is created instead
- `clean <agent>` keeps credentials; `clean <agent> all` removes credentials and images
- Diagnosis inline at the point of failure, each check printing the command that fixes it
- Distributed as an RPM from OBS, images from a public registry

## 4. `podman`, swept alone

Plain rootless podman with no wrapper.

- Rootless containers on a shared host kernel
- `--runtime` selects an OCI runtime, including `krun` where installed
- `-v` bind mounts, files or directories, at any target path
- `--env` and `--env-file`; nothing is forwarded unless named
- `--network none`, `--network host`, bridge, and `pasta`
- `--cap-drop`, `--security-opt`, `--pids-limit`, `--memory`, `--cpus`
- `podman exec` into a running container
- `podman ps -a` and `podman stop -a` across everything on the machine
- `podman system prune` and `podman volume prune` to reclaim disk
- Named volumes with their own lifecycle
- `podman generate systemd` and Quadlet units
- Images from any OCI registry, built from a `Containerfile` by arbitrary `RUN` steps, so the system inside is whatever the image is
- Runs on macOS and Windows through a managed VM
- Available as a package on essentially every distribution

## 5. The merge

Entries that name the same capability under different vocabularies were collapsed. An entry survived into the matrix when at least two subjects have a distinguishable answer to it, and when the answer is something a reader has to decide about rather than an implementation detail.

Cells are then filled at one fixed setup per subject — the microVM backend for all four, apples to apples:

1. `vivarium` — its one microVM.
2. `flake-pilot` — `firecracker-pilot`, at the upstream `claude` firecracker registration.
3. `glaipnir` — the libkrun microVM.
4. `podman` — rootless `podman run --runtime krun`.

A capability a subject has only at another backend is not credited. Backend availability and selection are not capability rows: the six entries themed `Backends` below fill the `Isolation backends` matrix in [`README.md`](./README.md) instead of a capability table. Filling a cell from whichever backend answers best would compare a tool with two modes against a tool with one, and every such mismatch found in review is listed below.

Verified: 2026-08-18 — merged from the four inventories above, at the commits `sources.md` pins.

| Merged capability                                       | Theme             | First seen in                                   |
| ------------------------------------------------------- | ----------------- | ----------------------------------------------- |
| Own kernel                                              | Backends          | `vivarium`, `flake-pilot`, `glaipnir`           |
| Shared-kernel isolation with an OCI runtime             | Backends          | `flake-pilot`, `glaipnir`, `podman`             |
| Nothing downgrades the boundary for you                 | Backends          | `vivarium`, `glaipnir`                          |
| Runs on a host without KVM                              | Backends          | `flake-pilot`, `glaipnir`, `podman`             |
| Choose the engine or hypervisor                         | Backends          | `flake-pilot`                                   |
| Work stays at its host path                             | Data              | `vivarium`, `glaipnir`                          |
| Host environment is deny-by-default                     | Data              | `vivarium`, `podman`                            |
| Session sockets refused as a mount source               | Data              | `vivarium`                                      |
| Use an SSH key without the key entering the sandbox     | Data              | `vivarium`                                      |
| Secrets are kept out of the built artifact              | Data              | `glaipnir`                                      |
| Credentials scoped per tool                             | Data              | `glaipnir`                                      |
| Egress can be default-deny                              | Network           | `vivarium`, `podman`                            |
| Allowlist by destination name                           | Network           | `vivarium`                                      |
| Stays off a corporate VPN                               | Network           | `glaipnir`                                      |
| Something outside can reach a guest service             | Network           | `flake-pilot`, `podman`                         |
| Defined by a project file                               | Guest environment | `vivarium`                                      |
| Compose the environment from separate, reusable parts   | Guest environment | `vivarium`, `flake-pilot`                       |
| A config unit works unchanged on someone else's machine | Guest environment | `vivarium`, `flake-pilot`                       |
| The same definition rebuilds the same environment       | Guest environment | `vivarium`                                      |
| Everyone building it gets the versions you got          | Guest environment | `vivarium`                                      |
| The build runs no user-supplied commands as root        | Guest environment | `vivarium`                                      |
| Choose the guest operating system                       | Guest environment | `flake-pilot`, `glaipnir`, `podman`             |
| Pull a prebuilt image instead of building               | Guest environment | `flake-pilot`, `glaipnir`, `podman`             |
| Re-enter a running instance                             | Living with it    | `vivarium`, `flake-pilot`, `glaipnir`, `podman` |
| Installs from a distro package in one command           | Living with it    | `flake-pilot`, `glaipnir`                       |
| The sandboxed tool feels like a native command          | Living with it    | `flake-pilot`                                   |
| Runs on macOS                                           | Backends          | `glaipnir`, `podman`                            |
| Boot a previous build when the new one is broken        | What accumulates  | `vivarium`                                      |
| Update on purpose rather than by surprise               | What accumulates  | `vivarium`                                      |
| Reclaim disk without a teardown                         | What accumulates  | `podman`, `glaipnir`                            |
| See every sandbox on the machine                        | What accumulates  | `flake-pilot`, `glaipnir`, `podman`             |

Fourteen of the thirty-one entries were first seen in a project other than vivarium — ten of the twenty-five capability rows and four of the six backend rows. That number is the point of sweeping separately, and it is the check worth repeating on any refresh: if a later sweep produces a table whose rows all originate with the subject, the sweep was not independent.

### Verdicts corrected to the microVM boundary

Found by re-reading each filled row against the rule above.

| Row                                               | Was                   | Now                   | Why                                                                     |
| ------------------------------------------------- | --------------------- | --------------------- | ----------------------------------------------------------------------- |
| Work stays at its host path                       | `flake-pilot` partial | `flake-pilot` no      | Firecracker has no share; the mirrored path is the podman engine        |
| Host environment is deny-by-default               | `flake-pilot` no      | `flake-pilot` yes     | Nothing crosses unless the registration names it, in either engine      |
| Session sockets refused as a source               | `flake-pilot` no      | `flake-pilot` n/a     | No bind-mount mechanism at that boundary, so nothing to refuse          |
| Default-deny egress                               | `flake-pilot` no      | `flake-pilot` partial | The firecracker level starts with the tap device off                    |
| Something outside reaches a guest service         | `glaipnir` partial    | `glaipnir` no         | The run publishes no port and takes no passthrough                      |
| Re-enter a running instance                       | `glaipnir` partial    | `glaipnir` no         | A krun guest cannot be entered; the resume path is the container        |
| The same definition rebuilds the same environment | `flake-pilot` no      | `flake-pilot` partial | Firecracker names local image files; `:latest` is the container rung    |
| The same definition rebuilds the same environment | `podman` no           | `podman` partial      | A digest reproduces exactly; nothing arranges one                       |
| Update on purpose                                 | `flake-pilot` no      | `flake-pilot` yes     | Nothing re-checks a local rootfs; updating is `pull --force`            |
| Update on purpose                                 | `podman` no           | `podman` partial      | A pulled image stays; the tag it came from does not                     |
| Reclaim disk without a teardown                   | `flake-pilot` partial | `flake-pilot` no      | `%remove` is podman-only; the firecracker overlay has no verb           |
| See every sandbox on the machine                  | `flake-pilot` yes     | `flake-pilot` partial | `flake-ctl list` reports registrations, not instances                   |
| Nothing downgrades the boundary for you           | `flake-pilot` no      | `flake-pilot` partial | Registration fixes the engine; only a drop-in file rewrites it          |
| Nothing downgrades the boundary for you           | `podman` n/a          | `podman` yes          | One boundary and nothing beneath it is stability, whatever its strength |

Verified: 2026-08-19 — re-read against the sources `sources.md` pins.

Eight of the fourteen moved in an alternative's favour, which is the check that the rule was applied to the comparison rather than to the competitors. Six came from re-reading the table, four from writing [`walkthroughs.md`](./walkthroughs.md), two more from re-reading [`scenarios.md`](./scenarios.md), and the last two from the row-label audit below — each pass found what the previous one could not, because a verdict, a worked example, a method, and a label fail in different ways. The walkthrough exposes a cell filled from the rung with the better answer; the method exposes a verdict resting on a mechanism the method never runs; the label exposes a question only one design was ever going to answer well.

### Verdicts corrected by fixing podman's setup

Until 2026-08-19 the `podman` column was read at its container boundary as the baseline. That was the last hidden cross-backend comparison in the set, so the column now reads at `podman run --runtime krun` like every other subject. Three verdicts moved, all against podman:

| Row                                       | Was          | Now              | Why                                                                                         |
| ----------------------------------------- | ------------ | ---------------- | ------------------------------------------------------------------------------------------- |
| Re-enter a running instance               | `podman` yes | `podman` no      | No in-guest agent under `krun`; the working `exec` is the shared-kernel runtime's           |
| Nothing downgrades the boundary for you   | `podman` yes | `podman` no      | The boundary is a per-invocation flag; omitting it silently runs the default runtime        |
| Something outside reaches a guest service | `podman` yes | `podman` partial | Publish is documented at the default runtime; under `krun` the path is TSI and undocumented |

Verified: 2026-08-19 — podman manual pages plus the libkrun project README. The third row is superseded the same day by `Verdicts corrected against upstream documentation` below: the publish path under `krun` is documented, in the README's networking section, and reading it moved the cell to yes.

### Verdicts reclassified as refusals

A gap and a refusal read the same in a table and mean opposite things to a reader deciding. One row moved once its design position was stated rather than assumed.

| Row                         | Was           | Now            | Why                                                                                                                                          |
| --------------------------- | ------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Credentials scoped per tool | `vivarium` no | `vivarium` no† | Automatic scoping needs a built-in table of application names; vivarium is application-agnostic and every path that crosses is user-declared |

Verified: 2026-08-19 — against `08-invariants-and-guarantees.md`, which carries no invariant stating application-agnosticism. That absence is the reason the cell is the set's only `†` citing a design position instead of a rule, and it is logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md). Two evidence sections were added in the same pass, so no cell in the row is a bare symbol.

### Labels corrected

`The boundary cannot be switched off` stated vivarium's property as the question, and asking it that way had already produced a wrong verdict: `flake-pilot` was marked `❌ no` when nothing at run time revisits a registered engine. The row is about a boundary changing underneath the user, which is `glaipnir`'s probe fallback and not `flake-pilot`'s registration. Its first rewrite, `Boundary
stays fixed once chosen`, still carried a presupposition: `once chosen` implies a choice was offered, which is why `podman` had been parked at `➖ n/a` while `vivarium` — with exactly as little choice — was answered `✅ yes`. `Nothing downgrades the boundary for you` names the event instead of the choice, and both single-mode subjects then answer the same way for opposite reasons. Three further labels ran past the six-word guidance and were shortened without changing what they ask.

Verified: 2026-08-19 — audited against the two row rules this document set follows: a label states an observable behavior, and a label is at most six words.

`Default-deny egress` failed the first of those two rules in the other direction: it reads as a statement of what a tool does, and vivarium's binding rule is that egress defaults to open, with one knob to switch. A reader taking the `✅ yes` as a default came away with the opposite of the specification. The method under the row had always asked the reachability question — its first step configures the most restrictive documented posture — so `Egress can be default-deny` names what was already being measured, and no verdict moved. The correction rows above keep the old label, because they record a reading made under it.

Verified: 2026-08-19 — against the egress-defaults-open rule in [`08-invariants-and-guarantees.md`](../spec/08-invariants-and-guarantees.md) and the two egress modes in [`05-networking-and-egress.md`](../spec/05-networking-and-egress.md). Two evidence sections were added in the same pass: the `vivarium` cells in both network-policy rows were the set's only unlinked `✅ yes` on rows where the other subjects carry evidence, and they were the cells where the surprise lived.

`Roll back to an older environment` named an action with no situation attached, so nothing in it could be verdicted. Both projects that own this idea upstream phrase it as booting a previous state because the current one failed: the NixOS manual's "Rolling Back Configuration Changes" describes booting any previous configuration not yet garbage-collected and says it is especially useful when the new configuration fails to boot, and openSUSE's reference titles the section "System rollback by booting from snapshots" and frames it as recovering a misconfigured system. `Boot a previous build when the new one is broken` says that in the set's own voice; the evidence keeps vivarium's own noun, generation. No verdict moved.

That retitle, and the three before it, retire the second of the two label rules above. A label is now required to state a claim a reader can verdict from the table alone, and length yields to that: `The same definition rebuilds the same environment` replaced a six-word label that said less. The first rule stands unchanged — a label states an observable behavior — and it is the one that was ever doing the work. The correction rows above keep the labels they were recorded under.

Verified: 2026-08-19 — the two upstream phrasings read at <https://nlewo.github.io/nixos-manual-sphinx/administration/rollback.xml.html> and <https://doc.opensuse.org/documentation/leap/reference/html/book-reference/cha-snapper.html>.

### Citations corrected

Three `vivarium` cells cited [`open-questions.md`](../../plan/open-questions.md) in prose without naming an entry, and two of those named no entry because none existed. A citation a reader cannot follow is worse than none: it claims a record is being kept and cannot be checked. All three now name a question, and the two missing ones were logged rather than dropped.

| Row                                       | Cell           | Cited                     | Now     |
| ----------------------------------------- | -------------- | ------------------------- | ------- |
| Credentials scoped per tool               | `vivarium` no† | design position, no entry | `Q-031` |
| Stays off a corporate VPN                 | `vivarium` no  | prose, no entry           | `Q-032` |
| Something outside reaches a guest service | `vivarium` no  | prose, no entry           | `Q-033` |

Two of the three evidence sections were rewritten in the same pass, because reading the code to write the question showed the cells had been stating absence where the behavior is something more specific. The VPN cell is the sharper one: the uplink keeps its sockets on the host side, so guest flows are re-originated as host sockets and follow the host routing table, which means a sandbox on a VPN-connected host is on the VPN, and its name lookups go to the host's resolver. Namespace, tap, uplink, and resolver are all per-VM; route selection is the one part the guest does not get its own copy of. The inbound cell moved the other way: nothing is arranged in either direction, and the specification names neither the capability nor a non-goal foreclosing it. No verdict moved — both were already `❌ no`, and both are better supported now.

Verified: 2026-08-19 — read against [`spec/05-networking-and-egress.md`](../spec/05-networking-and-egress.md), [`spec/00-goals-and-non-goals.md`](../spec/00-goals-and-non-goals.md), and the uplink the launcher builds in [`src/launch/policy.rs`](../../../src/launch/policy.rs).

### Verdicts corrected against upstream documentation

One cell in the previous round was marked partial for a reason that names the reader's ignorance rather than the tool's behavior: the mechanism was undocumented at the setup being read. That is a research gap, not a verdict, and it resolves by reading further.

| Row                                       | Was              | Now          | Why                                                                                                                                                                            |
| ----------------------------------------- | ---------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Something outside reaches a guest service | `podman` partial | `podman` yes | Impersonation is documented to carry inbound connections to a listening guest port, and an impersonated listener sits host-side in the namespace podman already publishes into |

The correction is worth stating as a rule. Reading the runtime's own documentation showed the mechanism is not a gap in the publish path but the thing that makes it work unchanged: because a guest listener is a host socket owned by the VMM process, `-p` needs no krun-specific handling, which is the same property that lets a sidecar reach the workload. Two conditions and one unsettled report bound the cell, and [`scenarios.md`](./scenarios.md#inbound-podman) carries all three rather than the verdict absorbing them.

This also sharpens the row's spread. It is now the one network row where the two microVM tools built as agent sandboxes both say no and the general-purpose runtime says yes, which reads as a posture rather than a shortfall — except that neither sandbox states it as one, and vivarium's silence is [`Q-033`](../../plan/open-questions.md).

Verified: 2026-08-19 — read, not run, against the libkrun project README's networking section, the `krun` manual page in `crun` for the `krun.use_passt` annotation, podman issue `25494` for the `AF_INET6` report, and a published `--annotation=run.oci.handler=krun -dp 8080:8080` run load-tested from the host.

### Rows added on a later pass

Five capabilities present in the inventories above had no row. Each was read back out of the sweeps rather than invented for the table, which is the check that they were available all along and were missed: composition and drop-ins are in the `flake-pilot` sweep, hooks and the runtime-only credential stance are in the `glaipnir` one, and module merge, `inputs.toml`, the lockfile, and the SSH and GPG relays are in vivarium's.

| Row                                                     | Theme             | Why it earns a row                                                                                |
| ------------------------------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| Compose the environment from separate, reusable parts   | Guest environment | All four layer something; only one reports a collision instead of letting a filename settle it    |
| A config unit works unchanged on someone else's machine | Guest environment | Every subject can share an image; sharing one concern is where they diverge                       |
| Everyone building it gets the versions you got          | Guest environment | Distinct from reproducibility: the input set itself being pinned, and portable to a second person |
| Use an SSH key without the key entering the sandbox     | Data              | The one credential question a shared-kernel answer cannot be borrowed for                         |
| Secrets are kept out of the built artifact              | Data              | A stance two subjects hold and two do not state at all                                            |

The three `Guest environment` rows are deliberately non-overlapping: same input gives the same output is `The same definition rebuilds the same environment`, the input set is pinned and portable is `Everyone building it gets the versions you got`, and the pin moves only when someone moves it is `Update on purpose`.

One of the five originates outside vivarium, which is worse than the set's running ratio and is recorded rather than smoothed over: adding rows a tool was built to answer is the failure mode this document exists to catch. The counterweight is that no new row is a one-column win — `glaipnir` takes the credential-stance row outright, and the composition and sharing rows are `⚠️ partial` for three subjects rather than `❌ no`.

Verified: 2026-08-19 — derived from §1 to §4 above, at the commits `sources.md` pins.

### Cut, and why

- Resource ceilings and process limits — every subject has them and the differences are numeric rather than decidable.
- Trusted and untrusted software classification — a real `glaipnir` product idea with no counterpart anywhere else, so a row would be one column wide. It is carried in [`walkthroughs.md`](./walkthroughs.md) instead.
- Snapshot and saved machine state — no subject in this set offers it.
- Systemd unit generation and Quadlet — `podman`-only, and about host integration rather than the sandbox.
- Cache policy per share, PTY sizing, exit-code propagation — implementation detail below the level a reader decides at.
