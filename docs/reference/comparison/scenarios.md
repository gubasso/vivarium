# Scenarios

The method for every row in [`README.md`](./README.md), and the evidence behind every verdict that needed more than its symbol. A method is identical for all four subjects; an evidence section says what one of them did.

Every evidence section is read at the subject's fixed setup:

- `vivarium` — its one microVM
- `flake-pilot` — `firecracker-pilot`, at the upstream `claude` firecracker registration
- `glaipnir` — the libkrun microVM
- `podman` — `podman run --runtime krun`

The first seven sections back the `Isolation backends` table; the rest back the capability tables.

## Own kernel

Record whether the tool can put the workload behind a kernel of its own, by any documented route.

1. Read the tool's own documentation for a mode that boots a guest kernel.
2. Start the workload through that mode.
3. Inside, run `uname -r` and record the value.
4. On the host, run `uname -r` and record the value.
5. Record every host prerequisite that mode required.

### Own kernel, flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Yes, two routes: `podman-pilot` with podman's runtime set to `krun`, and `firecracker-pilot`. The route is fixed at registration, not at run time. The compared setup is the upstream `claude` firecracker registration.

### Own kernel, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: a libkrun microVM, gated by `_check_microvm` on `/usr/bin/krun`, `libkrun.so.1` above 1.18.0, `/dev/kvm`, and `kvm` group membership. `copilot` and `opencode` are held out of it unconditionally by a known libkrun vsock bug.

### Own kernel, podman

podman 5.x, read 2026-08-19. Yes: `--runtime krun` runs the container as a libkrun microVM, with libkrun installed and `/dev/kvm` accessible as prerequisites. The guest kernel comes from libkrunfw, not from the image — the runtime decides the kernel, the image decides the userland.

## Shared-kernel isolation with an OCI runtime

Record whether the tool offers a mode where the workload is separated by namespaces and cgroups on the host kernel rather than by a kernel of its own.

1. Read the tool's engine or runtime list for an ordinary OCI runtime.
2. Start the workload through it.
3. Inside, run `uname -r` and compare it with the host value.
4. Record what the tool itself says about the isolation this mode gives.

### Shared kernel, vivarium

vivarium `ceb0027`, 2026-08-18. No, by rule: vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes a hardware-virtualization boundary with no shared-kernel mode. Not planned — a second, weaker mode would turn the guarantee into a default.

### Shared kernel, flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Yes: `podman-pilot` at podman's default runtime is the upstream README's first `claude` registration, and its cost is legible in the registration itself — a shared kernel, `--net host`, and `~/ai` as the one shared path. It is also the rung with the best ergonomics, and upstream says why: the `krun` handler does not support `exec`, so a `krun` registration cannot use `--resume` either.

### Shared kernel, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: rootless podman is the baseline and the microVM is an upgrade on top. `--no-microvm` selects the baseline outright; a failed probe lands on it with a warning; on macOS it is the only mode.

## Choose the engine

Record whether the user can select which virtualization or container engine runs the workload.

1. Read the registration or manifest schema for an engine key.
2. Change it to a second engine and re-run.
3. Record whether the workload starts and what reports the change.

### Engine choice, vivarium

vivarium `ceb0027`, 2026-08-18. Nobody chooses: vivarium's [class-not-tool rule](../spec/08-invariants-and-guarantees.md) fixes the boundary by capability class, so any backend satisfying the class is admissible and none is part of the contract. There is no manifest key to set. Not planned — the question does not mean anything for this design.

### Engine choice, flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. The user chooses, at registration: `podman-pilot` drives podman, whose OCI runtime is selected by `--opt "\--runtime=krun"` or by `runtime = "krun"` in `containers.conf`, so the reachable set is whatever podman accepts; `firecracker-pilot` drives firecracker. Upstream states the consequence directly — the `krun` runtime "gives isolation based on KVM and should be preferred for AI workloads" — so the rungs are not equivalent and the registration is where the difference is fixed.

### Engine choice, glaipnir

glaipnir `21ef389`, read 2026-08-19. The host probe chooses. There is no engine key: a passing `_check_microvm` selects libkrun, `--no-microvm` opts down to the container, and nothing selects among engines.

## Runs on a host without KVM

Record what happens on a host with no `/dev/kvm`.

1. Confirm `/dev/kvm` is absent or unreadable.
2. Run the tool's default invocation.
3. Record the exit status and the message.

### No KVM, vivarium

vivarium `ceb0027`, 2026-08-18. Refuses: the [separate-kernel rule](../spec/08-invariants-and-guarantees.md) admits no shared-kernel mode, so a host without KVM is a host vivarium does not run on. `viv doctor` reports it as a hard failure and `viv start`'s preflight consumes the same probe. Not planned — a fallback would make the boundary a default rather than a guarantee.

### No KVM, flake-pilot

flake-pilot `main`, read 2026-08-19. The microVM registrations do not start: firecracker and `krun` both require `/dev/kvm`. Only `crun` container registrations run. Nothing falls back — a registration names one engine, and its failure is a failure.

### No KVM, glaipnir

glaipnir `21ef389`, read 2026-08-19. Falls back: a failed `_check_microvm` proceeds in a plain container with a warning. The stance is stated — a weaker sandbox beats no sandbox.

### No KVM, podman

podman 5.x, read 2026-08-19. `--runtime krun` cannot create the VM without `/dev/kvm`; the default `crun` runtime runs anywhere. Nothing falls back on its own — the failing flag is the user's to remove.

## No flag selects a weaker boundary

A boundary is chosen once and used for months, on a host that changes in between. Three things can still reach that choice afterwards: a flag the user passes, a file the user did not write, or the host itself. This row asks the first, [the next row](#no-file-you-did-not-write-selects-a-weaker-boundary) asks the second, and [Runs on a host without KVM](#runs-on-a-host-without-kvm) asks the third.

1. Record which boundary the tool used on an ordinary first run.
2. Read its flag list and its call-time arguments for anything that selects a weaker one.
3. Pass it, and record which boundary the run then used, and what it said.

### Boundary flag, vivarium

vivarium `ceb0027`, read 2026-08-19. Yes: one boundary and no second mode beneath it, so there is no flag to pass. A host that cannot provide it gets a refusal rather than a substitute, which is what the [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes.

### Boundary flag, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: the engine is written into `/usr/share/flakes/<app>.yaml` at registration, and no call-time pseudo-argument revisits it. The set the pilot consumes — `@NAME`, `%remove`, `%interactive`, `%ignore_sync_error`, `%ignore_missing_volume_path`, `%progress`, `%port:number` — contains nothing that names an engine.

### Boundary flag, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `--no-microvm` selects the container outright, in the user's own command. The stance is deliberate — a weaker sandbox beats no sandbox — and vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) takes the opposite position. Both are coherent; they disagree about what a sandbox is for.

### Boundary flag, podman

podman 5.x, read 2026-08-19. No: the boundary is a per-invocation flag. Omit `--runtime krun` and the same command runs the same image under the default runtime, with no warning, and nothing records that the workload was meant to run behind a kernel of its own.

## No file you did not write selects a weaker boundary

The companion to [the flag row](#no-flag-selects-a-weaker-boundary). A flag is at least typed by the person who wanted it; a file is read by a run that never mentions it.

1. Read the tool's configuration schema for a key naming the engine, runtime, or boundary.
2. Record which files that key is read from, and who installs them.
3. Set it there rather than on the command line, re-run the ordinary invocation, and record which boundary was used.

### Boundary file, vivarium

vivarium `ceb0027`, read 2026-08-19. Yes: there is no weaker boundary for a file to select, so no key in a manifest, an image, or a piece can name one. The [class-not-tool rule](../spec/08-invariants-and-guarantees.md) is why the key does not exist rather than being refused — the backend is fixed by capability class, so there is nothing for a file to choose between.

### Boundary file, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the drop-in directory `<app>.d/*.yaml` is read in alpha order with the last key winning, and rewrites an existing registration's options. A file dropped in beside a registration reaches what the registration fixed, and no merge stage reports that it did — the same mechanism that loses flake-pilot the [collision row](#a-collision-between-two-parts-is-reported).

### Boundary file, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the weaker boundary is reached by `--no-microvm` and by a failed host probe, and neither is a file. One adjacency is worth naming and is unverified at this revision: `_parse_conf` runs after the argument loop, so a key `glaipnir.conf` parses overwrites the same value given on the command line. Were the microVM selector ever among those keys, a file would not merely reach the boundary — it would outrank the flag.

### Boundary file, podman

podman 5.x, read 2026-08-19. No: `containers.conf` sets the default OCI runtime, and both a system-wide copy and a per-user copy apply. An invocation that omits `--runtime` runs at whatever that file says, which is a boundary decided somewhere the command does not mention.

## Runs on macOS

Record whether the tool runs on macOS, and what boundary it provides there.

1. On macOS, follow the documented install.
2. Run the default invocation and record the boundary reported inside.

### macOS, vivarium

vivarium `ceb0027`, 2026-08-18. No: the tool targets a Linux host with KVM. The specification takes no position on other hosts, so this is an open gap rather than a refusal.

### macOS, glaipnir

glaipnir `21ef389`, read 2026-08-18. Container only: `_macos_adjust_microvm` sets `USE_MICROVM=0` on Darwin, treating Podman Machine's VM as the boundary. The Linux egress guarantee costs a LaunchAgent, an SSH channel into the machine VM, and an nftables ruleset there.

### macOS, podman

podman 5.x, read 2026-08-19. Container only: Podman Machine interposes one managed Linux VM for every container, and a krun microVM inside it would need nested virtualization the machine does not provide. The VM boundary is per machine, not per workload.

## The tool arranges the workspace mount

Two questions hide in "can I see my project inside". This one asks whether the tool puts the project there without being asked; [the next](#work-stays-at-its-host-path) asks whether the path it lands on is the one it had outside.

1. Put a project at a known absolute path on the host.
2. Start the tool for that project with no mount argument.
3. Record whether the project is visible inside, and what named it.

### Workspace mount, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the workspace is the project the manifest belongs to, so binding the project once is what names it, and no argument repeats the decision at each start. Project identity is anchored by a marker rather than by the path, so the mount survives a rename.

### Workspace mount, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker schema has no bind mount, so nothing is arranged and nothing can be. `include.tar` and `include.path` copy a payload into the artifact at provisioning time — work is copied, not seen through.

### Workspace mount, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the invocation's workspace crosses with no argument naming it, and a workspace equal to `$HOME` is refused and falls back rather than crossing wholesale. Where it lands is [the next row](#host-path-glaipnir), and it is not where it came from.

### Workspace mount, podman

podman 5.x, read 2026-08-19. No: `-v` is the only route and it is typed at the call site every time. Nothing reads the current directory, and a run that omits the flag starts a container that cannot see the project at all.

## Work stays at its host path

Given that the project is visible inside — [the row above](#the-tool-arranges-the-workspace-mount) — record whether the path it occupies inside is the path it occupies outside. Agents key session state on the working directory, so a mirrored tail is not the same answer as a mirrored path.

1. Put a project at a known absolute path on the host.
2. Give the tool that directory by its documented mechanism.
3. Inside, run `pwd` and read the path of a file the host also sees.
4. Record both paths, and whether a tool that stores absolute paths still resolves them.

### Host path, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the [host-symmetric mount rule](../spec/08-invariants-and-guarantees.md) fixes the workspace at the absolute path it occupies on the host, and a declared mount carries that same target rather than one the declaration invents.

### Host path, flake-pilot

flake-pilot `main`, read 2026-08-18. n/a: nothing crosses at the firecracker boundary, so there is no inside path to compare — the same reason the [mount-choice row](#choose-which-host-paths-cross-in-a-file-rather-than-on-the-command-line) and the [session-directory row](#refuses-a-mount-that-would-expose-the-host-session) read `n/a` here. The published `--volume %HOME/ai:%HOME/ai` that does mirror a path is the `crun` container backend, and it mirrors a quarantine directory rather than the project tree.

### Host path, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: since 1.0.0 a workspace under `$HOME` mounts at `/home/aiuser/<path relative to $HOME>` — a mirror of the tail, not of the path. The change answers the same failure class vivarium's [host-symmetric mount rule](../spec/08-invariants-and-guarantees.md) does and stops short of it: an agent that stored `/home/you/api` reads a path that is not there, and a project outside `$HOME` has no tail to mirror.

### Host path, podman

podman 5.x, read 2026-08-19. Reachable, nothing arranges it: `-v /host/path:/host/path` mirrors any single path exactly, and under krun the mount crosses as virtiofs, so it holds at the compared setup. The user types the path twice at every invocation, nothing refuses a mismatch, and published examples usually pick a different target.

## Choose which host paths cross, in a file rather than on the command line

Every subject decides what crosses. This row asks where that decision is written down: in a file, or in the arguments of the command that starts it. Whether that file travels with the project is [Defined by a project file](#defined-by-a-project-file), and whether a second author can add a path without editing the first is [Compose the environment from separate, reusable parts](#compose-the-environment-from-separate-reusable-parts); neither is re-asked here.

1. Add a host path to what crosses, and a second one that must not.
2. Record where that decision is recorded.
3. Record what a run that names neither path then sees.

### Choosing mounts, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: `[[mounts]]` is a table in the manifest, so the set is part of the project's definition rather than of an invocation, and no command adds a path the manifest does not show. Because layers merge as NixOS modules and lists concatenate, a piece contributes mounts to the same list without editing the manifest that imported it, and a shared layer may only reach the host through portable variables — `${HOME}` and the four durable XDG directories — with a literal personal path failing evaluation with `65`. Two floors bound the choice rather than the user: a mount whose source resolves to a session directory is refused before boot, and what does cross carries the [host-symmetric](../spec/08-invariants-and-guarantees.md) target rather than one the declaration invents.

### Choosing mounts, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. n/a: there is no bind-mount mechanism at the firecracker boundary, so there is no set to choose from — the same reason the [session-directory row](#refuses-a-mount-that-would-expose-the-host-session) reads `n/a` here. What the registration can carry is `include.tar` / `include.path`, which copies material into the artifact at registration time rather than selecting what crosses at run time. Under the container backend the choice is podman's `-v`, which is the shared-kernel answer.

### Choosing mounts, glaipnir

glaipnir `8c7420e`, read 2026-08-20 — a later revision than the `21ef389` the rest of this subject is pinned to, read fresh for this row. No: the crossing set is written in the script. `_bind_agent_mounts` emits a fixed `--volume` list per agent name, and the workspace, hooks, and cache mounts are assembled at the call site beside it. There is no configuration key that adds a path, so a user who wants one edits `glaipnir.sh` — which is the same built-in opinion that wins glaipnir the [credential-scoping row](#scopes-credentials-per-app-out-of-the-box) and loses it [Works for a tool the sandbox has never heard of](#works-for-a-tool-the-sandbox-has-never-heard-of).

### Choosing mounts, podman

podman 5.x, read 2026-08-20. Reachable, nothing arranges it: the documented answer is `-v` on the command line, and a Quadlet unit does record the same decision in a file — `Volume=` is "equivalent to the Podman `--volume` option" and takes the same argument form. What the unit is not is the path anyone is sent down: the manual pages teach `-v`, and a project that wants the file writes it itself. Where that file lives, and what happens when two concerns want to edit it, are the two rows this one defers to.

## The caller chooses which host environment variables cross

Record who decides which of the host's environment variables are visible inside: the person starting the sandbox, or the tool.

1. Export a distinctive variable on the host.
2. Start the tool without naming that variable, and record whether it appears inside.
3. Name it by the tool's documented mechanism, and record whether that mechanism is open to any variable or fixed to a list the tool ships.

### Environment, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the host environment is deny-by-default against a fixed allowlist, and `--env KEY` copies a named variable from the host only when it exists while `--env KEY=VAL` supplies a literal. The caller names what crosses, per invocation, and the manifest names what crosses durably.

### Environment, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: at the firecracker boundary there is no environment passthrough at all, so nothing crosses until the registration says so. What it lacks is a policy of its own — at the container backend the set is whatever the registration froze into `--opt` lines, and a `%VAR` placeholder with no matching variable becomes the literal name rather than failing.

### Environment, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: an explicit list crosses rather than a wholesale copy — `TERM` and `COLORTERM`, `GOOGLE_CLOUD_PROJECT` and `VERTEX_LOCATION` when set, plus five computed `AI_*` values — but the list is the tool's rather than the caller's. The four host-sourced names are forwarded whenever they exist, and there is no argument or key that adds a fifth. A closed set is a real guarantee; it is not this row's question.

## Refuses a mount that would expose the host session

Record what happens when a mount names one of the host's session directories — `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` — as its source. These hold live session state: the session bus, the display socket, the authentication-agent socket.

1. Name `/tmp`, `${XDG_RUNTIME_DIR}`, or an ancestor of either as a mount source.
2. Start the tool.
3. Record the exit status, the message, and whether anything booted.

### Session sockets, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the [session-directories-never-cross rule](../spec/08-invariants-and-guarantees.md) refuses a mount whose `source` resolves to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`, or any ancestor, in either layer, before boot. A share conveys an inode, not a listener, so mounting a socket directory grants the exposure without the capability that motivated it.

### Session sockets, flake-pilot

flake-pilot `main`, read 2026-08-18. n/a: there is no bind-mount mechanism at the firecracker boundary, so there is nothing to refuse. Under the container backend a session socket is an ordinary `--opt "\-v ..."` and nothing objects — the shared-kernel answer, not this one.

## Use an SSH or GPG key without the key entering the sandbox

Record what has to cross the boundary before a tool inside can authenticate with a key the user already has.

1. Have a key the host's `ssh-agent` or `gpg-agent` already holds.
2. Make the tool inside the sandbox use it.
3. Record what is in the guest afterwards: the key, a copy of it, or neither.

### Key material, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: neither. The host agent's socket is relayed on a dedicated credential port of the same vsock-class transport the control plane uses, and the guest talks to `/run/vivarium/ssh-agent.sock`. What may be forwarded is a closed allowlist of two, `ssh` and `gpg`, declared through a typed option that names no host path — which is what lets a shareable piece declare it — and the GPG side takes the agent's restricted extra socket rather than the ordinary one. A mount could not do this at all: a socket's endpoint is an object in the kernel that owns the listener, so a guest with its own kernel finds a name with nothing behind it. Two limits are stated rather than engineered away: a compromised guest can use the key for as long as the session lasts, and a byte relay does not carry the signal an agent uses to recognise a forwarded connection.

### Key material, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker boundary has neither a share nor a relay. A key reaches the guest only by being written into the image or into an `include.tar` / `include.path` payload, which is the material itself rather than its use.

### Key material, glaipnir

glaipnir `21ef389`, read 2026-08-18. No, by a different route: nothing is forwarded, and the design instead authenticates inside the sandbox and persists the result to a host cache directory the user owns. What ends up in the guest is a token rather than a private key, which is better than copying one — but an existing host key still cannot be used from inside.

### Key material, podman

podman 5.x, 2026-08-19. No: mounting the agent socket — `-v $SSH_AUTH_SOCK`, and the same move for `gpg-agent` — is the usual answer, and it is a shared-kernel answer. Under `krun` the guest runs its own kernel, so the shared inode has no listener behind it, and podman relays no agent by any other route.

## Secrets are kept out of the built artifact

Record whether the documented way of using the tool puts a credential into the artifact the environment is built from, and who can read it if one lands there. Whether anything stops a user putting one there anyway is [the next row](#keeping-secrets-out-of-the-build-is-enforced).

1. Read what the build consumes, and whether any documented flow puts a credential there.
2. Record what the tool says about it.
3. Record who else on the machine can read the artifact.

### Secrets in the build, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: a build-time secret is prohibited outright, and the reach of one is why the rule takes no exception. The store is shared read-only into every guest on the machine, so a secret in a store path is readable by every sandbox running there, including one deliberately running untrusted code. The rule is wider than "do not read a credential during the build": the manifest is itself compiled into a module and realised, so `[env] TOKEN = "…"` is a build-time secret whatever its launch-channel classification suggests. What replaces it is the agent channel above, a scoped short-lived value passed at launch, or an encrypted-at-rest scheme the user composes in — vivarium performs no decryption and holds no identity.

### Secrets in the build, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the stance is stated up front and holds in the code — nothing is baked into the image, authentication happens at runtime inside the container, the token lands in a host cache directory the user owns, and the image carries the label `security.credentials="runtime-only"`. Whether anything holds a user to it is [the next row](#secrets-enforced-glaipnir), and there the answer changes.

## Keeping secrets out of the build is enforced

The companion to [the row above](#secrets-are-kept-out-of-the-built-artifact). A documented flow that keeps credentials out of the artifact is worth having; this row asks what happens to the user who ignores it.

1. Put a credential into the build by whatever route the tool leaves open.
2. Record whether anything refuses, warns, or notices.
3. Record whether what noticed is a rule, a check, or a convention.

### Secrets enforced, vivarium

vivarium `ceb0027`, 2026-08-18. Partial: the prohibition is a binding rule rather than a practice, and the pure build means there is no arbitrary build step to smuggle one through. What detection cannot be is complete, and the specification says so itself — a plaintext secret is not decidable by inspection, so the `manifest-no-inline-secret` check warns heuristically, and a value it does not flag is not a promise. A rule that binds and a check that only warns is two thirds of an answer.

### Secrets enforced, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: it is practice rather than a rule. Build hooks run arbitrary commands as root at build time, so a user who puts a credential there gets it in the image, and nothing objects — no check reads the hooks, and the `security.credentials="runtime-only"` label keeps saying what it said.

## Commit an encrypted secret alongside the config

Record whether a secret the environment needs can travel with the project's own definition, rather than being installed on each machine by hand.

1. Put a secret the environment needs into the project's own files, in a form safe to commit.
2. Hand the project to a second person who holds a decryption identity.
3. Record what reaches the guest, what reaches the build, and what the tool itself had to do.

### Shipping a secret, vivarium

vivarium `ceb0027`, 2026-08-18. Reachable, nothing arranges it: encrypted-at-rest is one of the two shapes the [specification](../spec/07-secrets-and-config-sharing.md) names for a secret, and it is the one meant for sharing — commit files that decrypt at activation into a runtime-only location, never into the store, with only the public recipient identities in clear. What vivarium supplies is the seam, not the scheme. A piece is a NixOS module, so a team that wants decrypt-at-activation imports one the way it imports anything else, and the identity that scheme needs arrives over the GPG relay of the row above rather than as a mounted host path. What it will not do is a closed list of seven, not a gap: no decryptor, no provider command executed and relayed, no decryption identity held, no plaintext written to a host path, no credential in any root, no verb whose subject is a credential value, and no reasoning about a credential's lifetime ([`ADR-0072`](../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md)). The fourth and fifth are the load-bearing pair: a provider hook looks like the smallest possible integration and is the opposite of one, because it would put plaintext in vivarium's own address space, which is the condition its redaction guarantee is free of today. Available rather than provided, and deliberately so.

### Shipping a secret, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: the repository has no secrets mechanism of any kind, encrypted or otherwise — the only matches for the word are a CI workflow's own credentials. Material a registration needs reaches the guest as image content or as an `include.tar` / `include.path` payload, both of which carry it in clear inside the artifact, which is the shape the row above already records.

### Shipping a secret, glaipnir

glaipnir `8c7420e`, read 2026-08-20 — a later revision than the `21ef389` the rest of this subject is pinned to, read fresh for this row rather than inferred from the earlier one. No: the stance is that the image holds no secret at all, stated as a documented guarantee, and nothing ships one beside the definition either. Authentication happens at runtime inside the container and the result persists to a host cache directory the user owns, which is a per-machine step by construction: the second person authenticates again rather than receiving anything.

### Shipping a secret, podman

podman 5.x, read 2026-08-20. Reachable, nothing arranges it: `podman secret create` takes a `pass` driver, where the secret "resides in a GPG-encrypted file", and a `shell` driver that hands storage to scripts of the user's choosing; `--secret` then mounts it at runtime rather than baking it in. Two things keep the arrangement the user's own. The default `file` driver is a read-protected file and not an encrypted one, so the safe answer is the one you have to ask for. And the store is machine-local podman state that a `Containerfile` or Quadlet unit refers to by name — a `pass` store can itself be shared, but the binding to the project is a name that must already resolve, so the second person runs a command before anything works.

## Scopes credentials per app out of the box

Every subject can narrow what crosses by declaring or passing less. This row asks the narrower question: whether the tool arrives already knowing which credential directory belongs to which application, so the scoping happens without the user mapping it.

1. Authenticate two different applications so each writes its own credential directory.
2. Start the sandbox for one of them, naming only the application.
3. Inside, attempt to read the other's credential path, and record the result.

### Per-tool credentials, vivarium

vivarium `ceb0027`, 2026-08-18. No, by design: `[[mounts]]` is declared per manifest, so every process in the guest sees every declared mount, and narrowing means the user declaring less. Automatic scoping would require vivarium to hold a table of application names and the credential paths each one wants — a hidden mapping between a tool the user did not name and a host path they did not declare. vivarium takes the opposite position: it is application-agnostic, every path that crosses is one the user wrote down, and no command has a side effect the manifest does not show. The manifest is where a user narrows this, and it is the only place.

The position is consistent with the [config-read-only rule](../spec/08-invariants-and-guarantees.md) and the [never-touch-user-files rule](../spec/08-invariants-and-guarantees.md), which refuse side effects on user-owned surfaces for the same reason, but no invariant states application-agnosticism itself. Until one does, this cell is the only `†` in the set resting on a design position rather than a binding rule; the question is logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md).

### Per-tool credentials, flake-pilot

flake-pilot `main`, read 2026-08-18. No: at the firecracker boundary a credential arrives as an `--include-path` copy fixed at registration time, so the payload is frozen per registered application rather than selected per run. One registration, one baked-in set, and no per-consumer view of it once the guest is up.

### Per-tool credentials, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: `_bind_agent_mounts` holds a table mapping seven agent names to the credential directories each one owns, and emits only the volumes the selected agents need, so `run claude` never mounts the `gh` token — it is absent from the guest rather than hidden inside it. The volumes cross into the krun guest as virtiofs, so the scoping holds at the compared setup.

What it costs is the reason vivarium answers the other way. The table is seven hardcoded names, so an agent glaipnir does not know gets no scoping, and a user who wants a different mapping edits the tool rather than a file they own. The same built-in opinion is what loses glaipnir [Defined by a project file](#project-file-glaipnir) and [Choose the guest operating system](#guest-os-glaipnir).

### Per-tool credentials, podman

podman 5.x, read 2026-08-19. No: `-v` is a flat list assembled at the call site and podman has no notion of which process inside needs which mount, so everything passed is visible to everything in the guest. Launching one container per tool with a different `-v` set reproduces the effect, and that is the user doing it per invocation rather than the tool providing it.

## Egress can be default-deny

Record what the tool can reach on the network with no destination named. The row asks whether a default-deny posture is reachable at all; whether a destination can then be readmitted by name is [the next row](#allowlist-by-destination-name), and is not re-asked here.

1. Configure the tool for its most restrictive documented network posture.
2. Inside, attempt a TCP connection to an arbitrary public address.
3. Record the result and how long it took to answer.

### Default-deny egress, vivarium

vivarium `ceb0027`, 2026-08-18. Yes, and open is the default: the [egress-defaults-open rule](../spec/08-invariants-and-guarantees.md) makes unrestricted egress the shipped posture and requires exactly one declarative knob to switch to a default-deny allowlist, which is `sandbox.egress.mode`. Open egress is stated not to weaken the boundary — the boundary is the microVM, and the cost of open egress is exfiltration exposure, a workload policy choice rather than a containment property.

The deny posture is enforced host-side, in the VM's own network namespace, because a guest holding root could tear down any ruleset it can see; denials are rejected rather than dropped, so a blocked attempt fails in milliseconds instead of hanging. Both modes run today: a denied name answered `REFUSED` in 9 ms and a denied literal connect reset in 15 ms, measured 2026-08-14 and recorded in [`implementation-status.md`](../implementation-status.md). Details in [`spec/05-networking-and-egress.md`](../spec/05-networking-and-egress.md).

### Default-deny egress, flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Yes, by absence rather than by policy, and the absence is the shipped state. Upstream states that firecracker "supports networking only through TUN/TAP devices" and that "it is the user's responsibility to set up the routing on the host from the TUN/TAP device to the outside world", then walks a static-IP NAT setup: `ip_forward`, a MASQUERADE rule, a `tap-<app>` device per registration, and `boot_args` edited from `ip=dhcp` to a static triple. Until an operator does that work a microVM reaches nothing, and `flake-ctl firecracker register --no-net` keeps it that way deliberately, which is the documented restrictive posture this row asks for.

### Default-deny egress, podman

podman 5.x, read 2026-08-19. Yes: `--network none` is genuinely default-deny, and the network posture is podman's rather than the OCI runtime's, so it applies at the krun setup too.

## Allowlist by destination name

Record whether destinations can be permitted by name rather than by address, and what a denied name answers.

1. Configure an allowlist naming one host.
2. Inside, resolve and connect to that host.
3. Resolve and connect to a host not on the list.
4. Record both answers, and whether a denial is distinguishable from a name that does not exist.

### Allowlist, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: `egress.allow` names destinations as an exact name, `*.example.com` for exactly one further label, `**.example.com` for one or more, or a literal address or CIDR block, concatenating across composed pieces. vivarium's resolver is the only DNS the guest is given and is the enforcement point: an unmatched name is answered `REFUSED` and never forwarded, so it does not even leak upstream, and a matched name's addresses enter the filter before the reply reaches the guest. A denial is therefore distinguishable from a name that does not exist, which answers `NXDOMAIN`. An entry carries no port: the policy limits where data can go, not which port it leaves by. No other subject in this set has a name-level mechanism to compare.

## Stays off a corporate VPN

Record which host interface the tool's traffic leaves by.

1. Bring up a VPN on the host so it owns or shares the default route.
2. Start the tool and, from inside, reach an address that logs its source.
3. Record the observed source, and what the tool did about the VPN.

### Corporate VPN, vivarium

vivarium `ceb0027`, re-read 2026-08-19. No, and the behavior is inheritance rather than absence: the uplink keeps its sockets on the host side and opens its device inside the VM's namespace pair, so guest flows are re-originated as host sockets and take the host routing table. A VPN owning the default route therefore carries the sandbox's traffic, and the uplink's DNS forward reaches the host's configured resolver, so name lookups go the same way. `allowlist` mode narrows which destinations may be reached without changing which path reaches them. No manifest key selects an interface, and the specification names neither the threat nor the inherited default; the scope question is [`Q-032`](../../plan/open-questions.md).

### Corporate VPN, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: `_detect_public_iface` reads the default route, filters out `tun|wg|vpn|tap|ppp|openvpn|docker0|br-`, and binds egress with `--network pasta:--outbound-if4,<iface>`. No qualifying interface aborts the run — the one place the script is fatal rather than degrading.

## Something outside reaches a guest service

Record whether a process outside the sandbox can connect to a service running inside it.

1. Start a listener inside on a known port.
2. From the host, attempt to connect to it.
3. Record the result and what configuration, if any, was needed.

### Inbound, vivarium

vivarium `ceb0027`, re-read 2026-08-19. No, and nothing is arranged either way: the guest's network sits in its own namespace behind an uplink the launcher starts with no port-forwarding argument, so there is no host-visible address, and `exec` and `shell` reach the guest over the vsock control plane rather than a listening socket, so there is no key to reach one with. `spec/05` describes egress and is silent on the other direction, and `spec/00` does not list inbound reachability as a non-goal. An editor over SSH, a guest dev server in the host browser, and a hosted notebook have no answer today; the scope question is [`Q-033`](../../plan/open-questions.md), which [slice 023](../../plan/slices/023-a-declared-port-crosses-inward/README.md) is shaped to answer by building the declaration. The verdict stays a bare no rather than a `*` until it does: nothing in the specification closes the direction yet, and a shaped slice is not a contract.

### Inbound, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: reachable but entirely the operator's job — a TAP device per instance (`@NAME` names it), `ip_forward`, MASQUERADE, hand-edited `boot_args`. Nothing in the tool arranges any of it, and that is host plumbing the user builds rather than a mechanism the tool offers, which is why this stays a hedge rather than becoming a qualified yes.

### Inbound, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: the assembled `podman run` publishes no port and there is no passthrough argument for adding one. The network flags the script sets are about egress.

### Inbound, podman

podman 5.x at `--runtime krun`, re-read 2026-08-19 from upstream documentation rather than run. Yes: `-p` publishes and the guest listener is reached through it. libkrun's transparent socket impersonation is on whenever no virtual interface is added, which is the default under podman, and the project states that applications in the VM receive connections from the outside to ports listening inside it. An impersonated listener belongs host-side to the VMM process, so it lands in the container network namespace podman's ordinary publish path already forwards into, and `-p` needs no krun-specific handling. A published run of `podman run --annotation=run.oci.handler=krun -dp 8080:8080` load-tested from the host is the demonstration. Two conditions bound it: a guest cannot listen on datagram sockets, and the `krun.use_passt` annotation swaps impersonation for a virtio-net interface that no podman flag then wires a published port into. A 2025 podman report has `krun` declining an `AF_INET6` listener, which is where a program binding `::` instead of `0.0.0.0` lands; upstream documents both families as supported, so that one is reported rather than settled.

## Defined by a project file

Record where the definition of the environment lives relative to the project it serves.

1. Create two projects needing different environments.
2. Configure each by the tool's documented mechanism.
3. Record where each definition is stored, and whether entering a project directory is enough to select the right one.

### Project file, flake-pilot

flake-pilot `main`, read 2026-08-18. No: a registration writes `/usr/share/flakes/<app>.yaml`, plus an `<app>.d/` drop-in directory. The unit is the application, not the project — two projects wanting different environments for the same tool need two registered command names.

### Project file, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per project — and `_parse_conf` runs after the argument loop, so a value in the file overwrites the same value given on the command line.

### Project file, podman

podman 5.x, 2026-08-18. Partial: a `Containerfile` can live in the project and describe the environment exactly, but nothing binds it to the directory or resolves it on entry — the binding is the user's shell history.

## Choose which programs are installed in the guest

Record how a user adds a program the base does not already have.

1. Pick a compiler or command line tool absent from the default environment.
2. Add it by the tool's documented mechanism.
3. Record where that request is written, and what a second person has to do to get the same set.

### Programs installed, vivarium

vivarium `ceb0027`, 2026-08-20. Yes: an image and a piece are NixOS modules, so a program is named in `environment.systemPackages` exactly as it would be on a NixOS host, and the module system concatenates those lists across every layer. Adding a piece therefore adds its packages without touching the manifest that imported it. The place to write it is deliberately not the manifest: [`03-artifact-model.md`](../spec/03-artifact-model.md)'s key table is the manifest's whole surface and carries no package key, so a personal one-off goes through `extends = "./custom.nix"` and anything meant to be reused becomes a piece. That is the same split that makes the set shareable — the package a concern needs travels with the concern.

### Programs installed, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: flake-pilot registers an image and never describes its contents. Upstream is explicit that images are built by any means the user likes — KIWI, podman, mkosi, OBS, koji — and no key in a registration or an `<app>.d/` drop-in names a package. `--include-tar` and `--include-path` come closest and are not the same thing: they copy a payload onto the instance at provisioning, so the user supplies built files rather than a name to resolve.

### Programs installed, glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes: a `PACKAGES=(…)` array in the config file is interpolated into the image's `zypper install` line at build time. It resolves against the default Tumbleweed repositories only — a package from anywhere else needs a build hook that adds the repository first, which is the mechanism the next row measures.

### Programs installed, podman

podman 5.x, 2026-08-20. Yes: a `RUN` line in a `Containerfile`, which is the ordinary way and works. What it costs is the row below on pinning: the line names a package, the repository decides the version, and the same file built later produces a different set.

## The project's own dev environment loads when you enter

A project usually already describes its own toolchain — a `flake.nix` with direnv, a version manager, or an equivalent. Record what happens to that description inside the sandbox.

1. Take a project whose toolchain is declared in its own repository.
2. Start the sandbox and enter the workspace.
3. Record whether the project's toolchain is active, and what the user had to change to get there.

### Inner environment, vivarium

vivarium `ceb0027`, 2026-08-20. Yes, specified and not yet built: [`06-workspace-and-project-environment.md`](../spec/06-workspace-and-project-environment.md) makes this a design requirement rather than a convenience. The project's environment is the inner layer — owned by the repository, never modified by vivarium, and required to work identically whether or not the sandbox is in use. Two Nix evaluations exist and must not be conflated: the outer one builds the VM from the manifest on the host, the inner one builds the project's environment inside the guest when a shell enters the workspace, with separate files, separate lockfiles, and separate times. For that to work the base must ship a Nix toolchain with flakes enabled and direnv, and `viv shell` enters as a login-interactive shell so direnv can load it. The `*` is the shipped guest: it enables neither `nix-command` nor `flakes` globally and installs no direnv, leaving the entry-time half of the requirement unmet.

### Inner environment, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: there is no project to enter. A registration is per application, and at the firecracker rung the guest's init is `sci`, which evaluates the single `run=` command from the kernel command line, executes it, and reboots. Nothing mounts a project tree and nothing runs a login shell in it, so a repository's own toolchain has neither a place to be nor a moment to load.

### Inner environment, glaipnir

glaipnir `21ef389`, read 2026-08-20. Partial: the workspace is mounted, so the project's own files — including its `flake.nix` or version-manager config — are visible inside. What is missing is anything that reads them. The base is openSUSE Tumbleweed fixed in the `Containerfile`, and it carries no Nix, no direnv, and no version manager, so entering the sandbox leaves the project's declared toolchain inert. A run hook is where a user would add one, at their own expense.

### Inner environment, podman

podman 5.x, 2026-08-20. Partial: bind-mount the project and its files are there, and an image that happens to ship direnv or a version manager will load them. Nothing in podman asks for that, so whether the project's toolchain activates is a property of the image someone chose rather than of the tool — the same file works for one colleague and not another.

## Compose the environment from separate, reusable parts

Record how a second concern is added to an environment that already has one. What happens when the two disagree is [the next row](#a-collision-between-two-parts-is-reported).

1. Define an environment carrying one concern.
2. Add a second concern written by somebody else, without editing the first.
3. Record what was edited to adopt it.

### Composition, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: an image and an ordered list of pieces are imported as NixOS modules and merged by the module system, with no vivarium merge engine of its own. Lists concatenate, so every layer contributes to the package set, the mount list, and the egress allowlist without editing the layer beneath it.

### Composition, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: two mechanisms, at two levels. An image composes by OCI layering, `--base` for a delta container and a repeatable ordered `--layer`; a registration composes by drop-in, `<app>.d/*.yaml` read in alpha order. A second author's file is adopted by dropping it in, with nothing edited.

### Composition, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the extension surface is ordered `NN-*.sh` drop-in hooks, run as root at build and as `aiuser` on every start, `shellcheck`-validated before use, plus a `PACKAGES=(...)` array interpolated into the base install line. A second concern is a second file in the hooks directory.

### Composition, podman

podman 5.x, 2026-08-19. No: a `Containerfile` composes linearly — one `FROM` and a sequence of steps, with multi-stage builds copying artifacts between stages. There is exactly one base, so two bases cannot be adopted together, and combining two authors' work means editing one file into the other by hand.

## A collision between two parts is reported

Composition is cheap until two parts set the same thing. Record what the tool does then: report it, resolve it silently, or have nothing to resolve.

1. Adopt two parts that set the same value to different things.
2. Build, and record what the tool said.
3. Record which value the guest ended up with, and whether anything named the other.

### Collision, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: scalars resolve by priority rather than by position — a shared piece proposes with `mkDefault` and the user's manifest outranks it, a policy floor uses `mkForce` and nothing outranks that, and two definitions surviving at the same priority fail evaluation with `65` rather than being settled by order. That last rule is what lets two independently written pieces be adopted together: a collision is reported, never resolved behind the user's back. `viv config eval` and `viv config sources` give the merged view with provenance, so the answer to "who set this" is a command rather than a reading exercise.

### Collision, flake-pilot

flake-pilot `main`, read 2026-08-18. No: `<app>.d/*.yaml` is read in alpha order and the last key wins. Two drop-ins setting one key is neither a conflict nor a merge — the later filename wins — so adopting a second author's file can undo the first's without saying so, and the deciding fact is a filename. The same mechanism is what loses flake-pilot the [boundary-file row](#boundary-file-flake-pilot).

### Collision, glaipnir

glaipnir `21ef389`, read 2026-08-18. n/a: hooks compose the way shell does, by running one after another, and `PACKAGES=(...)` is one array in one file. There is no declaration for two parts to disagree over, so there is no stage at which a disagreement could be reported. Not a gap — a different shape of extension, whose cost is paid in [the row above](#composition-glaipnir) rather than here.

### Collision, podman

podman 5.x, 2026-08-19. n/a: there is one `Containerfile` and one author of it at a time, so two parts never meet to collide. A later `RUN` overwriting an earlier one's work is a script overwriting itself, which is the ordinary reading of a sequence.

## There is a shareable unit smaller than the whole environment

Record what a user can hand a colleague so the colleague's environment gains one concern — and whether that unit is the concern or the whole environment. Whether it then works unchanged is [the next row](#a-shared-units-portability-is-enforced).

1. Configure one concern that needs a package and a host path.
2. Identify the smallest unit carrying it.
3. Record what the colleague receives, and what else comes with it.

### Config unit, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: images and pieces are the shared class and the manifest is the personal one. A piece carries its whole concern — packages, guest config, mounts, environment, and any third-party flake input it needs through its own `inputs.toml` — so adopting it is one name in the colleague's `pieces` list. The same split is how a team makes a guarantee unwaivable: a piece setting a value with `mkForce` outranks every personal manifest.

### Config unit, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: an `<app>.d/*.yaml` drop-in is smaller than the registration and carries one concern's options, and a colleague adopts it by copying it into place. The image is the other unit and it is the whole environment; the drop-in is the small one.

### Config unit, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: configuration is one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per concern — there is no unit smaller than that file, and it is the user's own machine-local settings. The hooks directory can be copied by hand, which is file transfer rather than adoption.

### Config unit, podman

podman 5.x, 2026-08-19. No: a `Containerfile` travels and reproduces its build steps elsewhere, and a published image travels as a pull. Neither is a unit of one concern — the smallest shareable thing is the whole environment, which is the same single-base limit that loses podman [the composition row](#composition-podman).

## A shared unit's portability is enforced

A unit that travels is not the same as a unit that works. Record what happens when a shared unit carries something only its author's machine has.

1. Put a literal personal path into a unit meant to be shared.
2. Hand it to a second user and build.
3. Record whether anything refused, warned, or noticed, and when.

### Portability enforced, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: a shared layer reaches the host only through portable variables — `${HOME}` and the four durable XDG directories — which every host resolves, and expansion happens at launch, so no expanded path is ever written into the artifact. A literal personal path in a shared artifact fails evaluation with `65` before the build, naming the artifact. This is the mechanism, not the advice: the same rule that makes the docs self-contained is the one the evaluator applies to a piece.

### Portability enforced, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker registration names local file paths under `/var/lib/firecracker/images/`, and a drop-in carrying a host path carries it literally. Nothing checks, so the failure arrives on the colleague's machine at run time rather than on the author's at build time.

### Portability enforced, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: there is no shared unit for a check to apply to, and the hooks that can be copied by hand are shell scripts that may name anything. `shellcheck` validates them as shell, which is not the same question.

### Portability enforced, podman

podman 5.x, 2026-08-19. No: nothing reads a `Containerfile` for a host path the colleague does not have, and a `-v` or `Volume=` line naming one is ordinary. The build succeeds and the run is what fails.

## The same definition gives everyone the same environment

Record whether a definition produces the same environment on a second machine and at a later date, and what a second person has to be given for that to hold.

1. Record the definition and every version it names.
2. Reproduce the environment from it on a second machine, a month later.
3. Record what identifies each result, whether the two are the same object or merely similar, and what fixed each version.

### Same definition, vivarium

vivarium `ceb0027`, 2026-08-18. Yes, by two mechanisms that need each other. The [pure-build rule](../spec/08-invariants-and-guarantees.md) fixes the build as pure — the same manifest closure and lock give the same store output anywhere — and exactly one lockfile is in force, pinning the whole input graph rather than a revision here and there. By default that is the per-target lock vivarium owns under the data root, created by the first evaluation and moved only by `viv update`; a team that wants one shared answer puts a read-only override lock beside the manifest, and it then outranks the tool-owned lock for everybody. `viv config` reports which of the two is in force. A piece declaring a flake input the lock has no node for fails closed with `78`, naming the input and the artifact, rather than resolving it quietly — an unannounced input jump is exactly what the pin exists to prevent.

### Same definition, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the firecracker unit is a versioned tarball fetched by URL into `/var/lib/firecracker/images/<name>/`, so a second user given the same URL has so far received the same rootfs. Nothing makes that a property rather than a habit — no digest, and the definition the image was built from is not carried, so there is nothing to rebuild from and nothing to compare against.

### Same definition, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `Containerfile.agent` starts `FROM` a per-agent image at `:latest`, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and there is nothing to hand a second user that fixes any of them.

### Same definition, podman

podman 5.x, read 2026-08-18. Reachable, nothing arranges it: an image referenced by digest is exactly one artifact, and a colleague given the digest gets it. The published form is a tag, the documented flow is a tag, and a `Containerfile` rebuilt a month later re-executes its `RUN` steps against whatever the network serves that day. The exact answer exists and is the one nobody is pointed at.

## Run your own setup at build time

Record what a user can execute of their own while the environment is being produced. Read it beside [The build runs no user-supplied commands as root](#the-build-runs-no-user-supplied-commands-as-root): the two ask one question from opposite sides, and a yes here is what a no there is refusing. What can run at each start is [the next row](#run-your-own-setup-at-every-start).

1. Write a setup step the tool does not provide — a repository added, a file generated.
2. Attach it so it runs while the environment is built.
3. Record whether it ran, as whom, and what a second person needs to reproduce the result.

### Own setup at build, vivarium

vivarium `ceb0027`, 2026-08-20. No, by rule: there is no script slot at all, because arbitrary build steps running as root are refused. Build-time setup is expressed as declaration instead — a package to install, an option to set, or a derivation that produces the file. Most setup hooks convert; one that expects to reach the network mid-build does not, because that is the reproducibility [the same-definition row](#same-definition-vivarium) measures. Not planned, and the cost is real.

### Own setup at build, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: the registration flags carry no script. `--include-tar` and `--include-path` transfer a payload onto the instance rather than executing anything, and the image's own build happens in a toolchain the registration never sees.

### Own setup at build, glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes, and this is the subject that has it most directly: `--build-hook` runs the user's script as root inside the build context, which is how a package outside the default repositories gets its repository added — the mechanism [the packages row](#programs-installed-glaipnir) defers to. The cost is [the no-root-build row](#build-steps-glaipnir) and [the same-definition row](#same-definition-glaipnir), where the same generality reads as a loss.

### Own setup at build, podman

podman 5.x, 2026-08-20. Yes: `RUN` covers the build side completely, as root, with no restriction on what it does. It is the widest build-time answer in the set, and what it costs is [the same-definition row](#same-definition-podman), where the same line names a package and the repository decides the version.

## Run your own setup at every start

The companion to [the build-time row](#run-your-own-setup-at-build-time). Some setup cannot happen at build: it needs the host as it is now, or the credential that arrived since. Record whether the tool has a place for it, and whether a second such step can be added without editing the first.

1. Write a setup step that must run each time the sandbox starts, before the user's work does.
2. Attach it by the tool's documented mechanism.
3. Attach a second one, and record what it took to add.

### Own setup at start, vivarium

vivarium `ceb0027`, 2026-08-20. Yes, with no limit worth naming: a piece is a NixOS module, so a systemd service, a timer, or an activation step is declared the way it would be on any NixOS host, and it composes with every other layer through the same merge. A second one is a second module rather than an edit to the first, which is [the composition row](#composition-vivarium) paying out. What it is not is a shell hook — the unit is a declaration the module system can see, which is what lets [a collision be reported](#collision-vivarium).

### Own setup at start, flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: `sci` runs the one `run=` command and then reboots, so the single execution slot is the application itself. That is the same in-guest emptiness that loses flake-pilot [a second session](#concurrent-sessions-flake-pilot).

### Own setup at start, glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes: `--run-hook` stages scripts into a mounted directory, and the entrypoint finds every `*.sh` there, sorts them, and runs each one on every start. The ordered `NN-*.sh` convention is what composes them, and each is validated with `shellcheck` before use. The cost is the same as the mechanism: they compose the way shell does, one after another, so nothing can report a disagreement between two of them — which is where [the collision row](#collision-glaipnir) reads `n/a`.

### Own setup at start, podman

podman 5.x, 2026-08-20. Reachable, nothing arranges it: the start side is one command — `ENTRYPOINT` baked into the image, or `--entrypoint` replacing it for a single run — so setup means writing a script that does the work and then execs the real one. The mechanism is podman's and the arrangement is the user's: there is no directory of start steps to add to, so a second step edits the first, or rebuilds.

## The build runs no user-supplied commands as root

Record what executes while the environment is being produced, and as whom.

1. Read the tool's build path for anything executing a user-supplied command.
2. Record whether it runs as root, and whether its effect is recorded anywhere.

### Build steps, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: build hooks execute as root during the image build, and the base image installs agents from npm and from vendor scripts piped to `bash`. The generality is the point; the cost is that the image is not reproducible from the declaration alone.

## Choose the guest operating system

Record which operating system runs inside, and whether the user can name a different one.

1. Read the tool's own definition format for a key naming a base system or image.
2. Set it to a second, unrelated distribution and rebuild.
3. Start the workload and record what `/etc/os-release` says inside.
4. Record what had to change on the host for that to work.

### Guest OS, vivarium

vivarium `ceb0027`, 2026-08-18. No, by rule: the guest is a NixOS system because vivarium [composes through the NixOS module system](../spec/08-invariants-and-guarantees.md) rather than a bespoke merge engine, and another distribution would need a second composition engine. The manifest chooses the package set and configuration inside that system, not the system. Not planned.

### Guest OS, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: the guest is whatever the registered image is, built by any means the user likes — KIWI, podman, mkosi, OBS, koji. `firecracker-pilot` takes a KIS image with its own kernel and rootfs. The cost is the other side of that freedom, and it is this comparison's reading rather than an upstream claim: the boundary is only as good as the image the registration names, and nothing in the registration attests to what is in it.

### Guest OS, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `image/Containerfile` builds from `registry.opensuse.org/opensuse/tumbleweed:latest`, layered with a published per-agent image. `PACKAGES=(...)` and hooks extend that system; nothing selects a different one short of editing the `Containerfile`.

### Guest OS, podman

podman 5.x, read 2026-08-19. Yes: any image runs, so `/etc/os-release` inside is the user's choice. Under krun the kernel is libkrunfw's regardless of image — the userland is chosen, the kernel is not.

## Pull a prebuilt image

Record what the first run has to do before the environment is usable.

1. On a machine that has never run the tool, run its first documented invocation.
2. Record whether it fetched a finished artifact or produced one, and what that cost.

### Prebuilt image, vivarium

vivarium `ceb0027`, 2026-08-18. No, by rule: the [pure-build rule](../spec/08-invariants-and-guarantees.md) fixes the build as pure, a property an OCI registry pull cannot have since a tag is a mutable name. Not planned — the cheap cold start is a Nix binary cache, which delivers the same closure rather than a differently-built one.

## A later command reaches the instance already running

Two questions hide in "can I get back into it". This one asks whether the instance outlives the command that started it, so that the next invocation joins it rather than booting a fresh one; [the next](#a-second-session-joins-it-while-the-first-is-still-there) asks whether two can be inside at once.

1. Start the instance and let the first command finish.
2. Run a second command against the same instance.
3. Record whether it joined the existing instance or created another.

### Later command, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the VM outlives the command, and `start` is idempotent — on a fresh, already-running VM it is a no-op that exits `0`. That ensure-running step is the shared routine `viv exec` and `viv shell` reuse when they start the VM if needed, so joining and starting are the same code path reached from either state.

### Later command, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: `--resume` keeps the instance, and with `--force-vsock` the VM stays alive host-side so the next call reaches it over the vsock rather than booting a second one. The registration is where that is fixed, and it is fixed for firecracker: upstream states the `krun` handler does not support `exec`, so a `krun` registration cannot use `--resume` either.

### Later command, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: a running krun container cannot be entered at all, so `run` cannot resume into one. The script counts what exists and starts a numbered sibling instead. The `podman exec` and `podman start -ai` resume path that `run` does have is the container backend's.

### Later command, podman

podman 5.x, read 2026-08-19. No: `podman exec` cannot enter a krun container — there is no in-guest agent to inject a process into. The container keeps running and remains listed; what cannot happen is getting back inside it. The exec that works is the shared-kernel runtime's.

## A second session joins it while the first is still there

The companion to [the row above](#a-later-command-reaches-the-instance-already-running): not one session after another, but two at once, sharing one guest, one working tree, and one set of processes.

1. Start the instance and leave a process running inside it.
2. From a second terminal, open another session against the same instance.
3. Record whether it joined the existing instance or created another, and what the two sessions share.

### Concurrent sessions, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: `control.sock` is a listening socket, and each `viv exec` and each `viv shell` opens its own connection to it and performs the transport's per-connection session handshake. Session state — the PTY, the argv, the environment, the exit status — is per connection, and the multiplexing is the transport's rather than vivarium's. N terminals therefore share one guest rather than getting N guests, and an interactive session is a PTY sized before it starts, with job control and resize forwarding.

### Concurrent sessions, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the guest init is `sci`, which executes the one command named by `run=` and reboots, so nothing is left inside to hand out a second shell. Two concurrent sessions are therefore two VMs, separated by a call-time suffix that for firecracker also names the TAP device:

```bash
claude @projA        # one VM: instance projA, tap-claude@projA
claude @projB        # a second VM: its own overlay, memory, and TAP device
```

Two VMs is not the same capability as two shells: the sessions share no working tree, no running process, and no warm state. Registering the app as a multiplexer or an `sshd` would buy that back, at the cost of building the in-guest supervisor flake-pilot does not ship. The `--attach` flag that would join a running instance exists only in `flake-ctl podman register`; the firecracker registration has no such flag, and the full `exec` and `attach` ergonomics belong to the `crun` container backend.

### Concurrent sessions, glaipnir

glaipnir `21ef389`, read 2026-08-18. No, and for the same reason it fails [the row above](#later-command-glaipnir): a krun guest cannot be entered even once more, so it cannot be entered twice. Two runs are two numbered sibling containers.

### Concurrent sessions, podman

podman 5.x, read 2026-08-19. No: with no in-guest agent there is no way to inject a first extra process, let alone a concurrent one. Two `podman run` invocations are two microVMs with two rootfs layers.

## Installs from a distro package

Record what a user must already have before the tool can be installed.

1. On a machine with the tool absent, follow its documented install.
2. Record every prerequisite and every command.

### Install, vivarium

vivarium `ceb0027`, 2026-08-18. No: installation is Nix-native — the user needs Nix and a host with KVM. The other three install with one `zypper` or `apt` line. An open gap: nothing forecloses distribution packaging, and none exists.

## Feels like a native command

Record what the user types to run the sandboxed tool, and whether it names the sandbox.

1. Complete the tool's setup for one application.
2. Record the exact invocation a user types afterwards.

### Native command, vivarium

vivarium `ceb0027`, 2026-08-18. No: running something inside is `viv exec -- <command>` or `viv shell`, so the sandbox is always named — legible but not invisible. flake-pilot's symlink puts the sandboxed `claude` on `PATH`. An open gap: no decision for or against a shim.

### Native command, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `glaipnir run claude` names the sandbox, the same as vivarium's does, and one word shorter is not a different answer. What glaipnir does buy with that word is a roster the invocation can be checked against, which is [the next row](#works-for-a-tool-the-sandbox-has-never-heard-of) and where the two tools part company.

## Works for a tool the sandbox has never heard of

Record whether the sandbox is general, or arrives knowing a fixed set of applications.

1. Choose a program the tool's documentation never mentions.
2. Run it inside by the tool's ordinary mechanism.
3. Record whether anything had to be taught its name, and where.

### Any tool, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: vivarium is application-agnostic and classifies nothing inside as trusted or untrusted. The manifest names packages and mounts; it never names an application the tool holds an opinion about. That generality is the same position that loses vivarium the [credential-scoping row](#per-tool-credentials-vivarium), where knowing which directory belongs to which application is exactly what would be needed, and it is logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md).

### Any tool, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: a registration is a command name and an image, and nothing constrains which command. The pilot reads `argv[0]` from a symlink, so an unheard-of tool is one more registration.

### Any tool, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: the roster is five trusted agents and one untrusted one, hardcoded, with per-agent images, per-agent credential directories, and per-agent authentication predicates. A tool outside the roster has no image, no mounts, and no entry in `_bind_agent_mounts`, so running it means editing `glaipnir.sh`. This is the cost side of the same built-in opinion that wins glaipnir [the credential-scoping row](#per-tool-credentials-glaipnir) — the two rows are one design decision, read from its two ends.

### Any tool, podman

podman 5.x, read 2026-08-19. Yes: podman runs an image and an image runs anything. It knows nothing about applications, which is why it neither helps nor hinders here.

## Boot a previous build when the new one is broken

Record whether a build from a past date can be booted again as it was.

1. Build the environment and record what identifies that build.
2. Change the definition and rebuild.
3. Ask the tool for the earlier environment by that identifier and record what starts.

### Rollback, vivarium

vivarium `ceb0027`, 2026-08-18. Specified, not built: `spec/11` fixes per-project generations, each pinned by a GC root, with `viv generations list`/`activate`/`rollback`/`prune` and `viv start --generation <n>`. None of it runs yet — the `*`. No alternative in this set has any answer.

## Update on purpose

Record what causes the environment to change.

1. Run the tool, and record what identifies the environment.
2. Wait for upstream to move, run it again, and record the identifier.
3. Record what the user did to cause the change.

### Update, vivarium

vivarium `ceb0027`, 2026-08-18. Specified in part: the [pure-build rule](../spec/08-invariants-and-guarantees.md) plus the lockfile mean the environment does not move until a user moves it — that pinning runs today. `viv update`, the verb that moves it, is specified and not implemented — the `*`.

### Update, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: the registration names a local rootfs and kernel, so nothing can move on its own, and a newer image takes `flake-ctl firecracker pull --force`. The `:latest` tag rebuilt daily is the container backend — which is why this row is read at the boundary.

### Update, podman

podman 5.x, 2026-08-18. Partial: a pulled image stays until something pulls again, but the tag it was pulled by has already moved, `--pull=always` and a fresh host both take the new one, and nothing reports which of the two is running.

## Reclaim disk without a teardown

Record how space is recovered from a sandbox that should keep working.

1. Fill the sandbox until it occupies noticeable disk.
2. Delete the data from inside.
3. Run the tool's reclamation and record what the host recovered.

### Reclaim disk, vivarium

vivarium `ceb0027`, 2026-08-18. Specified, mostly not built: `viv volume list` reports occupancy today and `viv volume prune` removes orphans; `viv trim`, `viv volume trim`, `viv volume rm`, and `viv gc`'s store sweep are specified and not implemented — the `*`. A loss today against `podman system prune` and `glaipnir clean all`.

### Reclaim disk, flake-pilot

flake-pilot `main`, read 2026-08-18. No: no reclamation verb. The overlay is a file of the declared `overlay_size`, `%remove` is a podman-only pseudo-argument, and `flake-ctl firecracker remove --vm` deletes the image together with every registration using it — a teardown, not a reclamation.

### Reclaim disk, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes, structurally: the container filesystem is disposable and everything durable is a host directory, so deleting a file inside frees host space immediately. `clean <agent>` and `clean <agent> all` cover the images. Holds at the microVM — krun changes the kernel, not where the bytes live.

## See every definition on the machine

Record how a user finds every sandbox that has been defined on the machine, running or not. Whether the running ones can be found is [the next row](#see-every-running-instance-on-the-machine).

1. Define sandboxes for three different projects.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed and what is missing.

### Definitions, vivarium

vivarium `ceb0027`, 2026-08-18. Specified, not built: there is no machine-wide index of projects today. `viv status` reports the current project, and `viv status -g` together with `viv images list` are specified and do not run — the `*`. A definition lives in the project directory it belongs to, so the filesystem holds the answer and nothing collects it.

### Definitions, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: `flake-ctl list --format table|json|csv` reports every registration — name, engine, config path. The unit is the application rather than the project, which is [Defined by a project file](#project-file-flake-pilot), but every unit that exists is listed.

### Definitions, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: `status` reports the agents and images that exist, and the roster is fixed, so the set is small and fully known by construction. Enumeration is easy for the same reason [an unknown tool cannot run](#any-tool-glaipnir).

### Definitions, podman

podman 5.x, read 2026-08-19. Yes: `podman images` lists every image on the machine, from any directory.

## See every running instance on the machine

Record how a user finds every sandbox that is running right now, including ones started from directories they have forgotten.

1. Start sandboxes for three different projects and leave them running.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed, what is missing, and what would stop them all.

### Instances, vivarium

vivarium `ceb0027`, 2026-08-18. Specified, not built: `viv status` reports the current project; `viv status -g`, a live-session count, and `viv stop --all` are specified and do not run — the `*`. The machine-wide view is the one thing all three alternatives provide today and vivarium does not.

### Instances, flake-pilot

flake-pilot `main`, read 2026-08-18. No: `flake-ctl list` reports registrations rather than instances. Firecracker instances are processes with TAP devices that nothing enumerates, and podman instances live in a separate storage root that needs `CONTAINERS_STORAGE_CONF` to be visible at all — so even the engine's own listing does not find them by default.

### Instances, glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: instances are podman containers under the user's ordinary storage, so `status` and podman's own listing both find them, and the numbered-sibling naming makes a forgotten one legible rather than anonymous.

### Instances, podman

podman 5.x, read 2026-08-19. Yes: `podman ps -a` lists everything on the machine and `podman stop -a` stops it, at any runtime, including krun.
