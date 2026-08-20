# Scenarios

The method for every row in [`README.md`](./README.md), and the evidence behind every verdict that needed more than its symbol. A method is identical for all four subjects; an evidence section says what one of them did.

Every evidence section is read at the subject's fixed setup:

- `vivarium` — its one microVM
- `flake-pilot` — `firecracker-pilot`, at the upstream `claude` firecracker registration
- `glaipnir` — the libkrun microVM
- `podman` — `podman run --runtime krun`

The first six sections back the `Isolation backends` table; the rest back the capability tables.

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

## Nothing downgrades the boundary for you

A boundary is chosen once and used for months, on a host that changes in between. Record what can still reach that choice afterwards: a flag the user passes, a file the user did not write, or the host itself.

1. Record which boundary the tool used on an ordinary first run.
2. Read its flag list and configuration schema for anything that selects a weaker one.
3. Record whether reaching that weaker one takes the user's own command, or whether a configuration file or a host condition can reach it instead.
4. Invoke it again on a host lacking the strongest boundary's prerequisites.
5. Record whether it refuses, warns, or proceeds, and which boundary it then used.

### Boundary, vivarium

vivarium `ceb0027`, read 2026-08-19. Yes: one boundary, no second mode beneath it, so no flag, file, or host condition selects a weaker one. A host that cannot provide it gets a refusal rather than a substitute, which is what the [separate-kernel rule](../spec/08-invariants-and-guarantees.md) fixes.

### Boundary, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the engine is written into `/usr/share/flakes/<app>.yaml` at registration and no call-time pseudo-argument revisits it — but the drop-in directory `<app>.d/*.yaml` (alpha-ordered, last key wins) can rewrite an existing registration's options.

### Boundary, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `--no-microvm` selects the weaker boundary outright, and a failed probe reaches it anyway with a warning, so the boundary can change without the user asking. The stance is deliberate: a weaker sandbox beats no sandbox. vivarium's [separate-kernel rule](../spec/08-invariants-and-guarantees.md) takes the opposite position, and both are coherent.

### Boundary, podman

podman 5.x, read 2026-08-19. No: at the krun setup the boundary is a per-invocation flag. Omit `--runtime krun` and the same command runs the same image under the default runtime with no warning, and `containers.conf` can change that default in a file the user did not write. Nothing records that a workload was meant to run behind its own kernel.

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

## Work stays at its host path

Record the path the project occupies on the host and the path it occupies inside.

1. Put a project at a known absolute path on the host.
2. Give the tool that directory by its documented mechanism.
3. Inside, run `pwd` and read the path of a file the host also sees.
4. Record both paths, and whether a tool that stores absolute paths still resolves them.

### Host path, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker schema has no bind mount. `include.tar` and `include.path` copy a payload in at provisioning time — work is copied, not seen through. The published `--volume %HOME/ai:%HOME/ai` that does mirror a path is the `crun` container backend, and it mirrors a quarantine directory rather than the project tree.

### Host path, glaipnir

glaipnir `21ef389`, read 2026-08-18. Partial: since 1.0.0 a workspace under `$HOME` mounts at `/home/aiuser/<path relative to $HOME>` — a mirror of the tail, not of the path. The change answers the same failure class vivarium's [host-symmetric mount rule](../spec/08-invariants-and-guarantees.md) does: agents key session state on the working directory.

### Host path, podman

podman 5.x, read 2026-08-19. Partial: `-v /host/path:/host/path` mirrors any single path exactly, and under krun the mount crosses as virtiofs, so it holds at the compared setup. Nothing arranges it, nothing refuses a mismatch, and published examples usually pick a different target — available rather than provided.

## Host environment is deny-by-default

Record which host environment variables are visible inside.

1. Export a distinctive variable on the host.
2. Start the tool without naming that variable.
3. Inside, run `env` and record whether it appears, along with everything else that did.

### Environment, flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: at the firecracker boundary there is no environment passthrough at all, so nothing crosses. What it lacks is a policy of its own — at the container backend the set is whatever the registration froze into `--opt` lines, and a `%VAR` placeholder with no matching variable becomes the literal name rather than failing.

### Environment, glaipnir

glaipnir `21ef389`, read 2026-08-18. Partial: a fixed list crosses — `TERM` and `COLORTERM`, `GOOGLE_CLOUD_PROJECT` and `VERTEX_LOCATION` when set, plus five computed `AI_*` values. An explicit list rather than a wholesale copy, but not deny-by-default: the host-sourced four are forwarded whenever they exist.

## Session sockets refused as a source

Record what happens when the host's session directories are named as a share source.

1. Name `/tmp`, `${XDG_RUNTIME_DIR}`, or an ancestor of either as a mount source.
2. Start the tool.
3. Record the exit status, the message, and whether anything booted.

### Session sockets, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the [session-directories-never-cross rule](../spec/08-invariants-and-guarantees.md) refuses a mount whose `source` resolves to `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}`, or any ancestor, in either layer, before boot. A share conveys an inode, not a listener, so mounting a socket directory grants the exposure without the capability that motivated it.

### Session sockets, flake-pilot

flake-pilot `main`, read 2026-08-18. n/a: there is no bind-mount mechanism at the firecracker boundary, so there is nothing to refuse. Under the container backend a session socket is an ordinary `--opt "\-v ..."` and nothing objects — the shared-kernel answer, not this one.

## Use an SSH key without the key entering the sandbox

Record what has to cross the boundary before a tool inside can authenticate with a key the user already has.

1. Have a key the host's authentication agent already holds.
2. Make the tool inside the sandbox use it.
3. Record what is in the guest afterwards: the key, a copy of it, or neither.

### Key material, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: neither. The host agent's socket is relayed on a dedicated credential port of the same vsock-class transport the control plane uses, and the guest talks to `/run/vivarium/ssh-agent.sock`. What may be forwarded is a closed allowlist of two, `ssh` and `gpg`, declared through a typed option that names no host path — which is what lets a shareable piece declare it — and the GPG side takes the agent's restricted extra socket rather than the ordinary one. A mount could not do this at all: a socket's endpoint is an object in the kernel that owns the listener, so a guest with its own kernel finds a name with nothing behind it. Two limits are stated rather than engineered away: a compromised guest can use the key for as long as the session lasts, and a byte relay does not carry the signal an agent uses to recognise a forwarded connection.

### Key material, flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker boundary has neither a share nor a relay. A key reaches the guest only by being written into the image or into an `include.tar` / `include.path` payload, which is the material itself rather than its use.

### Key material, glaipnir

glaipnir `21ef389`, read 2026-08-18. No, by a different route: nothing is forwarded, and the design instead authenticates inside the sandbox and persists the result to a host cache directory the user owns. What ends up in the guest is a token rather than a private key, which is better than copying one — but an existing host key still cannot be used from inside.

### Key material, podman

podman 5.x, 2026-08-19. No: `-v $SSH_AUTH_SOCK` is the usual answer and it is a shared-kernel answer. Under `krun` the guest runs its own kernel, so the shared inode has no listener behind it, and podman relays no agent by any other route.

## Secrets are kept out of the built artifact

Record whether a credential can end up in the artifact the environment is built from, and who can read it if one does.

1. Read what the build consumes, and whether any documented flow puts a credential there.
2. Record what the tool does about it: a rule, a check, or nothing.
3. Record who else on the machine can read the artifact.

### Secrets in the build, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: a build-time secret is prohibited outright, and the reach of one is why the rule takes no exception. The store is shared read-only into every guest on the machine, so a secret in a store path is readable by every sandbox running there, including one deliberately running untrusted code. The rule is wider than "do not read a credential during the build": the manifest is itself compiled into a module and realised, so `[env] TOKEN = "…"` is a build-time secret whatever its launch-channel classification suggests. What replaces it is the agent channel above, a scoped short-lived value passed at launch, or an encrypted-at-rest scheme the user composes in — vivarium performs no decryption and holds no identity. One caveat the specification states itself: a plaintext secret is not decidable by inspection, so the `manifest-no-inline-secret` check warns heuristically, and a value it does not flag is not a promise.

### Secrets in the build, glaipnir

glaipnir `21ef389`, read 2026-08-18. Partial: the stance is stated up front and holds in the code — nothing is baked into the image, authentication happens at runtime inside the container, the token lands in a host cache directory the user owns, and the image carries the label `security.credentials="runtime-only"`. It is practice rather than a rule: build hooks run arbitrary commands as root at build time, so a user who puts a credential there gets it in the image and nothing objects.

## Credentials scoped per tool

Record whether one tool inside the sandbox can read another tool's credentials.

1. Authenticate two different tools so each writes its own credential directory.
2. Start the sandbox for one of them.
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

Record what the tool can reach on the network with no destination named. The row asks whether a default-deny posture is reachable at all, not what the tool does out of the box.

1. Configure the tool for its most restrictive documented network posture.
2. Inside, attempt a TCP connection to an arbitrary public address.
3. Attempt one to a destination the configuration names.
4. Record both results and how long each took to answer.

### Default-deny egress, vivarium

vivarium `ceb0027`, 2026-08-18. Yes, and open is the default: the [egress-defaults-open rule](../spec/08-invariants-and-guarantees.md) makes unrestricted egress the shipped posture and requires exactly one declarative knob to switch to a default-deny allowlist, which is `sandbox.egress.mode`. Open egress is stated not to weaken the boundary — the boundary is the microVM, and the cost of open egress is exfiltration exposure, a workload policy choice rather than a containment property.

The deny posture is enforced host-side, in the VM's own network namespace, because a guest holding root could tear down any ruleset it can see; denials are rejected rather than dropped, so a blocked attempt fails in milliseconds instead of hanging. Both modes run today: a denied name answered `REFUSED` in 9 ms and a denied literal connect reset in 15 ms, measured 2026-08-14 and recorded in [`implementation-status.md`](../implementation-status.md). Details in [`spec/05-networking-and-egress.md`](../spec/05-networking-and-egress.md).

### Default-deny egress, flake-pilot

flake-pilot `main`, read 2026-08-18, re-read 2026-08-20. Partial, and by absence rather than by policy. Upstream states that firecracker "supports networking only through TUN/TAP devices" and that "it is the user's responsibility to set up the routing on the host from the TUN/TAP device to the outside world", then walks a static-IP NAT setup: `ip_forward`, a MASQUERADE rule, a `tap-<app>` device per registration, and `boot_args` edited from `ip=dhcp` to a static triple. Until an operator does that work a microVM reaches nothing, and `flake-ctl firecracker register --no-net` keeps it that way deliberately. All-or-nothing: nothing readmits a named destination.

### Default-deny egress, podman

podman 5.x, read 2026-08-19. Partial: `--network none` is genuinely default-deny and the network posture is podman's, outside the OCI runtime, so it applies at the krun setup too. All-or-nothing — readmitting named destinations needs a firewall the user maintains outside podman.

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

flake-pilot `main`, read 2026-08-18. Partial: reachable but entirely the operator's job — a TAP device per instance (`@NAME` names it), `ip_forward`, MASQUERADE, hand-edited `boot_args`. Nothing in the tool arranges any of it.

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

## Compose the environment from separate, reusable parts

Record how a second concern is added to an environment that already has one.

1. Define an environment carrying one concern.
2. Add a second concern written by somebody else, without editing the first.
3. Record what happens when the two set the same value.

### Composition, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: an image and an ordered list of pieces are imported as NixOS modules and merged by the module system, with no vivarium merge engine of its own. Lists concatenate, so every layer contributes to the package set, the mount list, and the egress allowlist. Scalars resolve by priority rather than by position: a shared piece proposes with `mkDefault` and the user's manifest outranks it, a policy floor uses `mkForce` and nothing outranks that, and two definitions surviving at the same priority fail evaluation with `65` rather than being settled by order. That last rule is what lets two independently written pieces be adopted together — a collision is reported, never resolved behind the user's back.

### Composition, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: two mechanisms, neither of which merges. An image composes by OCI layering, `--base` for a delta container and a repeatable ordered `--layer`; a registration composes by drop-in, `<app>.d/*.yaml` read in alpha order with the last key winning. Two drop-ins setting one key is neither a conflict nor a merge — the later filename wins — so adopting a second author's file can undo the first's without saying so.

### Composition, glaipnir

glaipnir `21ef389`, read 2026-08-18. Partial: the extension surface is ordered `NN-*.sh` drop-in hooks, run as root at build and as `aiuser` on every start, plus a `PACKAGES=(...)` array interpolated into the base install line. Hooks compose the way shell does, by running one after another; there is no declaration for two of them to disagree over, and so no stage at which a disagreement could be reported.

### Composition, podman

podman 5.x, 2026-08-19. Partial: a `Containerfile` composes linearly — one `FROM` and a sequence of steps, with multi-stage builds copying artifacts between stages. There is exactly one base, so two bases cannot be adopted together, and combining two authors' work means editing one file into the other by hand.

## A config unit works unchanged on someone else's machine

Record what a user can hand a colleague so the colleague's environment gains the same concern.

1. Configure one concern that needs a package and a host path.
2. Identify the smallest unit carrying it, and hand that unit to a second user.
3. Record what the second user edits before it works.

### Config unit, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: images and pieces are the shared class and the manifest is the personal one. A shared layer reaches the host only through portable variables — `${HOME}` and the four durable XDG directories — which every host resolves, and expansion happens at launch, so no expanded path is ever written into the artifact. A literal personal path in a shared layer fails evaluation with `65` before the build, so this is enforced rather than advised. A piece carries its whole concern — packages, guest config, mounts, environment, and any third-party flake input it needs through its own `inputs.toml` — so adopting it is one name in the colleague's `pieces` list. The same split is how a team makes a guarantee unwaivable: a piece setting a value with `mkForce` outranks every personal manifest.

### Config unit, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: what travels is the image and the registration. A colleague reruns `flake-ctl firecracker register` with the same flags, or copies an `<app>.d/*.yaml` drop-in into place. Both work; neither is portable by construction, because the firecracker registration names local file paths under `/var/lib/firecracker/images/` and a drop-in carrying a host path carries it literally, with nothing checking.

### Config unit, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: configuration is one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per concern — there is no unit smaller than that file, and it is the user's own machine-local settings. The hooks directory can be copied by hand, which is file transfer rather than adoption.

### Config unit, podman

podman 5.x, 2026-08-19. Partial: a `Containerfile` travels and reproduces its build steps elsewhere, and a published image travels as a pull. Neither is a unit of one concern — the smallest shareable thing is the whole environment — and nothing checks a `Containerfile` for a host path the colleague does not have.

## The same definition rebuilds the same environment

Record whether the same definition produces the same environment on a second machine or at a later date.

1. Record the definition and every version it names.
2. Reproduce the environment from it on a second machine.
3. Record what identifies the result, and whether the two are the same object or merely similar.

### Same definition, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the firecracker unit is a versioned tarball fetched by URL into `/var/lib/firecracker/images/<name>/`, and the registration names local file paths, so the same URL gives a second machine the same environment. Nothing pins it: no digest, and the definition it was built from is not carried.

### Same definition, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `Containerfile.agent` starts `FROM` a per-agent image at `:latest`, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice.

### Same definition, podman

podman 5.x, 2026-08-18. Partial: a digest reference reproduces an image exactly, and nothing arranges one — the published form is a tag, and a `Containerfile`'s `RUN` steps re-execute against whatever the network serves that day.

## Everyone building it gets the versions you got

Record what fixes the versions inside the environment, and whether a second person can hold the same answer.

1. Build the environment and record what fixed each version in it.
2. Hand the definition to a second user who builds it a month later.
3. Record what that user would have to be given to get the versions the first one got.

### Pinned versions, vivarium

vivarium `ceb0027`, 2026-08-18. Yes: exactly one lockfile is in force, and it pins the whole input graph rather than a revision here and there. By default it is the per-target lock vivarium owns under the data root, created by the first evaluation and moved only by `viv update`; a team that wants one shared answer puts a read-only override lock beside the manifest, and it then outranks the tool-owned lock for everybody. `viv config` reports which of the two is in force. A piece declaring a flake input the lock has no node for fails closed with `78`, naming the input and the artifact, rather than resolving it quietly — an unannounced input jump is exactly what the pin exists to prevent.

### Pinned versions, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the firecracker registration names a versioned tarball fetched by URL into `/var/lib/firecracker/images/<name>/`, so a second user given the same URL gets the same rootfs. Nothing pins it — no digest, and the definition it was built from is not carried — so what the colleague holds is a name that has resolved to the same bytes so far.

### Pinned versions, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `Containerfile.agent` starts `FROM` a `:latest` tag, installs whatever `PACKAGES=(...)` names, and runs whatever build hooks are present. There is nothing to hand a second user that fixes any of the three.

### Pinned versions, podman

podman 5.x, 2026-08-19. Partial: an image referenced by digest is exactly one artifact, and a colleague given the digest gets it. Nothing arranges that — the published form is a tag, and a `Containerfile` rebuilt on the colleague's machine re-executes its `RUN` steps against whatever the network serves that day.

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

## Re-enter a running instance

Record whether a second command can join an instance that is already running.

1. Start the instance and leave a process running inside it.
2. From a second terminal, run another command against the same instance.
3. Record whether it joined the existing instance or created another.

### Re-entry, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the instance survives between calls, a second session into it does not. `--resume --force-vsock` keeps the VM alive host-side so the next call reaches it over the vsock, but the guest init is `sci`, which executes the one command named by `run=` and reboots. Nothing is left in the guest to hand out a second shell, so two concurrent sessions are two VMs, separated by a call-time suffix that for firecracker also names the TAP device:

```bash
claude @projA        # one VM: instance projA, tap-claude@projA
claude @projB        # a second VM: its own overlay, memory, and TAP device
```

Two VMs is not the same capability as two shells: the sessions share no working tree, no running process, and no warm state. Registering the app as a multiplexer or an `sshd` would buy that back, at the cost of building the in-guest supervisor flake-pilot does not ship. The `--attach` flag that would join a running instance exists only in `flake-ctl podman register`; the firecracker registration has no such flag, and the full `exec`/`attach` ergonomics belong to the `crun` container backend.

### Re-entry, glaipnir

glaipnir `21ef389`, read 2026-08-18. No: a running krun container cannot be entered, so the script counts what exists and starts a numbered sibling instead. The microVM boundary and re-entry are mutually exclusive here; the `podman exec` resume path is the container backend.

### Re-entry, podman

podman 5.x, read 2026-08-19. No: `podman exec` cannot enter a krun container — there is no in-guest agent to inject a process into, the same reason it fails in the other two tools. The exec that works is the shared-kernel runtime's.

## Installs from a distro package

Record what a user must already have before the tool can be installed.

1. On a machine with the tool absent, follow its documented install.
2. Record every prerequisite and every command.

### Install, vivarium

vivarium `ceb0027`, 2026-08-18. No: installation is Nix-native — the user needs Nix and a host with KVM. The other three install with one `zypper` or `apt` line. An open gap: nothing forecloses distribution packaging, and none exists.

## Feels like a native command

Record what the user types to run the sandboxed tool.

1. Complete the tool's setup for one application.
2. Record the exact invocation a user types afterwards, and whether it names the sandbox.

### Native command, vivarium

vivarium `ceb0027`, 2026-08-18. No: running something inside is `viv exec -- <command>` or `viv shell`, so the sandbox is always named — legible but not invisible. flake-pilot's symlink puts the sandboxed `claude` on `PATH`. An open gap: no decision for or against a shim.

### Native command, glaipnir

glaipnir `21ef389`, read 2026-08-18. Partial: `glaipnir run claude` names the sandbox, but the tool is one word away and the agent roster is built in — between flake-pilot's invisible symlink and a general-purpose wrapper.

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

## See every sandbox on the machine

Record how a user finds every sandbox that exists, running or not.

1. Create sandboxes for three different projects and leave one running.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed and what is missing.

### Enumerate, flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: `flake-ctl list` reports registrations — name, engine, config path — not instances. Podman instances live in a separate storage root needing `CONTAINERS_STORAGE_CONF` to see, and firecracker instances are processes with TAP devices that nothing enumerates.

### Enumerate, vivarium

vivarium `ceb0027`, 2026-08-18. Specified, not built: `viv status` reports the current project; `viv status -g` and `viv stop --all` are specified and do not run — the `*`. The machine-wide view is the one thing all three alternatives provide today and vivarium does not.
