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
- `built` — packages and guest configuration declared inside an image or a piece as ordinary NixOS module options, concatenating across layers
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
- `spec` — the project's own development environment as an independent inner layer: two evaluations never conflated, and a guest base that must ship Nix with flakes and direnv so entering the workspace loads it
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

- `built` — `viv start`, `viv stop`, `viv destroy`, `viv status`, `viv config`
- `built` — `viv exec` and `viv shell` against a running VM, over a private guest agent
- `built` — an interactive PTY sized before it starts, job control, resize forwarding
- `built` — a sandbox keyed by its manifest name, shared by every workspace that manifest declares
- `built` — `viv volume list` and `viv volume prune`
- `built` — `viv doctor`, 31 probes, `--json`, `--strict`, `--list`, `--online`
- `built` — declared resources as ceilings, auto-sized from the host when undeclared
- `spec` — `viv images list`, `viv update`, `viv trim`
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
- Images built outside flake-pilot; the firecracker artifact is KIWI's `kis` type, built directly or through the Open Build Service
- Upstream ships the image descriptions behind its own appstore VMs, so building your own has a worked example to copy
- A runtime store of prebuilt images, pulled on first use
- Images enter that store over https only, relaxed by `FLAKE_ALLOW_INSECURE_TRANSPORT`; `flake-ctl podman load` has no firecracker counterpart
- A KIS archive carries a sha256 record of its rootfs, which `pull` verifies

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
- `podman secret create` with a `file`, `pass`, or `shell` driver, and `--secret` to mount one at runtime
- Named volumes with their own lifecycle
- `podman generate systemd` and Quadlet units
- Images from any OCI registry, built from a `Containerfile` by arbitrary `RUN` steps, so the system inside is whatever the image is
- Runs on macOS and Windows through a managed VM
- Available as a package on essentially every distribution

## 5. The merge

Entries that name the same capability under different vocabularies were collapsed. An entry survived into the matrix when at least two subjects have a distinguishable answer to it, and when the answer is something a reader has to decide about rather than an implementation detail.

Cells are then filled at one fixed isolation class for every subject — a microVM with its own guest kernel — with the engine inside that class left to be whatever each subject reaches it by:

1. `vivarium` — its one microVM.
2. `flake-pilot` firecracker — `firecracker-pilot`, at the upstream `claude` firecracker registration.
3. `flake-pilot` krun — `podman-pilot` at `--runtime krun`, at the upstream `claude` `krun` registration.
4. `glaipnir` — the libkrun microVM.
5. `podman` — rootless `podman run --runtime krun`.

Five columns for four subjects, because flake-pilot reaches the class two ways and electing one of them would be a selection the phrase "the microVM" does not make. A capability a subject has only outside the class is not credited. Backend availability and selection are not capability rows: the seven entries themed `Backends` below fill the `Isolation backends` matrix in [`README.md`](./README.md) instead of a capability table. Filling a cell from whichever backend answers best would compare a tool with two modes against a tool with one, and every such mismatch found in review is listed below.[^merge]

| Merged capability                                                        | Theme             | First seen in                                   |
| ------------------------------------------------------------------------ | ----------------- | ----------------------------------------------- |
| Own kernel                                                               | Backends          | `vivarium`, `flake-pilot`, `glaipnir`           |
| Shared-kernel isolation with an OCI runtime                              | Backends          | `flake-pilot`, `glaipnir`, `podman`             |
| No flag selects a weaker boundary                                        | Backends          | `vivarium`, `glaipnir`                          |
| No file you did not write selects a weaker boundary                      | Backends          | `flake-pilot`, `podman`                         |
| Runs on a host without KVM                                               | Backends          | `flake-pilot`, `glaipnir`, `podman`             |
| Choose the engine or hypervisor                                          | Backends          | `flake-pilot`                                   |
| Runs on macOS                                                            | Backends          | `glaipnir`, `podman`                            |
| The tool derives the workspace mount, with no host path named            | Data              | `vivarium`, `glaipnir`                          |
| Work stays at its host path                                              | Data              | `vivarium`, `glaipnir`                          |
| Choose which host paths cross, in a file rather than on the command line | Data              | `vivarium`                                      |
| The caller chooses which host environment variables cross                | Data              | `vivarium`, `podman`                            |
| Refuses a mount that would expose the host session                       | Data              | `vivarium`                                      |
| Use an SSH or GPG key without the key entering the sandbox               | Data              | `vivarium`                                      |
| Secrets are kept out of the built artifact                               | Data              | `glaipnir`                                      |
| Keeping secrets out of the build is enforced                             | Data              | `glaipnir`                                      |
| Commit an encrypted secret alongside the config                          | Data              | `vivarium`, `podman`                            |
| Scopes credentials per app out of the box                                | Data              | `glaipnir`                                      |
| Egress can be default-deny                                               | Network           | `vivarium`, `podman`                            |
| Allowlist by destination name                                            | Network           | `vivarium`                                      |
| Stays off a corporate VPN                                                | Network           | `glaipnir`                                      |
| Something outside can reach a guest service                              | Network           | `flake-pilot`, `podman`                         |
| Defined by a project file                                                | Guest environment | `vivarium`                                      |
| The project's own dev environment loads when you enter                   | Guest environment | `vivarium`                                      |
| Compose the environment from separate, reusable parts                    | Guest environment | `vivarium`, `flake-pilot`                       |
| A collision between two parts is reported                                | Guest environment | `vivarium`                                      |
| There is a shareable unit smaller than the whole environment             | Guest environment | `vivarium`, `flake-pilot`                       |
| A shared unit's portability is enforced                                  | Guest environment | `vivarium`                                      |
| The same definition gives everyone the same environment                  | Guest environment | `vivarium`                                      |
| Choose what goes in the guest, and set it up at build                    | Guest environment | `glaipnir`, `podman`                            |
| Run your own setup at every start                                        | Guest environment | `glaipnir`                                      |
| The build runs no user-supplied commands as root                         | Guest environment | `vivarium`                                      |
| Choose the guest operating system                                        | Guest environment | `flake-pilot`, `glaipnir`, `podman`             |
| Pull a prebuilt image instead of building                                | Guest environment | `flake-pilot`, `glaipnir`, `podman`             |
| A later command reaches the instance already running                     | Living with it    | `vivarium`, `flake-pilot`, `glaipnir`, `podman` |
| A second session joins it while the first is still there                 | Living with it    | `vivarium`                                      |
| Installs from a distro package in one command                            | Living with it    | `flake-pilot`, `glaipnir`                       |
| The sandboxed tool feels like a native command                           | Living with it    | `flake-pilot`                                   |
| Boot a previous build when the new one is broken                         | What accumulates  | `vivarium`                                      |
| Update on purpose rather than by surprise                                | What accumulates  | `vivarium`                                      |
| Reclaim disk without a teardown                                          | What accumulates  | `podman`, `glaipnir`                            |
| See every definition on the machine                                      | What accumulates  | `flake-pilot`, `glaipnir`, `podman`             |
| See every running instance on the machine                                | What accumulates  | `flake-pilot`, `glaipnir`, `podman`             |

Nineteen of the forty-two entries were first seen in a project other than vivarium — fourteen of the thirty-five capability rows and five of the seven backend rows. That number is the point of sweeping separately, and it is the check worth repeating on any refresh: if a later sweep produces a table whose rows all originate with the subject, the sweep was not independent.

### Verdicts corrected to the microVM boundary

Found by re-reading each filled row against the rule above.[^microvm-boundary]

| Row                                                | Was                   | Now                   | Why                                                                     |
| -------------------------------------------------- | --------------------- | --------------------- | ----------------------------------------------------------------------- |
| Work stays at its host path                        | `flake-pilot` partial | `flake-pilot` no      | Firecracker has no share; the mirrored path is the podman engine        |
| Choose which host environment variables cross      | `flake-pilot` no      | `flake-pilot` yes     | Nothing crosses unless the registration names it, in either engine      |
| Refuses a mount that would expose the host session | `flake-pilot` no      | `flake-pilot` n/a     | No bind-mount mechanism at that boundary, so nothing to refuse          |
| Default-deny egress                                | `flake-pilot` no      | `flake-pilot` partial | The firecracker level starts with the tap device off                    |
| Something outside reaches a guest service          | `glaipnir` partial    | `glaipnir` no         | The run publishes no port and takes no passthrough                      |
| Re-enter a running instance                        | `glaipnir` partial    | `glaipnir` no         | A krun guest cannot be entered; the resume path is the container        |
| The same definition rebuilds the same environment  | `flake-pilot` no      | `flake-pilot` partial | Firecracker names local image files; `:latest` is the container rung    |
| The same definition rebuilds the same environment  | `podman` no           | `podman` partial      | A digest reproduces exactly; nothing arranges one                       |
| Update on purpose                                  | `flake-pilot` no      | `flake-pilot` yes     | Nothing re-checks a local rootfs; updating is `pull --force`            |
| Update on purpose                                  | `podman` no           | `podman` partial      | A pulled image stays; the tag it came from does not                     |
| Reclaim disk without a teardown                    | `flake-pilot` partial | `flake-pilot` no      | `%remove` is podman-only; the firecracker overlay has no verb           |
| See every sandbox on the machine                   | `flake-pilot` yes     | `flake-pilot` partial | `flake-ctl list` reports registrations, not instances                   |
| Nothing downgrades the boundary for you            | `flake-pilot` no      | `flake-pilot` partial | Registration fixes the engine; only a drop-in file rewrites it          |
| Nothing downgrades the boundary for you            | `podman` n/a          | `podman` yes          | One boundary and nothing beneath it is stability, whatever its strength |

Eight of the fourteen moved in an alternative's favour, which is the check that the rule was applied to the comparison rather than to the competitors. Six came from re-reading the table, four from writing [`walkthroughs.md`](./walkthroughs.md), two more from re-reading [`scenarios/`](./scenarios/README.md), and the last two from the row-label audit below — each pass found what the previous one could not, because a verdict, a worked example, a method, and a label fail in different ways. The walkthrough exposes a cell filled from the rung with the better answer; the method exposes a verdict resting on a mechanism the method never runs; the label exposes a question only one design was ever going to answer well.

### Verdicts corrected by reading flake-pilot at both routes

Until 2026-08-21 the `flake-pilot` column was read at `firecracker-pilot` alone. That was the last hidden single-route selection in the set: upstream reaches the microVM class two ways, publishes a `claude` registration for each, and states the two are not equivalent, so naming the class did not name a setup. The subject now carries two columns. No verdict at the firecracker route moved; these are the twelve rows where the second route answers differently — eleven of them changing the mark, and one reaching the same mark by another mechanism — and they run both ways.[^both-routes]

| Row                                                                      | firecracker | krun    | Why                                                                                                          |
| ------------------------------------------------------------------------ | ----------- | ------- | ------------------------------------------------------------------------------------------------------------ |
| Work stays at its host path                                              | n/a         | yes‡    | A registered `--volume` mirroring a path crosses as virtio-fs                                                |
| Choose which host paths cross, in a file rather than on the command line | n/a         | yes‡    | The registration is a file, and a drop-in adds to it                                                         |
| Refuses a mount that would expose the host session                       | n/a         | no      | There is now a mount to refuse, and nothing refuses it                                                       |
| The project's own dev environment loads when you enter                   | no          | yes‡    | A mounted directory and a shell target; whether a toolchain loads is the image's property                    |
| Something outside reaches a guest service                                | partial     | yes     | Impersonated listeners reach podman's ordinary publish path                                                  |
| Stays off a corporate VPN                                                | partial     | no      | Both libkrun networking modes leave through the host's routing table                                         |
| A later command reaches the instance already running                     | yes         | no      | The engine has no `exec`, so a registration cannot use `--resume`                                            |
| The same definition gives everyone the same environment                  | partial     | no      | The unit is a `:latest` tag on a registry rebuilt nightly                                                    |
| Update on purpose                                                        | yes         | no      | The same moving tag, moving without being asked                                                              |
| Reclaim disk without a teardown                                          | no          | partial | `%remove` drops the container's writable layer without touching the registration                             |
| Egress can be default-deny                                               | yes         | yes‡    | Deny is the shipped state at one route and an option to write into the registration at the other             |
| Boot a previous build when the new one is broken                         | no          | no      | Same verdict, opposite reason: firecracker overwrites the bytes, `krun` keeps bytes no registration can name |

Five move toward the second route and six away from it, which is the check that adding a column was a correction rather than a concession. The row that stayed put is the instructive one: two routes reaching the same `❌` by opposite mechanisms is what a single column had been hiding, and it is the shape [the workspace row](./scenarios/workspace-mount.md#flake-pilot) has too.

One neighbouring claim was tested and rejected in the same pass. `overlay_size` at the firecracker route was put to us as a host mount, in the same breath as the registered `--volume` at the `krun` route. The engine schema carries no mount key of any kind, and the overlay is a sparse ext2 image attached as a second virtio-blk drive, so that verdict did not move — but the row's label had let the two readings coexist, which is the label correction below.

### A verdict corrected by locating the include payload

The `flake-pilot` author, reading these tables, rejected the `❌` on `Secrets are kept out of the built artifact`: no image the project ships carries a credential, and a token obtained after registration lands in instance storage rather than in the image. Reading the two provisioning paths at `920f41e` confirms it, and the cell's reasoning was wrong twice.[^include-destination]

| Row                                        | Was                  | Now                    | Why                                                                              |
| ------------------------------------------ | -------------------- | ---------------------- | -------------------------------------------------------------------------------- |
| Secrets are kept out of the built artifact | `flake-pilot` no, no | `flake-pilot` yes, yes | An include payload lands on the instance, and the tool builds no artifact at all |

The first error was mechanical. The cell had said an `include.tar` / `include.path` payload is carried "inside the artifact that gets registered and shared". It is not: at the firecracker route the payload is synced into a per-instance ext2 overlay layered over the image and unmounted before boot, and at the `krun` route into `podman mount <container-id>`, the created container's writable rootfs. Neither pilot calls `podman build` or `podman commit`, so nothing the payload touches is ever an artifact a second person receives. Three other pages carried the same phrasing and two more implied it; all five were corrected with it.

The second error was the row applied to a subject that has no stage for it. flake-pilot registers an image built elsewhere — pulled as a KIS archive or named in a registry — so "does the tool's build put a credential in the artifact" has no subject here, and a credential baked into an image is the image builder's flow rather than this one's.

What survives is the absence of a secrets mechanism, which is real and is already scored twice: `Keeping secrets out of the build is enforced` and `Commit an encrypted secret alongside the config` both read `❌` for flake-pilot and both keep it. Scoring it a third time under a question about the artifact was double-counting, and it is what let a factual claim about provisioning ride along unchecked.

The row now reads yes in every column, which is the cost of the correction and is recorded rather than avoided. It keeps its place for two reasons: the marks still separate the subjects, since podman's `✅ yes‡` says the safe form is reachable while the ordinary `ENV TOKEN=…` is not it; and the row is one half of a pair whose other half discriminates sharply, vivarium alone reaching `⚠️ partial` where every alternative reads `❌`.

### A row corrected by finding the layering this sweep had already recorded

The flake-pilot sweep above lists, under `Composition`, "OCI layering: `--base` for a delta container, `--layer` repeatable and ordered". The merged row lost it. [`composition.md`](./scenarios/composition.md#flake-pilot) said that inside the guest there is no second level and that composing what goes into the artifact belongs to the builder rather than to flake-pilot. That is wrong at the `krun` route, where `podman-pilot` image-mounts a `base_container`, then each entry of an ordered `layers:` list, then the application container, syncing each onto the instance at provisioning. The mechanism is flake-pilot's own, and upstream publishes it as a use case: a solution stack of base plus python plus python-app, and deltas pulled against a base that exists only once.

No verdict moved, because the row was already yes at both flake-pilot columns on the strength of drop-ins alone. What was wrong was the reason under two cells, and a second row rested on the same mistake: [`config-unit.md`](./scenarios/config-unit.md#flake-pilot) said the image is the whole environment, where a delta container is precisely a unit smaller than one. [`collision.md`](./scenarios/collision.md#flake-pilot) gains the second place two parts meet without a report, its verdict unchanged. The firecracker route keeps the old reading, which is correct there: `firecracker-pilot` has neither key.

This is the failure this sweep exists to prevent, running backwards. The inventory was right and the merge dropped it, so sweeping alone caught what comparing lost.

### A framing corrected for a subject that does not build

A reader of this comparison objected that several rows assume the sandboxing tool creates the build artifact, and that for flake-pilot this does not apply — the images come from a store, and the build questions belong to whatever produced them. The objection is correct, and it is now answered where it belongs rather than inside a cell: [the methodology](./methodology.md) states the split between building and running, and how a build row is scored for a subject that only runs.

Verified at `920f41e`: the entire `flake-ctl` command surface is `pull`, `load`, `register`, `show`, `remove`, `init`, and `list`, and no `build` or `commit` call appears anywhere in either pilot. Upstream states the delegation as a position rather than leaving it as silence, naming the Open Build Service with KIWI as one option among the different ways an image can be built, and anchoring trust at the image source instead. [`same-definition.md`](./scenarios/same-definition.md#flake-pilot) now carries that position; its two verdicts stand, because what a registration names is the pilot's own surface rather than the builder's.

One mark moved. [`build-steps.md`](./scenarios/build-steps.md#flake-pilot) keeps its no at both columns, because the row asks what produced the artifact a user receives and the documented flow produces it through an arbitrary root shell — the same `config.sh` that earns the yes at [`own-setup-at-build.md`](./scenarios/own-setup-at-build.md#flake-pilot), credited once and charged once. What it gains is `†`: the guarantee is one flake-pilot placed outside itself, not one it failed to make. These are the first `†` marks in the tables that are not vivarium's, which was the asymmetry worth fixing on its own — the same kind of design refusal had been reading as a position for one subject and as a gap for another.

### Verdicts corrected by fixing podman's setup

Until 2026-08-19 the `podman` column was read at its container boundary as the baseline. That was the last hidden cross-backend comparison in the set, so the column now reads at `podman run --runtime krun` like every other subject. Three verdicts moved, all against podman:[^podman-setup]

| Row                                       | Was          | Now              | Why                                                                                         |
| ----------------------------------------- | ------------ | ---------------- | ------------------------------------------------------------------------------------------- |
| Re-enter a running instance               | `podman` yes | `podman` no      | No in-guest agent under `krun`; the working `exec` is the shared-kernel runtime's           |
| Nothing downgrades the boundary for you   | `podman` yes | `podman` no      | The boundary is a per-invocation flag; omitting it silently runs the default runtime        |
| Something outside reaches a guest service | `podman` yes | `podman` partial | Publish is documented at the default runtime; under `krun` the path is TSI and undocumented |

### Verdicts reclassified as refusals

A gap and a refusal read the same in a table and mean opposite things to a reader deciding. One row moved once its design position was stated rather than assumed.[^refusals]

| Row                         | Was           | Now            | Why                                                                                                                                          |
| --------------------------- | ------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Credentials scoped per tool | `vivarium` no | `vivarium` no† | Automatic scoping needs a built-in table of application names; vivarium is application-agnostic and every path that crosses is user-declared |

### A verdict moved because the subject changed

Every other entry here corrects a reading. This one records a subject that moved under a reading that was correct when it was made: `vivarium` derived the workspace from the invoking directory, and at `162f230` it stopped. `[[workspaces]]` makes the project tree a declaration carrying a host `source`, a manifest that declares none cannot launch, and the sandbox that held one tree holds a set ([`ADR-0108`](../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md)).

| Row                                                           | Was            | Now            | Why                                                                                                                                                                                      |
| ------------------------------------------------------------- | -------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The tool derives the workspace mount, with no host path named | `vivarium` yes | `vivarium` no† | The tree is declared rather than discovered; deriving it would make the mount set a function of the call, and a directory must belong to one sandbox for resolution from it to be unique |

A project moving its own cell from a yes to a no is where a self-authored comparison earns or loses its credibility, so the `†` faces the same test as every other one: a recorded position that names what it costs. [`ADR-0109`](../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) writes the cost down — a user who moves a project edits the manifest by hand, where the marker used to follow the move — and refuses rather than defaulting. Two neighbouring rows gained the same reading without moving: mirroring now runs across a declared set, and the crossing set is two tables rather than one.

The row is also the only one where `vivarium` and `flake-pilot`'s `krun` route now answer alike for related reasons, both having been told which tree to carry. What still separates them is where the telling lives, which is the row this one defers to.

### A claim corrected where the reader was right

The same reader objected that the image description is not unshared: upstream publishes it in the appstore, at both routes. Confirmed at `920f41e` — [`appstore/firecracker/claude/`](https://github.com/OSInside/flake-pilot/tree/920f41e/appstore/firecracker/claude) and [`appstore/podman/claude/`](https://github.com/OSInside/flake-pilot/tree/920f41e/appstore/podman/claude) each carry an `appliance.kiwi` and a `config.sh` — and this comparison already cited those files as evidence on [the guest OS row](./scenarios/guest-os.md#flake-pilot) and [the setup row](./scenarios/own-setup-at-build.md#flake-pilot). One page was contradicting two others rather than reading something new.

The verdict holds and the reason changed, which is the correction worth having. [`same-definition.md`](./scenarios/same-definition.md#flake-pilot) had rested its `partial` on the description being unavailable, which was wrong. It now rests it on what the published description does: repositories named as moving branches, packages with no version, and a `config.sh` that installs the agent from the network, so a rebuild a month later is a different image built honestly from the same file. Published is not reproduced — a claim about a file a reader can open, rather than about what they were given.

Two smaller changes came with it. Upstream began publishing a `<image>.tar.xz.sha256` beside each appstore tarball at [`36090e6`](https://github.com/OSInside/flake-pilot/commit/36090e6db7e244494982303f47cbb8453b9395cf) on 2026-08-23, after this subject's reading; it is the external identifier the row said did not exist, `pull` does not fetch it, and the row now names it. And the sha256 argument lost its security vocabulary: a record travelling inside the archive it attests reads as a tamper claim, the reader answered it as one by asking whether any artifact is unsafe because its holder can edit it, and no cell in these tables measures tamper resistance for any subject, `vivarium` included.

### Labels corrected

`The boundary cannot be switched off` stated vivarium's property as the question, and asking it that way had already produced a wrong verdict: `flake-pilot` was marked `❌ no` when nothing at run time revisits a registered engine. The row is about a boundary changing underneath the user, which is `glaipnir`'s probe fallback and not `flake-pilot`'s registration. Its first rewrite, `Boundary
stays fixed once chosen`, still carried a presupposition: `once chosen` implies a choice was offered, which is why `podman` had been parked at `➖ n/a` while `vivarium` — with exactly as little choice — was answered `✅ yes`. `Nothing downgrades the boundary for you` names the event instead of the choice, and both single-mode subjects then answer the same way for opposite reasons. Three further labels ran past the six-word guidance and were shortened without changing what they ask.[^labels]

`Default-deny egress` failed the first of those two rules in the other direction: it reads as a statement of what a tool does, and vivarium's binding rule is that egress defaults to open, with one knob to switch. A reader taking the `✅ yes` as a default came away with the opposite of the specification. The method under the row had always asked the reachability question — its first step configures the most restrictive documented posture — so `Egress can be default-deny` names what was already being measured, and no verdict moved. The correction rows above keep the old label, because they record a reading made under it.[^egress-label]

`Roll back to an older environment` named an action with no situation attached, so nothing in it could be verdicted. Both projects that own this idea upstream phrase it as booting a previous state because the current one failed: the NixOS manual's "Rolling Back Configuration Changes" describes booting any previous configuration not yet garbage-collected and says it is especially useful when the new configuration fails to boot, and openSUSE's reference titles the section "System rollback by booting from snapshots" and frames it as recovering a misconfigured system. `Boot a previous build when the new one is broken` says that in the set's own voice; the evidence keeps vivarium's own noun, generation. No verdict moved.

`The tool arranges the workspace mount` was the label that let a reader and the subject's own author reach opposite readings from the same cell. `Arranges` is satisfied by a decision recorded once at registration and replayed at every start, which is exactly what a `krun` registration does; the property the row was built to measure is narrower, that the tool works out which project it is looking at with no host path named by anyone. Those are two capabilities, and the set already had a row for the first — `Choose which host paths cross, in a file rather than on the command line` asks precisely where the decision was written down. `The tool derives the workspace mount, with no host path named` states the second without borrowing the first, and the method under it now registers the subject before it starts it, so registration-time configuration is inside the test rather than ambiguously outside it. No verdict moved at any subject; two of them now say why they are a `❌` where before they only said that they were.

That retitle, and the four before it, retire the second of the two label rules above. A label is now required to state a claim a reader can verdict from the table alone, and length yields to that: `The same definition rebuilds the same environment` replaced a six-word label that said less. The first rule stands unchanged — a label states an observable behavior — and it is the one that was ever doing the work. The correction rows above keep the labels they were recorded under.[^rollback-label]

### Citations corrected

Three `vivarium` cells cited [`open-questions.md`](../../plan/open-questions.md) in prose without naming an entry, and two of those named no entry because none existed. A citation a reader cannot follow is worse than none: it claims a record is being kept and cannot be checked. All three now name a question, and the two missing ones were logged rather than dropped.

| Row                                       | Cell           | Cited                     | Now     |
| ----------------------------------------- | -------------- | ------------------------- | ------- |
| Credentials scoped per tool               | `vivarium` no† | design position, no entry | `Q-031` |
| Stays off a corporate VPN                 | `vivarium` no  | prose, no entry           | `Q-032` |
| Something outside reaches a guest service | `vivarium` no  | prose, no entry           | `Q-033` |

Two of the three evidence sections were rewritten in the same pass, because reading the code to write the question showed the cells had been stating absence where the behavior is something more specific. The VPN cell is the sharper one: the uplink keeps its sockets on the host side, so guest flows are re-originated as host sockets and follow the host routing table, which means a sandbox on a VPN-connected host is on the VPN, and its name lookups go to the host's resolver. Namespace, tap, uplink, and resolver are all per-VM; route selection is the one part the guest does not get its own copy of. The inbound cell moved the other way: nothing is arranged in either direction, and the specification names neither the capability nor a non-goal foreclosing it. No verdict moved — both were already `❌ no`, and both are better supported now.[^citations]

### Verdicts corrected against upstream documentation

One cell in the previous round was marked partial for a reason that names the reader's ignorance rather than the tool's behavior: the mechanism was undocumented at the setup being read. That is a research gap, not a verdict, and it resolves by reading further.

| Row                                       | Was              | Now          | Why                                                                                                                                                                            |
| ----------------------------------------- | ---------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Something outside reaches a guest service | `podman` partial | `podman` yes | Impersonation is documented to carry inbound connections to a listening guest port, and an impersonated listener sits host-side in the namespace podman already publishes into |

The correction is worth stating as a rule. Reading the runtime's own documentation showed the mechanism is not a gap in the publish path but the thing that makes it work unchanged: because a guest listener is a host socket owned by the VMM process, `-p` needs no krun-specific handling, which is the same property that lets a sidecar reach the workload. Two conditions and one unsettled report bound the cell, and [`scenarios/inbound.md`](./scenarios/inbound.md#podman) carries all three rather than the verdict absorbing them.

This also sharpens the row's spread. It is now the one network row where the two microVM tools built as agent sandboxes both say no and the general-purpose runtime says yes, which reads as a posture rather than a shortfall — except that neither sandbox states it as one, and vivarium's silence is [`Q-033`](../../plan/open-questions.md).[^upstream-docs]

### Rows added on a later pass

Five capabilities present in the inventories above had no row. Each was read back out of the sweeps rather than invented for the table, which is the check that they were available all along and were missed: composition and drop-ins are in the `flake-pilot` sweep, hooks and the runtime-only credential stance are in the `glaipnir` one, and module merge, `inputs.toml`, the lockfile, and the SSH and GPG relays are in vivarium's.

| Row                                                        | Theme             | Why it earns a row                                                                                |
| ---------------------------------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| Compose the environment from separate, reusable parts      | Guest environment | All four layer something; only one reports a collision instead of letting a filename settle it    |
| A config unit works unchanged on someone else's machine    | Guest environment | Every subject can share an image; sharing one concern is where they diverge                       |
| Everyone building it gets the versions you got             | Guest environment | Distinct from reproducibility: the input set itself being pinned, and portable to a second person |
| Use an SSH or GPG key without the key entering the sandbox | Data              | The one credential question a shared-kernel answer cannot be borrowed for                         |
| Secrets are kept out of the built artifact                 | Data              | A stance two subjects hold and two do not state at all                                            |

The three `Guest environment` rows are deliberately non-overlapping: same input gives the same output is `The same definition rebuilds the same environment`, the input set is pinned and portable is `Everyone building it gets the versions you got`, and the pin moves only when someone moves it is `Update on purpose`.

One of the five originates outside vivarium, which is worse than the set's running ratio and is recorded rather than smoothed over: adding rows a tool was built to answer is the failure mode this document exists to catch. The counterweight is that no new row is a one-column win — `glaipnir` takes the credential-stance row outright, and the composition and sharing rows are `⚠️ partial` for three subjects rather than `❌ no`.[^rows-added]

### A row added for shipping a secret with the definition

`Secrets are kept out of the built artifact` asks where a secret must not be. It does not ask the question a team hits next, which is where a shared secret then lives, and the two have different answers: vivarium takes the first outright and only the second partially.

Adding the row exposed a gap in §4 above. vivarium's sweep already carried the refusal — `vivarium decrypting, holding an identity, or brokering a login` — so the question was readable out of the inventories on one side and not the other, because podman's sweep never recorded `podman secret create` at all. The sweep bullet was added from the manual page before the row was written, in that order, so the row is read back out of the inventories like the five before it rather than written from the tables down.

| Row                                             | Theme | Why it earns a row                                                                           |
| ----------------------------------------------- | ----- | -------------------------------------------------------------------------------------------- |
| Commit an encrypted secret alongside the config | Data  | Two subjects supply a seam and neither supplies the scheme, which no other row distinguishes |

This is the second added row that originates in vivarium's own sweep, and the counterweight is the same one the section above names: it is not a one-column win. vivarium and podman both land `⚠️ partial` for unrelated reasons — one refuses the integration by rule, the other defaults to an unencrypted driver — and the row's value is that it separates supplying a seam from supplying a scheme.[^shipping-a-secret]

### A row added for where the crossing set is written

Relabelling `Credentials scoped per tool` to `Scopes credentials per app out of the box` exposed what that row had been absorbing. Under the old label it read as though only one subject could scope a credential at all, when every subject can narrow what crosses by declaring or passing less. Once the label said `out of the box`, the ordinary capability underneath it had no row: choosing the crossing set is something all four do, and where that choice is recorded is what separates them.

`Work stays at its host path` does not ask it — that row is about the target a mount lands on, not about who picks the set — and `Defined by a project file` asks where the definition lives without asking what is in it. The new row is the mount twin of `Choose which host environment variables cross`, which had no counterpart for paths.

| Row                                              | Theme | Why it earns a row                                                                                                 |
| ------------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------ |
| Choose which host paths cross, in a project file | Data  | The four record the same decision in four places: a merged project file, a registration, a script, and a call site |

The row is read out of the inventories rather than invented: `[[mounts]]` and module merge are in vivarium's sweep, `include.tar` / `include.path` in flake-pilot's, and `-v` in podman's. The spread is genuine — one `n/a` at the compared boundary, one `no` that is a hardcoded script, and a `partial` that turns on Quadlet recording in a file what the command line otherwise holds.[^crossing-set]

### Two rows added for what goes inside

The `Guest environment` section asked how an artifact is defined, shared, and reproduced, and never what a user can put in it. Every row there was about the mechanism of definition, so a reader could learn that vivarium composes and pins without learning whether they can add a compiler.

Two separate capabilities were hiding in that gap, and they separate the subjects differently, which is why they are two rows rather than one.

| Row                                                                      | Theme             | Why it earns a row                                                                                                  |
| ------------------------------------------------------------------------ | ----------------- | ------------------------------------------------------------------------------------------------------------------- |
| The project's own dev environment loads when you enter                   | Guest environment | One subject makes the inner layer a specified requirement; the rest leave it to whatever the image happens to carry |
| Choose what goes in the guest, and set it up at build and at every start | Guest environment | What a user may put in and execute at each moment, against declaration-only, against one baked-in command           |

Both were first seen elsewhere in part. `PACKAGES=(…)` is in glaipnir's sweep and `Containerfile` build steps are in podman's, while vivarium's sweep recorded module merge without ever recording that a module is where packages are named. The build-and-start hook pair is glaipnir's alone. Only the inner-environment row originates with vivarium, and it too was missing from that sweep — [`spec/06-workspace-and-project-environment.md`](../spec/06-workspace-and-project-environment.md) fixes the two-layer design in normative terms and no bullet carried it, so no row could be derived from it. Both gaps were filled in section 1 before any row here was written.

The build-time row is placed beside `The build runs no user-supplied commands as root` on purpose. The two are the same question asked from opposite sides, and reading them together is what keeps vivarium's `partial` from looking like a shortfall: the half it lacks is the arbitrary root build step the next row reports as refused.[^guest-environment]

### Rows split so one cell states one claim

A reader asked what `Re-enter a running instance` meant for flake-pilot, because its `partial` was answering two questions at once: the instance does survive between calls, and a second session into it does not exist. Those are different mechanisms with different verdicts, and one symbol could only average them.

Sweeping every remaining cell for the same shape produced a test. A row is split when two mechanisms sit under one label, some subject has one without the other, both halves discriminate among subjects, neither half restates a row that already exists, and the resulting row is not one only vivarium was ever going to win. Most hedges failed it. Eight rows passed.

| Split                                                   | Into                                                                                      | What the one symbol was hiding                                                                                                     |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Nothing downgrades the boundary for you                 | No flag selects a weaker boundary; no file you did not write selects one                  | The method already named three readers and the verdict averaged them; flake-pilot fails on the file, glaipnir on the flag          |
| Work stays at its host path                             | The tool arranges the workspace mount; work stays at its host path                        | glaipnir arranges the mount and moves the path; podman keeps the path and arranges nothing — one symbol, opposite failures         |
| Secrets are kept out of the built artifact              | The documented flow keeps them out; keeping them out is enforced                          | glaipnir's stance holds in the code and nothing holds a user to it                                                                 |
| Compose the environment from separate, reusable parts   | Composition; a collision between two parts is reported                                    | Three subjects compose and three resolve collisions differently, including two with nothing to collide                             |
| A config unit works unchanged on someone else's machine | There is a shareable unit smaller than the whole environment; its portability is enforced | flake-pilot has the unit and no check; podman has neither                                                                          |
| Run your own setup at build time and at every start     | At build time; at every start                                                             | The label named two moments and vivarium answers them differently — no script slot at build, declared like any NixOS host at start |
| Re-enter a running instance                             | A later command reaches the instance; a second session joins it                           | flake-pilot keeps the instance with `--resume` and has no in-guest supervisor to hand out a second shell                           |
| See every sandbox on the machine                        | See every definition; see every running instance                                          | flake-pilot enumerates registrations and cannot enumerate instances                                                                |

Two rows that this pass had added were later folded back in. `Choose which programs are installed in the guest` and `Works for a tool the sandbox has never heard of` both asked, from different ends, whether a user decides what the guest is — and every cell in them turned on a mechanism another row already owned. Naming a package is one thing a user does while the guest is built, so it belongs to `Choose what goes in the guest, and set it up at build`; a roster that decides which applications may run at all is glaipnir's built-in opinion, which `Scopes credentials per app out of the box` measures from both ends and now states the cost of. Neither fold moved a verdict that survived it.

Two rows were merged, which is the same principle running backwards. `The same definition rebuilds the same environment` and `Everyone building it gets the versions you got` had near-verbatim evidence for both alternatives — the same tarball-by-URL argument, the same digest-versus-tag argument — because they were one claim wearing two labels. They are now `The same definition gives everyone the same environment`, and vivarium's two mechanisms, the pure build and the single lockfile, are stated in one place because neither is sufficient alone.

Nine hedges were left standing on the test. `Defined by a project file` splits cleanly into whether the definition can live beside the project and whether entering the directory selects it, and the second is a row only vivarium was ever going to win — the failure mode this document already names, where a label exposes a question one design was built to answer. `Update on purpose` splits into whether the environment moves and whether you can tell what you are running, and the second belongs nearer the enumeration rows than to a row of its own.

| Row                                       | Was                                     | Now                                   | Why                                                                                               |
| ----------------------------------------- | --------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Egress can be default-deny                | `flake-pilot` partial, `podman` partial | `flake-pilot` yes, `podman` yes       | Both hedged on readmitting a named destination, which is the next row, where both already read no |
| Choose which host paths cross             | `podman` partial                        | `podman` yes‡, and the row narrowed   | Hedged on travelling with the project and on merging, which are two other rows                    |
| The caller chooses which environment vars | `glaipnir` partial                      | `glaipnir` no, and the row relabelled | An explicit list is a real guarantee and not this row's question; the list is the tool's          |
| Work stays at its host path               | `podman` partial                        | `podman` yes‡                         | `-v /path:/path` is exact; what was missing is arrangement, which the mark now carries            |
| Commit an encrypted secret                | `vivarium` partial, `podman` partial    | `vivarium` yes‡, `podman` yes‡        | Both supply a seam and neither supplies the scheme; the mark says so without averaging it         |
| The same definition                       | `podman` partial                        | `podman` yes‡                         | A digest reproduces exactly and the documented flow is a tag                                      |
| Run your own setup at every start         | `podman` partial                        | `podman` yes‡                         | `ENTRYPOINT` is the mechanism; there being no directory of steps is the composition row           |

Seven of those eleven moved in an alternative's favour and none of the four remaining moved in vivarium's: vivarium gains `‡` on one cell, gives up the `†` it had at build time, and picks up a `partial` on `Keeping secrets out of the build is enforced` that its old cell had absorbed. Twenty-four hedged cells became seven.

The `‡` mark is what made the promotions honest rather than generous. It says the tool provides the mechanism and arranges nothing, and it applies only when the user invokes something the tool offers — not when the user builds the mechanism themselves. That line is why `Something outside reaches a guest service` stays a hedge for flake-pilot: `ip_forward`, a MASQUERADE rule, and a hand-edited `boot_args` are host plumbing an operator assembles, not a flag anyone passes. A mark that turned every no into a qualified yes would be worth nothing.[^rows-split]

### Cut, and why

- Resource ceilings and process limits — every subject has them and the differences are numeric rather than decidable.
- Trusted and untrusted software classification — a real `glaipnir` product idea with no counterpart anywhere else, so a row would be one column wide. It is carried in [`walkthroughs.md`](./walkthroughs.md) instead.
- Snapshot and saved machine state — no subject in this set offers it.
- Systemd unit generation and Quadlet — `podman`-only, and about host integration rather than the sandbox.
- Cache policy per share, PTY sizing, exit-code propagation — implementation detail below the level a reader decides at.

[^merge]: Verified: 2026-08-18 — merged from the four inventories above, at the commits `sources.md` pins.

[^microvm-boundary]: Verified: 2026-08-19 — re-read against the sources `sources.md` pins.

[^podman-setup]: Verified: 2026-08-19 — podman manual pages plus the libkrun project README. The third row is superseded the same day by `Verdicts corrected against upstream documentation` below: the publish path under `krun` is documented, in the README's networking section, and reading it moved the cell to yes.

[^refusals]: Verified: 2026-08-19 — against `08-invariants-and-guarantees.md`, which carries no invariant stating application-agnosticism. That absence is the reason the cell is the set's only `†` citing a design position instead of a rule, and it is logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md). Two evidence sections were added in the same pass, so no cell in the row is a bare symbol.

[^labels]: Verified: 2026-08-19 — audited against the two row rules this document set follows: a label states an observable behavior, and a label is at most six words.

[^egress-label]: Verified: 2026-08-19 — against the egress-defaults-open rule in [`08-invariants-and-guarantees.md`](../spec/08-invariants-and-guarantees.md) and the two egress modes in [`05-networking-and-egress.md`](../spec/05-networking-and-egress.md). Two evidence sections were added in the same pass: the `vivarium` cells in both network-policy rows were the set's only unlinked `✅ yes` on rows where the other subjects carry evidence, and they were the cells where the surprise lived.

[^rollback-label]: Verified: 2026-08-19 — the two upstream phrasings read at <https://nlewo.github.io/nixos-manual-sphinx/administration/rollback.xml.html> and <https://doc.opensuse.org/documentation/leap/reference/html/book-reference/cha-snapper.html>.

[^citations]: Verified: 2026-08-19 — read against [`spec/05-networking-and-egress.md`](../spec/05-networking-and-egress.md), [`spec/00-goals-and-non-goals.md`](../spec/00-goals-and-non-goals.md), and the uplink the launcher builds in [`src/launch/policy.rs`](../../../src/launch/policy.rs).

[^upstream-docs]: Verified: 2026-08-19 — read, not run, against the libkrun project README's networking section, the `krun` manual page in `crun` for the `krun.use_passt` annotation, podman issue `25494` for the `AF_INET6` report, and a published `--annotation=run.oci.handler=krun -dp 8080:8080` run load-tested from the host.

[^rows-added]: Verified: 2026-08-19 — derived from §1 to §4 above, at the commits `sources.md` pins.

[^shipping-a-secret]: Verified: 2026-08-20 — vivarium against [`spec/07-secrets-and-config-sharing.md`](../spec/07-secrets-and-config-sharing.md) and `ADR-0072`; flake-pilot against a fresh clone at `44e3ab2`; glaipnir against a fresh clone at `8c7420e`, a later revision than this document's pin, recorded as such in the evidence; podman against the `podman-secret-create` manual page. No existing verdict moved.

[^crossing-set]: Verified: 2026-08-20 — vivarium against [`spec/07-secrets-and-config-sharing.md`](../spec/07-secrets-and-config-sharing.md) and [`spec/08-invariants-and-guarantees.md`](../spec/08-invariants-and-guarantees.md); flake-pilot against a fresh clone at `44e3ab2`; glaipnir against `_bind_agent_mounts` in a fresh clone at `8c7420e`; podman against the `podman-systemd.unit` manual page for `Volume=`. No existing verdict moved.

[^guest-environment]: Verified: 2026-08-20 — vivarium against [`spec/03-artifact-model.md`](../spec/03-artifact-model.md) for the manifest key table and [`spec/06-workspace-and-project-environment.md`](../spec/06-workspace-and-project-environment.md) for the inner layer, and against [`implementation-status.md`](../implementation-status.md) and the shipped guest module for the `*`; flake-pilot against the `sci` and `flake-ctl-firecracker-register` manual pages in a fresh clone at `44e3ab2`; glaipnir against `image/Containerfile`, `image/scripts/entrypoint.sh`, and `docs/overview.md` at `21ef389`; podman against its manual pages. No existing verdict moved.

[^rows-split]: Verified: 2026-08-20 — every split and every promotion re-read against the evidence already recorded in [`scenarios/`](./scenarios/README.md) at the revisions [`sources.md`](./sources.md) pins, with no subject re-read for this pass. The correction tables above keep the row labels they were written with; a label frozen in a dated record is what makes the record readable later, and the inventory above is where the current set lives.

[^include-destination]: Verified 2026-08-22, against a fresh clone at `920f41e`: `sync_includes` and the overlay assembly in `firecracker-pilot/src/firecracker.rs`, `sync_includes` and `mount_container` in `podman-pilot/src/podman.rs`, and the absence of any `build` or `commit` call in either pilot.

[^both-routes]: Verified 2026-08-21, against a fresh clone at `920f41e` and the upstream libkrun, `crun`, and passt documentation named in [`sources.md`](./sources.md).
