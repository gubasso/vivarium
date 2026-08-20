# Walkthroughs

The same five jobs, done four ways, start to finish. Commands are transcribed from each project's own material; see [`sources.md`](./sources.md). Where vivarium's answer is specified rather than built it is marked `*`, and no command is shown that would refuse.

Every job runs at the fixed setup [`README.md`](./README.md)'s methodology names: vivarium's one microVM, flake-pilot's firecracker rung, glaipnir's libkrun microVM, and `podman run --runtime krun`. Container rungs appear only where they teach something, and are named as container rungs when they do, because most of what these tools do comfortably they do with the host kernel.

## W1 — Point an agent at a real project

This is the job every one of these tools exists for, and the four answers differ before a single agent has run.

### flake-pilot: pick an isolation level, then register

Upstream registers the same agent three ways, at three strengths, and all three claim the same app name — they are alternatives rather than a ladder that coexists. Commands are transcribed from the upstream `README.md`. Preparation is shared:

```bash
mkdir -p ~/ai
export PATH=$PATH:$HOME/bin
```

Rung 1, podman at its default runtime: shared kernel, host network, data scope `~/ai`:

```bash
flake-ctl podman --user register \
    --app $HOME/bin/claude \
    --target /bin/bash \
    --container public.ecr.aws/b9k1j9y6/ai/claude:latest \
    --resume \
    --opt "\--net host" \
    --opt "\-ti" \
    --opt "\--workdir %HOME/ai" \
    --opt "\--volume %HOME/ai:%HOME/ai" \
    --opt "\-e HOME=%HOME"
```

Rung 2, own kernel through `krun`. The registration differs by one option and loses `--resume`, and upstream gives the reason: libkrun runs the workload inside an isolated microVM with no in-guest agent to spawn and inject a secondary process, so `exec` does not work and a resumed instance cannot be re-entered:

```bash
flake-ctl podman --user register \
    --app $HOME/bin/claude \
    --target /bin/bash \
    --container public.ecr.aws/b9k1j9y6/ai/claude:latest \
    --opt "\--net host" \
    --opt "\--runtime=krun" \
    --opt "\-ti" \
    --opt "\--workdir %HOME/ai" \
    --opt "\--volume %HOME/ai:%HOME/ai" \
    --opt "\-e HOME=%HOME"
```

The same switch can be made once for every registration instead, by setting `runtime = "krun"` under `[engine]` in `containers.conf`.

Rung 3, a firecracker microVM with its own storage overlay:

```bash
flake-ctl firecracker --user pull --name claude \
    --kis-image https://github.com/OSInside/flake-pilot/raw/refs/heads/main/appstore/firecracker/claude.x86_64-1.15.6-0.tar.xz

flake-ctl firecracker --user register --vm claude \
    --app $HOME/bin/claude --target /bin/bash \
    --overlay-size 20GiB --force-vsock --resume
```

Note what rung 3 does not carry: there is no `--volume`, because the firecracker schema has no bind mount. The guest is the image plus a 20 GiB ext2 overlay, and host files reach it only as `--include-path` or `--include-tar` copies made at provisioning. The rung with its own kernel is also the rung where the agent stops looking at your project and starts looking at a copy of it.

Afterwards the user types `claude`. The sandbox is invisible: the registered app is a symlink to a pilot binary that reads its own `argv[0]`.

### glaipnir: one command, boundary chosen by the host

```bash
sudo zypper install podman passt crun libkrun1 libkrunfw5
sudo usermod -aG kvm $USER
sudo zypper install glaipnir

cd ~/projects/my-thing
glaipnir run claude
```

That one command probes for `krun`, `libkrun` above 1.18.0, `/dev/kvm`, and `kvm` group membership; offers to add the group and re-executes itself under `sg`; finds a non-VPN egress interface or aborts; pulls the prebuilt per-agent image; creates the credential directories; and runs. If any capability check fails it proceeds in a plain container with a warning.

On a host that passes the probe, the container name gains a `-microvm` suffix — and a second `glaipnir run claude` on the same project cannot rejoin it, because a krun guest has no in-guest agent to exec into. The script counts what exists and starts a numbered sibling instead.

### podman: assemble it yourself

```bash
cd ~/projects/my-thing
podman run --rm -it --runtime krun \
  -v "$PWD:$PWD" -w "$PWD" \
  --cap-drop ALL --security-opt no-new-privileges \
  --userns keep-id --pids-limit 1024 \
  docker.io/library/node:22 bash
```

`--runtime krun` is what makes it a microVM — it needs libkrun and `/dev/kvm`, and it is also the first flag forgotten. Nothing here is wrong, and nothing here is remembered. The next project is another line of shell.

### vivarium: bind the project once

```bash
cd ~/projects/my-thing
viv init --manifest rust-web --write
viv start
viv shell
```

The project tree is at the absolute path it occupies on the host, so `git`, editors, and linked worktrees resolve from either side. There is no quarantine directory to copy work into, and no level to choose: vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes one boundary and no other.

### What the difference costs

- flake-pilot: most flexible, least self-describing — three registrations per agent, and the isolation strength lives in shell history rather than in the project.
- glaipnir: fastest path to a working sandbox, bought by deciding the boundary for you.
- podman: everything is possible, nothing is remembered.
- vivarium: slowest to first run, and the only one where "what is this environment" is a file you can read. Against the firecracker rung it is a fair fight — both boot a kernel, and only one still has the project tree at its own path afterwards.

## W2 — Stop it reaching the whole internet

### flake-pilot

Two rungs, opposite postures. The container rungs publish `--opt "\--net host"`, which is the opposite of a restriction — and the `krun` rung carries it too, so a rung with its own kernel still shares the host's network namespace.

The firecracker rung inverts it. Upstream states that firecracker "supports networking only through TUN/TAP devices" and that "it is the user's responsibility to set up the routing on the host", and `flake-ctl firecracker register` has a `--no-net` flag to disable networking outright. Turning it on is the operator's job: enable `ip_forward`, MASQUERADE on the outgoing interface, hand-edit `boot_args` in `/usr/share/flakes/<app>.yaml` to replace `ip=dhcp` with a static triple, and create and address a TAP device per instance. Restrictive by default, and by absence rather than by policy — there is no posture that denies and then readmits a name.

### glaipnir

Automatic on Linux, and answering a different question:

```text
--network pasta:--outbound-if4,<first non-VPN default-route interface>
```

The agent cannot reach internal company networks and its requests do not carry the corporation's identity. The public internet stays fully open. This one is rung-independent: `pasta` is podman's network, and the krun guest rides it unchanged.

### podman

```bash
podman run --runtime krun --network none ...
```

All or nothing, and readmitting named destinations needs a firewall outside podman. The posture is podman's, outside the OCI runtime, so it holds under `krun`.

### vivarium

```toml
[egress]
mode = "allowlist"
allow = ["api.anthropic.com", "registry.npmjs.org"]
```

Default-deny is in the kernel before any packet path exists, and the guest's only DNS is a gating resolver that releases an answer after installing its addresses with the record's TTL. A denied name answers `REFUSED`, distinguishably from a name that does not exist. An empty `allow` list is an air-gapped run.

### What the difference costs

- glaipnir asks which host interface the traffic leaves by; vivarium asks which destinations may be reached at all. Both are worth having.
- vivarium has the mechanism for glaipnir's question — a per-VM namespace and an unprivileged uplink — and no knob that selects a host interface.

## W3 — Give it one credential and not the keyring

### glaipnir

Authenticate inside; the token lands in a host cache the tool owns:

```bash
glaipnir run claude
#   → claude auth login
#   → persists to ~/.cache/glaipnir/agents-mount/.claude/
```

Because `_bind_agent_mounts` emits only what the named agent needs, `run claude` never mounts the `gh` token at all. The mounts are podman volumes, so they hold at both rungs — the krun guest sees them through virtiofs.

### flake-pilot

At the container rung the credential arrives by sharing a path and an environment variable, and `--resume` is what keeps the resulting login alive between calls:

```bash
export ANTHROPIC_VERTEX_PROJECT_ID=vertex-ai-206179
gcloud auth application-default login --project $ANTHROPIC_VERTEX_PROJECT_ID
```

Neither half survives the climb intact. `krun` has no resume, so a login done inside is gone by the next call. Firecracker resumes over the vsock and keeps it — but has no bind mount, so a credential that already exists on the host arrives only as an `--include-path` copy fixed at provisioning, which is a copy of the secret rather than a view of it.

### vivarium

An existing host credential crosses as a declared mount, on its own confined daemon:

```toml
[[mounts]]
source = "${HOME}/.config/gcloud"
target = "~/.config/gcloud"
readonly = true
```

vivarium never performs or brokers the login, holds no identity, and decrypts nothing. A source resolving to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` — or any ancestor — is refused before boot. For SSH and GPG the key itself never enters the guest: a relay over a second vsock port delivers the authority instead.

### What the difference costs

- glaipnir's per-agent scoping is finer than any general-purpose sandbox offers; it works because the tool knows which agent needs which path.
- vivarium's mounts are per manifest, so narrowing means the user declaring less — exactly the kind of thing users get wrong. A real gap, no decision taken on closing it.

## W4 — Run three projects at once

### flake-pilot

Instance identity is a call-time suffix, and for firecracker it also names the TAP device, so each extra instance is another device to create and address:

```bash
claude @projA
claude @projB
```

### glaipnir

One cache directory, one container per agent. Two projects share the credential tree; the workspace path is what separates their agent session state. At the microVM rung the count is what changes: each `run` that cannot rejoin starts another numbered sibling VM, so three projects worked on across a day leave more machines than you started them.

### podman

```bash
podman ps -a
podman stop -a
```

Enumeration and mass control are free — `podman ps -a` lists krun containers like any other. What is gone at that runtime is `podman exec` into them, and the per-project setup is still yours to retype.

### vivarium

Identity is the project directory, anchored by a marker so it survives a rename:

```bash
cd ~/projects/a && viv start
cd ~/projects/b && viv start
viv status
```

Each VM gets its own namespace pair, tap, and uplink, set up by `viv start` — no `iptables` rule, no `ip tuntap add`, no per-instance bookkeeping. Guests read the host store read-only and no per-VM store image is built, so the marginal cost of the third sandbox is small.

Machine-wide, vivarium is behind: `viv status -g`\* would enumerate every project and `viv stop
--all`\* would sweep them, and neither runs. `flake-ctl list`, `glaipnir status`, and `podman ps -a` all do this today.

## W5 — Get the same environment back next month

### flake-pilot

The two rungs answer this differently, and the microVM rung answers it better. At the container rung the unit is a `:latest` tag on a public ECR registry rebuilt daily, and `%remove` makes the next call re-check it, so the enclosure moves by default.

At the firecracker rung it cannot. `flake-ctl firecracker pull` fetches a versioned artifact by URL — `claude.x86_64-1.15.6-0.tar.xz` — into `/var/lib/firecracker/images/<name>/`, and the registration then names local file paths for the rootfs and kernel. There is no registry left to re-check. Moving to a newer image is `pull --force`, which is a thing a person does.

What it does not have is a way back or a way to re-derive: the tarball is the unit, so a second machine gets the same environment only by fetching the same URL and trusting it, and yesterday's image is gone once you overwrite it.

### glaipnir

`Containerfile.agent` starts `FROM` a per-agent image at `:latest` on an OBS registry, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and this is the same at both rungs — the microVM changes the kernel, not the image.

### podman

Pinning a digest works and nothing arranges it. A `Containerfile`'s `RUN` steps re-execute against whatever the network serves that day.

### vivarium

```bash
viv config eval          # the merged view, with provenance
viv config sources       # which layer set what
```

The manifest plus the lockfile the first evaluation writes is the unit, and vivarium's [pure-build rule](../spec/08-invariants-and-guarantees.md) fixes that the same closure and lock evaluate to the same store output on any machine at any later time. Updating is a verb rather than a default.

Getting an older environment back is specified and not built: `viv generations list`\*, `viv generations rollback`\*, and `viv start --generation <n>`\* are what `spec/11` fixes, with each retained generation pinned by a GC root so an ordinary store collection cannot eat the history. No alternative in this set has any answer to this at all.

### What the difference costs

Not the clean sweep the container rungs suggest. The difference that survives is what you hold:

- flake-pilot holds a tarball — it reaches "it will not change until I say so" by having no mechanism for changing, a real answer arrived at from the other side.
- podman holds a digest, if you remembered to write one down.
- glaipnir holds a `Containerfile` whose inputs move without notice.
- vivarium holds a manifest and a lock — a definition rather than a copy, which is why it is the only one here where getting last month's back could be specified at all.

## One idea worth stealing, with no row of its own

glaipnir classifies the software inside the sandbox. Five agents are trusted; `hermes-agent` is not, and naming it for a `build` or a `run` triggers an interactive disclaimer that exits on anything but yes — while `clean` and `status` pass without prompting, so the classification costs nothing until it would matter.

No other tool in this set has anything like it, which is why it is not a table row. vivarium's nearest surface classifies artifacts by layer, shared against personal, rather than classifying the software those artifacts carry.
