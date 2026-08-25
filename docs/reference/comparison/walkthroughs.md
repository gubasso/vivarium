# Walkthroughs

Two jobs, done four ways, start to finish. Every other question these tools answer is decided one row at a time, and [`scenarios/`](./scenarios/README.md) is where those live, each with its method, its evidence, and a worked example where the command is what the row turns on. What survives here is what no single row owns: a sequence, where the cost of a choice made in step one only becomes legible in step three.

Commands are transcribed from each project's own material; see [`sources.md`](./sources.md). Where vivarium's answer is specified rather than built it is marked `*`, and no command is shown that would refuse.

Both jobs run at the fixed setups [`methodology.md`](./methodology.md) names: vivarium's one microVM, both of flake-pilot's microVM rungs, glaipnir's libkrun microVM, and bunkerbox's one Kata container. Container rungs appear only where they teach something, and are named as container rungs when they do, because most of what these tools do comfortably they do with the host kernel.

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

The same switch can be made once for every registration instead, by setting `runtime = "krun"` under `[engine]` in `containers.conf` — which is [where a file rather than a flag reaches the boundary](./scenarios/boundary-file.md#flake-pilot).

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

### bunkerbox: install the tool, then type its name

```bash
sudo bunkerbox setup          # Ubuntu 22.04 or 24.04, x86_64
make image IMAGE=images/opencode.conf
make install-image OCI=bunkerbox-opencode-1.17.18.oci

cd ~/projects/my-thing
opencode
```

The command is a symlink to the bunkerbox binary, so the last line is the whole of the daily experience: no sandbox is named, no path is typed, and nothing about the project is configured first. bunkerbox resolves the repository root from the working directory, mounts it at `/workspace` over a capped copy-on-write overlay, prepares the tool's own home, applies the packaged network policy, and boots a Kata guest.

What that first run also does is write `.bunkerbox/project.conf` into the repository, with a `passthrough` whitelist auto-detected from the build-system files it finds and `profiles` left empty. From then on the agent can call `cargo` and `make`, and they run on the host — [inside a bubblewrap sandbox once profiles are configured, and unwrapped until then](./scenarios/build-inside-boundary.md#bunkerbox).

### vivarium: declare the workspace once

```bash
cd ~/projects/my-thing
viv start
viv shell
```

The manifest names the tree, because a workspace is declared rather than inferred — `[[workspaces]] source = "${HOME}/projects/my-thing"` — and that declaration is also what lets one manifest carry a family of related repositories instead of one. Binding says which manifest applies here; the manifest says which trees it owns.

The project tree is at the absolute path it occupies on the host, so `git`, editors, and linked worktrees resolve from either side. There is no quarantine directory to copy work into, and no level to choose: vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes one boundary and no other.

### What the difference costs

- flake-pilot: most flexible, least self-describing — three registrations per agent, two of them behind a kernel of their own, and the isolation strength lives in shell history rather than in the project.
- glaipnir: fastest path to a working sandbox, bought by deciding the boundary for you.
- bunkerbox: the best daily ergonomics in the set — you type the agent's name — bought twice over, once with a host setup nobody would call one command, and once with a first run that writes a policy file you did not open.
- vivarium: slowest to first run, and the only one where "what is this environment" is a file you can read. Against either microVM rung it is a fair fight, and the two rungs lose it differently: rung 3 boots a kernel and leaves the work behind as a copy, rung 2 boots a kernel and keeps the work in view but at a directory somebody typed once. vivarium types a directory too, and the difference is where: the tree is named in the project's own definition beside everything else the environment is made of, and one manifest may name several, rather than in a system file keyed by a registered command name.

## W2 — Get the same environment back next month

Four rows touch this — [the same definition](./scenarios/same-definition.md), [update on purpose](./scenarios/update.md), [rollback](./scenarios/rollback.md), and [what a shared unit pins](./scenarios/portability-enforced.md) — and none of them asks the question a user actually has, which is what they are holding when they come back.

### flake-pilot

At both podman rungs — rung 1 and the `krun` rung this comparison reads — the unit is a `:latest` tag on a public ECR registry rebuilt daily, and `%remove` makes the next call re-check it, so the enclosure moves by default. At the firecracker rung it cannot: `pull` fetches a versioned artifact by URL — `claude.x86_64-1.15.6-0.tar.xz` — into `/var/lib/firecracker/images/<name>/`, and the registration then names local file paths for the rootfs and kernel. There is no registry left to re-check. So the two rungs with their own kernel answer this walkthrough oppositely, and the stronger-sounding one is the one that holds still.

What it does not have is a way back or a way to re-derive. A description can exist — upstream's example VM is a KIWI file anyone can copy and rebuild — but it does not pin: its `config.sh` installs the agent through `npm install -g` and a piped vendor installer, against repository URLs that carry no version, so the same file rebuilt next month produces a different system. The tarball is therefore the unit, a second machine gets the same environment only by fetching the same URL and trusting it, and yesterday's image is gone once you overwrite it.

### glaipnir

`Containerfile.agent` starts `FROM` a per-agent image at `:latest` on an OBS registry, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and this is the same at both rungs — the microVM changes the kernel, not the image.

### bunkerbox

The unit is an OCI archive on disk, so nothing moves until a new package is installed — the same not-a-mechanism answer flake-pilot's firecracker rung reaches. Re-deriving it is where that stops: the image config is a `containerfile` over `alpine:3.22` running `apk add --no-cache` and curling a release tarball, so the same file rebuilt next month builds a different system. One version is pinned, in `build_args`, and it pins the agent rather than the environment around it.

### vivarium

```bash
viv config eval          # the merged view, with provenance
viv config sources       # which layer set what
```

The manifest plus the lockfile the first evaluation writes is the unit, and the [pure-build rule](../spec/08-invariants-and-guarantees.md) fixes that the same closure and lock evaluate to the same store output on any machine at any later time. Updating is a verb rather than a default.

Getting an older environment back is specified and not built: `viv generations list`\*, `viv generations rollback`\*, and `viv start --generation <n>`\* are what [`spec/11`](../spec/11-generations-and-build-history.md) fixes, with each retained generation pinned by a GC root so an ordinary store collection cannot eat the history. No alternative in this set answers it at all: [every other column reads `❌`](./scenarios/rollback.md), and the nearest thing to a way back is an old archive that happens to still be on disk under its versioned filename.

### What the difference costs

Not the clean sweep the container rungs suggest. The difference that survives is what you hold:

- flake-pilot holds a tarball — it reaches "it will not change until I say so" by having no mechanism for changing, a real answer arrived at from the other side.
- bunkerbox holds an OCI archive, reaching the same answer the same way, and a recipe that will not rebuild it.
- glaipnir holds a `Containerfile` whose inputs move without notice.
- vivarium holds a manifest and a lock — a definition rather than a copy, which is why it is the only one here where getting last month's back could be specified at all.

## One idea worth stealing

glaipnir classifies the software inside the sandbox. Five agents are trusted; `hermes-agent` is not, and naming it for a `build` or a `run` triggers an interactive disclaimer that exits on anything but yes — while `clean` and `status` pass without prompting, so the classification costs nothing until it would matter.

No other tool in this set has anything like it, which is why it lived here for a while instead of in the tables: a row one column wide did not qualify. It does now, and the detail belongs to [its own scenario](./scenarios/trust-classification.md). What stays worth saying here is why it is worth stealing rather than merely worth scoring — vivarium's nearest surface classifies artifacts by layer, shared against personal, and has no opinion at all about the software those artifacts carry.
