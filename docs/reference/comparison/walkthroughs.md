# Walkthroughs

Two jobs, done four ways, start to finish. Every other question these tools answer is decided one row at a time, and [`scenarios/`](./scenarios/README.md) is where those live, each with its method, its evidence, and a worked example where the command is what the row turns on. What survives here is what no single row owns: a sequence, where the cost of a choice made in step one only becomes legible in step three.

Commands are transcribed from each project's own material; see [`sources.md`](./sources.md). Where vivarium's answer is specified rather than built it is marked `*`, and no command is shown that would refuse.

Both jobs run at the fixed setups [`README.md`](./README.md)'s methodology names: vivarium's one microVM, both of flake-pilot's microVM rungs, glaipnir's libkrun microVM, and `podman run --runtime krun`. Container rungs appear only where they teach something, and are named as container rungs when they do, because most of what these tools do comfortably they do with the host kernel.

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

The same switch can be made once for every registration instead, by setting `runtime = "krun"` under `[engine]` in `containers.conf` — which is [where a file rather than a flag reaches the boundary](./scenarios/boundary-file.md#podman).

Rung 3, a firecracker microVM with its own storage overlay:

```bash
flake-ctl firecracker --user pull --name claude \
    --kis-image https://github.com/OSInside/flake-pilot/raw/refs/heads/main/appstore/firecracker/claude.x86_64-1.15.6-0.tar.xz

flake-ctl firecracker --user register --vm claude \
    --app $HOME/bin/claude --target /bin/bash \
    --overlay-size 20GiB --force-vsock --resume
```

Note what rung 3 does not carry: there is no `--volume`, because the firecracker schema has no bind mount. The guest is the image plus a 20 GiB ext2 overlay, and host files reach it only as `--include-path` or `--include-tar` copies made at provisioning. The strongest rung is also the one where the agent stops looking at your project and starts looking at [a copy of it](./scenarios/workspace-mount.md#flake-pilot) — while rung 2, with a kernel of its own too, keeps the directory in view and gives back re-entry instead. Rungs 2 and 3 are the two this comparison reads, and choosing between them is choosing what to lose.

Afterwards the user types `claude`. The sandbox is invisible: the registered app is a symlink to a pilot binary that reads its own `argv[0]`.

### glaipnir: one command, boundary chosen by the host

```bash
sudo zypper install podman passt crun libkrun1 libkrunfw5
sudo usermod -aG kvm $USER
sudo zypper install glaipnir

cd ~/projects/my-thing
glaipnir run claude
```

That one command probes the host, offers to add the missing group and re-executes itself under `sg`, finds a non-VPN egress interface or aborts, pulls the prebuilt per-agent image, creates the credential directories, and runs. A failed probe [proceeds in a plain container with a warning](./scenarios/no-kvm.md#glaipnir) rather than refusing.

On a host that passes, the container name gains a `-microvm` suffix — and a second `glaipnir run claude` on the same project [cannot rejoin it](./scenarios/later-command.md#glaipnir). The script counts what exists and starts a numbered sibling instead.

### podman: assemble it yourself

```bash
cd ~/projects/my-thing
podman run --rm -it --runtime krun \
  -v "$PWD:$PWD" -w "$PWD" \
  --cap-drop ALL --security-opt no-new-privileges \
  --userns keep-id --pids-limit 1024 \
  docker.io/library/node:22 bash
```

`--runtime krun` is what makes it a microVM, and it is also the first flag forgotten. Nothing here is wrong, and nothing here is remembered. The next project is another line of shell.

### vivarium: bind the project once

```bash
cd ~/projects/my-thing
viv init --manifest rust-web --write
viv start
viv shell
```

The project tree is at the absolute path it occupies on the host, so `git`, editors, and linked worktrees resolve from either side. There is no quarantine directory to copy work into, and no level to choose: vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes one boundary and no other.

### What the difference costs

- flake-pilot: most flexible, least self-describing — three registrations per agent, two of them behind a kernel of their own, and the isolation strength lives in shell history rather than in the project.
- glaipnir: fastest path to a working sandbox, bought by deciding the boundary for you.
- podman: everything is possible, nothing is remembered.
- vivarium: slowest to first run, and the only one where "what is this environment" is a file you can read. Against either microVM rung it is a fair fight, and the two rungs lose it differently: rung 3 boots a kernel and leaves the work behind as a copy, rung 2 boots a kernel and keeps the work in view but at a directory somebody typed once, which does not follow the next project.

## W2 — Get the same environment back next month

Four rows touch this — [the same definition](./scenarios/same-definition.md), [update on purpose](./scenarios/update.md), [rollback](./scenarios/rollback.md), and [what a shared unit pins](./scenarios/portability-enforced.md) — and none of them asks the question a user actually has, which is what they are holding when they come back.

### flake-pilot

At both podman rungs — rung 1 and the `krun` rung this comparison reads — the unit is a `:latest` tag on a public ECR registry rebuilt daily, and `%remove` makes the next call re-check it, so the enclosure moves by default. At the firecracker rung it cannot: `pull` fetches a versioned artifact by URL — `claude.x86_64-1.15.6-0.tar.xz` — into `/var/lib/firecracker/images/<name>/`, and the registration then names local file paths for the rootfs and kernel. There is no registry left to re-check. So the two rungs with their own kernel answer this walkthrough oppositely, and the stronger-sounding one is the one that holds still.

What it does not have is a way back or a way to re-derive. A description can exist — upstream's example VM is a KIWI file anyone can copy and rebuild — but it does not pin: its `config.sh` installs the agent through `npm install -g` and a piped vendor installer, against repository URLs that carry no version, so the same file rebuilt next month produces a different system. The tarball is therefore the unit, a second machine gets the same environment only by fetching the same URL and trusting it, and yesterday's image is gone once you overwrite it.

### glaipnir

`Containerfile.agent` starts `FROM` a per-agent image at `:latest` on an OBS registry, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and this is the same at both rungs — the microVM changes the kernel, not the image.

### podman

Pinning a digest works and nothing arranges it. A `Containerfile`'s `RUN` steps re-execute against whatever the network serves that day.

### vivarium

```bash
viv config eval          # the merged view, with provenance
viv config sources       # which layer set what
```

The manifest plus the lockfile the first evaluation writes is the unit, and the [pure-build rule](../spec/08-invariants-and-guarantees.md) fixes that the same closure and lock evaluate to the same store output on any machine at any later time. Updating is a verb rather than a default.

Getting an older environment back is specified and not built: `viv generations list`\*, `viv generations rollback`\*, and `viv start --generation <n>`\* are what [`spec/11`](../spec/11-generations-and-build-history.md) fixes, with each retained generation pinned by a GC root so an ordinary store collection cannot eat the history. The one alternative that answers at all is podman, and it answers by not collecting: the image the old tag pointed at is still on disk, untagged, until [a prune removes it](./scenarios/rollback.md#podman).

### What the difference costs

Not the clean sweep the container rungs suggest. The difference that survives is what you hold:

- flake-pilot holds a tarball — it reaches "it will not change until I say so" by having no mechanism for changing, a real answer arrived at from the other side.
- podman holds a digest, if you remembered to write one down.
- glaipnir holds a `Containerfile` whose inputs move without notice.
- vivarium holds a manifest and a lock — a definition rather than a copy, which is why it is the only one here where getting last month's back could be specified at all.

## One idea worth stealing, with no row of its own

glaipnir classifies the software inside the sandbox. Five agents are trusted; `hermes-agent` is not, and naming it for a `build` or a `run` triggers an interactive disclaimer that exits on anything but yes — while `clean` and `status` pass without prompting, so the classification costs nothing until it would matter.

No other tool in this set has anything like it, which is why it is not a table row. vivarium's nearest surface classifies artifacts by layer, shared against personal, rather than classifying the software those artifacts carry.
