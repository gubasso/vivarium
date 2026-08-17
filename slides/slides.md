---
theme: default
colorSchema: all
title: vivarium — secure sandbox factory
info: |
  ## vivarium
  A Nix-native tool that boots each project inside its own microVM.
layout: cover
class: text-center
transition: slide-left
---

# vivarium

<div class="tagline">
Secure sandbox factory
</div>

<div class="subtitle">
One manifest. A separate guest kernel behind a hardware-virtualization boundary.
</div>

<!--
Landing slide, and the whole pitch in two lines. Factory is the load-bearing
word: the product is not one sandbox but the thing that produces one per
project, on demand, from a manifest. The subtitle carries the claim the rest of
the deck spends its time earning — a separate guest kernel, not namespaces drawn
around a shared one. Nix is deliberately not named here; it arrives once the
boundary has been argued for.
-->

---
layout: default
---

# Premises

If none of these sound like you, this project probably isn't for you.

<div class="premises">

- <carbon-locked class="inline" /> &nbsp; You care about security.

- <carbon-flow class="inline" /> &nbsp; You want to multi-task.

- <carbon-document class="inline" /> &nbsp; You prefer declarative, composable, shareable config.

- <carbon-bot class="inline" /> &nbsp; You use AI agents.

- <carbon-fire class="inline" /> &nbsp; YOLO mode is the correct mode.

- <carbon-tool-kit class="inline" /> &nbsp; Your agents need your tooling.

- <carbon-user-multiple class="inline" /> &nbsp; You work on a team.

</div>

<!--
These are premises, not promises. Each one is a claim the rest of the deck has
to earn: if the reader nods at all seven, the microVM boundary is worth its cost.

The YOLO line is the joke that is also the thesis, and the only line that drops
the second person — a flat assertion lands the beat better than an opinion
attributed to the audience. Approval prompts are a tax paid for a boundary
nobody trusts: every one of them asks a human to be the security control,
interrupts the work, and gets waved through by the tenth time anyway. Given a
boundary that actually holds, unrestricted is the correct mode, and the prompts
were only ever compensating for the missing wall. Land it as a laugh, then say
the serious half if the room is with you.
-->

---
layout: default
---

# Why a sandbox

<div class="risk-grid">

<div class="risk-col risk-out">
  <div class="risk-head">
    <carbon-warning-alt class="text-3xl" />
    <span>What leaks out</span>
  </div>
  <ul>
    <li><carbon-password class="inline" /> &nbsp; Secrets</li>
    <li><carbon-user-profile class="inline" /> &nbsp; Personal files</li>
    <li><carbon-code class="inline" /> &nbsp; Untrusted code, on your host</li>
  </ul>
</div>

<div class="risk-col risk-in">
  <div class="risk-head">
    <carbon-error class="text-3xl" />
    <span>What goes wrong inside</span>
  </div>
  <ul>
    <li><carbon-migrate class="inline" /> &nbsp; Drift</li>
    <li><carbon-idea class="inline" /> &nbsp; Hallucination</li>
    <li><carbon-edit class="inline" /> &nbsp; Edits nobody asked for</li>
  </ul>
</div>

</div>

<div class="risk-note">Not hypothetical. Both happen, eventually, to everyone.</div>

<!--
Two failure directions, not one. The first is the community's usual worry:
an agent on a bare host reaches every file the user reaches. The second is the
one people underplay: agents drift and touch what they were not asked to touch.
A boundary has to answer both, which is why the answer is a boundary rather
than a better prompt.
-->

---
layout: default
---

# But the agents ship sandboxes now

<div class="quotes">

<div class="quote-card">
  <div class="quote-text">Sandboxing reduces risk but is not a complete isolation boundary.</div>
  <div class="quote-src">Claude Code docs</div>
</div>

<div class="quote-card">
  <div class="quote-text">Not a complete, ready-made sandbox with a specific security policy.</div>
  <div class="quote-src">bubblewrap README</div>
</div>

<div class="quote-card">
  <div class="quote-text">Sandboxing reduces but doesn't eliminate all risks.</div>
  <div class="quote-src">Gemini CLI docs · off by default</div>
</div>

<div class="quote-card">
  <div class="quote-text">A malicious project can exfiltrate anything available inside the devcontainer, including Codex credentials.</div>
  <div class="quote-src">Codex CLI docs · sandbox covers local commands, not MCP</div>
</div>

<div class="quote-card quote-wide">
  <div class="quote-text">Process isolation that shares the host kernel is the wrong shape for this problem.</div>
  <div class="quote-src">Pillar Security · 8 escapes, 4 agents, 1 week</div>
</div>

</div>

<div class="verdict">
  <carbon-virtual-machine class="verdict-icon" />
  <div>
    <div class="verdict-text">A dedicated virtual machine provides the strongest separation, with its own kernel.</div>
    <div class="verdict-src">Claude Code docs — the recommendation for untrusted code</div>
  </div>
</div>

<!--
Not an argument against the vendors — an argument from them. Every native
sandbox is a policy written in host-kernel primitives and enforced by the same
kernel that runs the agent, so the boundary and the code it constrains share an
implementation. The published escapes are policy bugs, not kernel zero-days,
which is exactly what bubblewrap warns about. The last line is the vendor of the
most-used coding agent naming this project's architecture as the top of the
ladder.

Codex is the one to expand on if asked: its sandbox covers local commands only,
and its own docs list what stays outside — web search, connector tools, MCP
server connections, browser and Computer Use activity. The quote on the card is
about the container path specifically, which is the same wall the previous slide
named. Linux primitives are bwrap plus seccomp now, not Landlock; most write-ups
are stale on this.

Sources: code.claude.com/docs/en/sandboxing, github.com/containers/bubblewrap,
google-gemini.github.io/gemini-cli, learn.chatgpt.com/docs/agent-approvals-security,
pillar.security "Week of Sandbox Escapes" (July 2026).
-->

---
layout: default
class: meme-slide
---

<div class="meme">
  <img src="/meme.png" alt="Boardroom meeting suggestion: asked how to safely sandbox coding agents running untrusted code, one answer is give each agent a fully isolated environment, another is never give the agent root access to the host, and the third — just run it in Docker — gets the speaker thrown out the window." />
</div>

<!--
The comic beat after the quotes slide, and the pivot into the walkthrough. The
two suggestions that survive the meeting are the two requirements this project
took seriously; the one that gets thrown out the window is the iteration I
actually shipped before this one, which is why the next slide is honest about
where it stopped.
-->

---
layout: default
---

# Past the shared kernel

<div class="search-q">
  <div class="search-q-text">What gives a container's ergonomics without a container's limits?</div>
</div>

<div class="search-flow">

<div class="search-item">
  <div class="search-head"><carbon-close-outline class="inline" /> &nbsp; The limit</div>
  <div class="search-body">
    One kernel, shared with the host. Architecture, not configuration.
  </div>
</div>

<div class="search-arrow"><carbon-arrow-right /></div>

<div class="search-item">
  <div class="search-head"><carbon-search class="inline" /> &nbsp; The finding</div>
  <div class="search-body">
    <code>microVM</code> — its own kernel, a container's footprint.
  </div>
</div>

<div class="search-arrow"><carbon-arrow-right /></div>

<div class="search-item search-item-proof">
  <div class="search-head"><carbon-flash class="inline" /> &nbsp; The proof</div>
  <div class="search-body">
    Firecracker. AWS runs tenant code one microVM apiece, in production.
  </div>
</div>

</div>

<div class="verdict">
  <carbon-idea class="verdict-icon" />
  <div>
    <div class="verdict-text">Not a better container — a VM small enough to use like one.</div>
  </div>
</div>

<!--
The research beat, and the arrow the previous slide left implicit between dctl
and vivarium. The point is that the arrival was found, not preferred: the wall
was architectural, so the fix had to be architectural too.

Firecracker is the example here, not the implementation — it is what proved a
per-tenant VM is practical at scale. vivarium ships Cloud Hypervisor, the same
capability class, because Firecracker's device model has no virtio-fs and this
tool shares a live workspace into the guest. The boundary contract names the
class rather than the binary, so the choice is an implementation one. Say any of
that only if asked; the slide's job is the turn.

Sources: firecracker-microvm.github.io; ADR-0001 (boundary is a capability
class), ADR-0025 (Cloud Hypervisor as the shipped default).
-->

---
layout: default
---

# OCI ergonomics, VM boundary

<div class="try-grid">

<div class="try-col">
  <div class="try-head"><carbon-tools class="inline" /> &nbsp; The attempt</div>
  <div class="try-stack">
    <span>Podman</span><span>krun · libkrun</span><span>OCI images</span><span>devcontainer.json</span>
  </div>
  <div class="try-body">Keep the dev-container workflow. Swap the runtime underneath for a KVM microVM.</div>
  <div class="try-src">github.com/gubasso/podbox</div>
</div>

<div class="try-col">
  <div class="try-head"><carbon-group class="inline" /> &nbsp; Same idea, already shipping</div>

  <div class="try-ref">
    <div class="try-ref-text">Rootless Podman plus libkrun — one microVM per agent.</div>
    <div class="try-src">GLAIPNIR &nbsp;·&nbsp; val4oss/ai-agents-sandbox &nbsp;·&nbsp; Valentin Lefebvre</div>
  </div>

  <div class="try-ref">
    <div class="try-ref-text">Register a tool as a command; it launches into a container or a VM.</div>
    <div class="try-src">flake-pilot &nbsp;·&nbsp; OSInside &nbsp;·&nbsp; Marcus Schäfer</div>
  </div>
</div>

</div>

<!--
The iteration between the finding and this project, and the one that proves the
finding was right about the boundary and wrong about the door. The stack is the
obvious move: keep everything the OCI ecosystem already gives — images,
registries, devcontainer layers, a workflow people know — and replace only the
runtime with a microVM. Podman plus krun does exactly that, in one flag.

Credit is deliberate here, and worth saying out loud: this is not a niche idea
and vivarium is not first. Valentin Lefebvre's GLAIPNIR runs coding agents on
rootless Podman with libkrun today, and Marcus Schäfer's flake-pilot has been
launching registered applications into podman or firecracker with a native feel
for years, with prebuilt agent containers in a public registry. If someone in
the room wants a working answer this week rather than a Nix one, point them
there.

The wall is the same one both hit, which is why it is a property and not a bug:
krun's OCI handler cannot exec, because a microVM has no agent inside it to
spawn a secondary process, so flake-pilot's own docs rule out the resume feature
under krun. A workspace tool lives on exec — a second shell, a test run beside
the agent, reattaching to a session. So the boundary has to be entered rather
than wrapped, which is the guest agent and the live share vivarium builds.

Sources: github.com/val4oss/ai-agents-sandbox, github.com/OSInside/flake-pilot
(podman-pilot registration docs, krun runtime caveat), github.com/gubasso/podbox.
-->

---
layout: default
---

# How I got here

<div class="walk">

<div class="walk-step">
  <div class="walk-dot"><carbon-laptop /></div>
  <div class="walk-body">
    <div class="walk-title">Agents on the bare host</div>
    <div class="walk-lines">
      <div class="walk-win"><carbon-checkmark /> Nothing to set up</div>
      <div class="walk-wall"><carbon-close /> Every file I can read, it can read</div>
    </div>
  </div>
</div>

<div class="walk-step">
  <div class="walk-dot"><carbon-bare-metal-server /></div>
  <div class="walk-body">
    <div class="walk-title"><code>dev-sandbox</code> <span class="walk-tech">systemd-nspawn</span></div>
    <div class="walk-lines">
      <div class="walk-win"><carbon-checkmark /> A real boundary, on my machine</div>
      <div class="walk-wall"><carbon-close /> My distro, my host, my setup — not yours</div>
    </div>
  </div>
</div>

<div class="walk-step">
  <div class="walk-dot"><carbon-container-software /></div>
  <div class="walk-body">
    <div class="walk-title"><code>dctl</code> <span class="walk-tech">Dev Containers · OCI</span></div>
    <div class="walk-lines">
      <div class="walk-win"><carbon-checkmark /> Portable, composable, one env per project</div>
      <div class="walk-wall"><carbon-close /> Still your kernel underneath</div>
    </div>
  </div>
</div>

<div class="walk-step">
  <div class="walk-dot"><carbon-box /></div>
  <div class="walk-body">
    <div class="walk-title"><code>podbox</code> <span class="walk-tech">Podman · krun</span></div>
    <div class="walk-lines">
      <div class="walk-win"><carbon-checkmark /> OCI ergonomics over a real boundary</div>
      <div class="walk-wall"><carbon-close /> No <code>exec</code> into the guest — one shell, no resume</div>
    </div>
    <div class="walk-sub">
      <div class="walk-sub-item">
        <div class="walk-sub-name"><carbon-arrow-right /> <code>GLAIPNIR</code></div>
        <div class="walk-sub-note">Podman + libkrun, one microVM per agent</div>
      </div>
      <div class="walk-sub-item">
        <div class="walk-sub-name"><carbon-arrow-right /> <code>flake-pilot</code></div>
        <div class="walk-sub-note">A registered command, launched into a VM</div>
      </div>
    </div>
  </div>
</div>

</div>

<!--
Each row keeps the thing that worked and names the wall that forced the next
attempt, so the arrival is a consequence rather than a preference. Row one is
an attempt too: it is what most people are running today.

The rail deliberately stops on a wall rather than on an answer. Every row here
is behind us; what comes next is the answer, and it gets its own slide instead
of a line at the bottom of this one. Pause on the last wall before advancing.

The two indented entries under the podbox row are other people's tools, run
rather than read about: GLAIPNIR and flake-pilot reach the same stack from
different directions and stop at the same place. Three attempts hitting one wall
is what makes the wall a property of the approach rather than a gap in mine.
-->

---
layout: center
class: text-center
---

<div class="section-kicker">The answer</div>

# vivarium

<div class="section-lead">
Nix describes the sandbox. One manifest, one microVM per project.
</div>

<!--
The turn, and the halfway mark. Everything before this slide is the problem and
four attempts at it; everything after is the tool. It is a poster, not a page —
dark in both light and dark mode, one word on it, so the room resets before the
second half starts.

This is also where Nix finally gets named. The cover keeps it out on purpose:
the boundary has to be worth wanting before the way it is described matters. By
now the audience has seen why a shared kernel fails and why wrapping a VM in the
OCI runtime interface stops at exec, so a declarative description of the whole
sandbox reads as the fix rather than as a preference.
-->

---
layout: default
---

# How it fits together

<div class="arch">

<div class="arch-lane">
  <div class="arch-tag">Build — once per change</div>
  <div class="arch-flow">
    <div class="arch-node">
      <div class="arch-node-name"><carbon-document /> vivarium.toml</div>
      <div class="arch-node-sub">one image, a list of pieces</div>
    </div>
    <div class="arch-arrow"><carbon-arrow-right /></div>
    <div class="arch-node arch-node-viv">
      <div class="arch-node-name"><carbon-terminal /> viv</div>
      <div class="arch-node-sub">Rust — resolves names, emits a flake</div>
    </div>
    <div class="arch-arrow"><carbon-arrow-right /></div>
    <div class="arch-node">
      <div class="arch-node-name"><carbon-cube /> nix build</div>
      <div class="arch-node-sub">microvm.nix + nixpkgs, merged by priority</div>
    </div>
    <div class="arch-arrow"><carbon-arrow-right /></div>
    <div class="arch-node arch-node-out">
      <div class="arch-node-name"><carbon-package /> runner closure</div>
      <div class="arch-node-sub">kernel, system, VMM — one lock pins it</div>
    </div>
  </div>
</div>

<div class="arch-lane">
  <div class="arch-tag">Run — one supervised lifetime, no daemon, no root</div>
  <div class="arch-run">
    <div class="arch-box">
      <div class="arch-box-tag"><carbon-terminal /> Host, as your user</div>
      <div class="arch-row"><span class="arch-row-name">viv</span><span class="arch-row-note">supervises, owns state and logs</span></div>
      <div class="arch-row"><span class="arch-row-name">cloud-hypervisor</span><span class="arch-row-note">Rust VMM · KVM · seccomp · Landlock</span></div>
      <div class="arch-row"><span class="arch-row-name">virtiofsd</span><span class="arch-row-note">one confined daemon per share</span></div>
    </div>
    <div class="arch-arrow arch-arrow-big"><carbon-arrow-right /></div>
    <div class="arch-box arch-box-guest">
      <div class="arch-box-tag"><carbon-virtual-machine /> Guest microVM</div>
      <div class="arch-row"><span class="arch-row-name">its own kernel</span><span class="arch-via">KVM</span><span class="arch-row-note">NixOS, direct boot</span></div>
      <div class="arch-row"><span class="arch-row-name">/nix/store</span><span class="arch-via">virtio-fs</span><span class="arch-row-note">host store, read-only</span></div>
      <div class="arch-row"><span class="arch-row-name">/workspace</span><span class="arch-via">virtio-fs</span><span class="arch-row-note">your project, live</span></div>
      <div class="arch-row"><span class="arch-row-name">guest agent</span><span class="arch-via">vsock</span><span class="arch-row-note"><code>viv exec</code>, <code>viv shell</code></span></div>
    </div>
  </div>
</div>

<div class="arch-thesis">vivarium orchestrates. Nix evaluates and builds. The hypervisor isolates. No layer reimplements the one below it.</div>

</div>

<!--
The figure to spend time on. Read it as two lanes, and say the lane names out
loud: build happens once per change, run happens per session, and nothing in the
run lane can add an input the build lane did not already pin.

Top lane, left to right. The manifest is TOML on purpose — a name-list a reader
can grep, not a program. viv resolves those names to module files and emits a
generated flake; that is the whole of its build job, and it deliberately does no
merging. The NixOS module system merges, because vivarium builds NixOS guests
and the merge engine is already there: pieces import, lists concatenate, scalar
conflicts resolve by priority. What comes back is one closure holding the guest
kernel, the guest system, and the hypervisor itself.

Bottom lane. Everything on the host runs as the invoking user — no daemon, no
setuid, no privileged socket. The hypervisor is Cloud Hypervisor: a Rust VMM
about twenty times smaller than QEMU, exposing only virtio devices, with seccomp
on by default and Landlock where the host has it. It is not on $PATH; it is a
member of the closure Nix just built, so its version is settled by the lockfile
rather than by whatever the distro shipped. vivarium generates its launch
arguments itself instead of taking microvm.nix's ready-made runner, because that
runner writes host paths into the build output and this project forbids a host
path in any build artifact.

Right box, and the point of the via chips: every one of those four rows crosses
the boundary through a virtual device. The kernel through KVM, the filesystems
through virtio-fs, control through vsock. Nothing crosses by sharing a namespace,
which is exactly what the previous half of the deck said was missing. The store
row is the one to linger on if there is time — the guest reads the host's store
rather than carrying a copy, which is what makes the fifth sandbox nearly free.
The agent row is the answer to podbox's wall: exec works because something is
listening inside.

Sources: ADR-0002 (module system as the composition engine), ADR-0004 (TOML
compiles to a generated flake), ADR-0048 (guest module only, vivarium owns the
launch), ADR-0049 (the backend is a closure member), ADR-0038 (host store shared
read-only), ADR-0016 and ADR-0065 (guest agent over vsock).
-->

---
layout: default
---

# Why Nix

<div class="cmp">

<div class="cmp-head"></div>
<div class="cmp-head">Podman + an OCI image</div>
<div class="cmp-head cmp-head-ours">vivarium + a Nix manifest</div>

<div class="cmp-axis">Adding a tool</div>
<div class="cmp-cell">Edit the recipe, rebuild the image, re-publish it if others use it.</div>
<div class="cmp-cell cmp-cell-ours">Add a name to the manifest. There is no image to publish.</div>

<div class="cmp-axis">Handing it to a teammate</div>
<div class="cmp-cell">The image reproduces. The recipe that built it does not.</div>
<div class="cmp-cell cmp-cell-ours">The manifest and its lock rebuild the same VM from source.</div>

<div class="cmp-axis">Reusing config across projects</div>
<div class="cmp-cell">Copy recipe lines, or maintain a base image of your own.</div>
<div class="cmp-cell cmp-cell-ours">One piece, imported by many manifests. Change it in one place.</div>

<div class="cmp-axis">Disk, five projects in</div>
<div class="cmp-cell">Layers dedupe only while two images agree from the base down.</div>
<div class="cmp-cell cmp-cell-ours">Guests share the host store path by path. No per-VM image at all.</div>

</div>

<div class="done-band">
  <carbon-checkmark-filled class="done-icon" />
  <div>
    <div class="done-title">And the hard part was already written.</div>
    <div class="done-text">microvm.nix boots NixOS as a microVM: kernel wiring, mount units, volumes, interfaces. vivarium imports it and adds the launch. That is why one person can maintain this.</div>
  </div>
</div>

<!--
Four things a user actually does, and what each one costs on either side. Read
the rows, not the columns — and say plainly that the left column is a working
tool a lot of people are happy with, because the moment this looks like a
strawman the room stops believing the right column too.

Adding a tool. The rebuild happens on both sides; that is not the difference.
The difference is that on the left there is an artifact — an image — that has to
be built and then distributed to anyone else who needs it, and on the right the
manifest is the artifact. Nothing gets pushed anywhere.

Handing it to a teammate. Be precise here, because the sloppy version of this
claim is wrong: an OCI image is perfectly reproducible as bytes. What is not
reproducible is the recipe. Rebuild a Containerfile six months later and the
package manager resolves differently, so you get a working environment that is
not the one you tested. The manifest plus its lock rebuild from source and land
on the same closure. The honest catch is that the teammate needs Nix, which is
the next slide.

Reusing config. Base images and devcontainer features do give real reuse, so do
not oversell this one. The difference is what happens when two reusable things
must combine: image inheritance is a single chain, while pieces merge by
priority, so a base can set a default that a project overrides without either
knowing about the other.

Disk. This is the most concrete row and the one with a number behind it. Note the
precise claim: container layers do dedupe, but only from the base down to the
first point where two images disagree. Nix shares the individual store path, so
two sandboxes that agree on one dependency share exactly that dependency whatever
else they differ on. The fifth project costs what only the fifth project needs.

The band is the one to say with feeling if the room is Nix-literate. The hardest
part of this project would have been guest integration, and it was already
written by someone else: kernel and initrd wiring, mount units per share,
volumes, interfaces. vivarium declines only the host half — the ready-made runner
and its supervisor — and that is a fit problem with host paths and confinement,
not a criticism of upstream.

Deliberately not on this slide: the hypervisor being pinned inside the closure.
It is true and it matters to me, and no user has ever cared which build of a VMM
they are running. It stays in the architecture slide's notes.

Sources: spec/00 (goals — declarative, deterministic, composable, cheap to run
several), ADR-0002 and ADR-0003 (module system, pieces), ADR-0004 (TOML manifest),
ADR-0038 and ADR-0086 (store sharing, per-VM copy refused), ADR-0048 (guest
module only), explanation/disk-model-vs-containers.
-->

---
layout: default
---

# What it costs

<div class="con-grid">

<div class="con-item">
  <div class="con-head"><carbon-cloud-offline /> Nothing to pull</div>
  <div class="con-text">Docker Hub, and every dev-container config already published, are out of reach. Environments here are written in Nix instead.</div>
</div>

<div class="con-item">
  <div class="con-head"><carbon-hourglass /> The first run takes minutes</div>
  <div class="con-text">Podman downloads an image someone already built. vivarium builds yours. Later runs reuse it.</div>
</div>

<div class="con-item">
  <div class="con-head"><carbon-education /> You will meet Nix</div>
  <div class="con-text">The manifest is plain TOML, and most days that is all you touch. Anything unusual — or any error message — is Nix.</div>
</div>

<div class="con-item">
  <div class="con-head"><carbon-code /> More of this stack is mine</div>
  <div class="con-text">Podman and krun are proven by millions of users. The part that gets you a shell inside the VM is mine, and it is new.</div>
</div>

<div class="con-item">
  <div class="con-head"><carbon-view /> The sandbox sees your package list</div>
  <div class="con-text">Sharing the store keeps disk cheap. It also lets code in the sandbox see what you have installed — the list, not your files.</div>
</div>

<div class="con-item">
  <div class="con-head"><carbon-download /> Security updates come from us</div>
  <div class="con-text">vivarium fixes every version it builds with. So a fix reaches you when this project ships an update, not when your distro does.</div>
</div>

</div>

<div class="con-note">Every one of these is the price of something on the last slide. None is an accident, and none goes away by trying harder.</div>

<!--
Say this slide slowly, and do not soften any of it. A comparison that only lists
wins is an advertisement and the room can tell. This is also where the Podman
path stays genuinely the better choice for some people, which is why those two
projects were credited by name earlier rather than merely mentioned.

Nothing to pull is the biggest loss and the least fixable. There is a public
registry of prebuilt agent containers on the flake-pilot side, and nothing here
can consume it. Every environment is described from nixpkgs instead, which is a
real sacrifice of other people's finished work.

The first run is only slow once per change, and after that a start reuses the
build. Worth adding that Nix itself has to be on the machine — it is the one
thing this project asks the host for rather than bringing along.

More of this stack is mine is the one to be plainest about, because it is the
price of the win two slides back. podbox and flake-pilot both stop where they do
because a microVM has nothing inside it to start a second process; the only way
past that is to put something inside and talk to it, so that something is mine
to write and mine to keep correct. Millions of people have found Podman's bugs
for me. Nobody has found mine yet.

The package list is a decision, not an oversight, and the honest framing is the
narrow one: what leaks is which software is installed, not any file. The store
holds no secrets and the threat being defended against is escape rather than
snooping — so the trade was taken openly, and the alternative was refused
outright, because giving every VM its own copy of the store would destroy the
disk property the previous slide sells.

Security updates coming from us is what "we lock every version" costs. Locking is
why a teammate gets the same VM, and the same mechanism means a fix in the
hypervisor does not arrive through your package manager. It arrives when this
project publishes a new lock and you update. That obligation has a named owner
here rather than being left implicit.

One more if someone asks about upstream risk: importing microvm.nix's guest
module means depending on option names upstream makes no promise about. A rename
there gives you a VM that boots with an empty workspace and no error at all,
which is exactly why a boot-and-mount test exists.

Sources: ADR-0001 (heavier startup than a container), ADR-0002 (authors must
learn priority merge), ADR-0038 (a guest can enumerate the host closure),
ADR-0048 (upstream option names carry no stability policy), ADR-0049 with
ADR-0078 (a backend fix arrives as a lock move, and who owes it), ADR-0016 (the
guest agent and its protocol are ours), spec/00 audience.
-->
