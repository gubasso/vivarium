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

- <carbon-renew class="inline" /> &nbsp; You want determinism — the same environment every time, not a similar one.

- <carbon-bot class="inline" /> &nbsp; You use AI agents, and they need your tooling.

- <carbon-fire class="inline" /> &nbsp; YOLO mode is the correct mode.

- <carbon-user-multiple class="inline" /> &nbsp; You work on a team.

</div>

<!--
These are premises, not promises. Each one is a claim the rest of the deck has
to earn: if the reader nods at all seven, the microVM boundary is worth its cost.

The determinism line is the one to say slowly, because the whole word is in the
distinction it draws. Declarative buys a description instead of a script, and
composable and shareable buy reuse — none of the three promises that the
description lands on the same thing twice. Determinism is that fourth,
independent property: same description, same result, today and next year, here
and on a teammate's machine. Say "not a similar one" out loud; the failure it
names is the one everybody has had, where a rebuild six months later works but
is not the environment that was tested.

Be honest about what delivers it, because the deck earns this in two parts and
neither half is enough alone. The manifest is the description, and a lockfile is
what pins the description to exact versions — and by default that lock is
per-machine, so two people building one manifest on different days may resolve
different inputs. A team that needs byte-agreement commits a shared lock, which
is the third point on the co-workers slide. Determinism here is a guarantee the
tool makes available and a lock the user or team owns, not magic in the tool.

The YOLO line is the joke that is also the thesis, and the only line that drops
the second person — a flat assertion lands the beat better than an opinion
attributed to the audience. Approval prompts are a tax paid for a boundary
nobody trusts: every one of them asks a human to be the security control,
interrupts the work, and gets waved through by the tenth time anyway. Given a
boundary that actually holds, unrestricted is the correct mode, and the prompts
were only ever compensating for the missing wall. Land it as a laugh, then say
the serious half if the room is with you.

The agents line carries two claims on one row, and the second is the one worth
expanding. An agent closes its feedback loop with the same tools a human does —
pre-commit hooks, language servers, formatters, the build and test commands — so
a sandbox that only holds the language runtime makes the agent guess. The
sandbox has to mirror the host development environment rather than approximate
it, which is why the manifest describes the whole toolchain and not just a base
image.
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
---

# So I built one

<div class="journey">

<div class="journey-step">
  <div class="journey-when"><carbon-idea class="inline" /> &nbsp; the want</div>
  <div class="journey-what">One sandbox, described once, the same everywhere.</div>
</div>

<div class="journey-step">
  <div class="journey-when"><carbon-tools class="inline" /> &nbsp; the build</div>
  <div class="journey-what">Built it. Lived in it, daily, on real work.</div>
</div>

<div class="journey-step">
  <div class="journey-when"><carbon-presentation-file class="inline" /> &nbsp; the talk</div>
  <div class="journey-what">Shared it with my team — workflow and tech stack.</div>
</div>

</div>

<div class="journey-note">That talk is the one before this one.</div>

<!--
The personal turn, and a slide to talk over rather than read. Everything up to
here is the industry's problem in other people's words; from here it is mine.

No detail on purpose — the detail is the walkthrough two slides ahead. The one
thing to land out loud is that the want came first and never changed, which is
why the tool underneath it could be replaced twice.

The talk is the dev-container one, same team, same Ana. If the room asks: it was
right about the ergonomics, and that half survived into this project unchanged.
It was wrong about the boundary, which is the joke on the next slide, and I get
to make it because I am the one who made the mistake.
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
      <div class="arch-row"><span class="arch-row-name">the project</span><span class="arch-via">virtio-fs</span><span class="arch-row-note">live, at its own host path</span></div>
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
listening inside. The project row mounts at the same absolute path it occupies
on the host, so linked git worktrees and absolute-path tooling resolve from
either side.

Sources: ADR-0002 (module system as the composition engine), ADR-0004 (TOML
compiles to a generated flake), ADR-0048 (guest module only, vivarium owns the
launch), ADR-0049 (the backend is a closure member), ADR-0038 (host store shared
read-only), ADR-0016 and ADR-0065 (guest agent over vsock), ADR-0100 (the
workspace mirrors its host path).
-->

---
layout: default
---

# One source of truth

<div class="story-grid">

<div>

<div class="kind">
  <div class="kind-name"><carbon-cube /> images/ — the base</div>
  <div class="kind-note">One toolchain per file, as a NixOS module: <code>python.nix</code>, <code>rust.nix</code>. Each imports a shared <code>base.nix</code>.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-layers /> pieces/ — one concern each</div>
  <div class="kind-note">Git identity, ssh agent, the agent CLI. A piece carries everything its concern needs — packages, guest config, mounts, env.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-document /> manifests/ — your combination</div>
  <div class="kind-note">Plain TOML: one image, a list of pieces, resource and network knobs. The file a project binds to.</div>
</div>

</div>

<div>

```text
~/.config/vivarium/
├── config.toml
├── images/
│   ├── base.nix
│   ├── python.nix      # imports ./base.nix
│   └── rust.nix
├── pieces/
│   ├── git.nix
│   ├── ssh-agent.nix
│   └── claude-code.nix
└── manifests/
    └── snackbar-api.toml
```

</div>

</div>

<div class="story-note">vivarium only ever reads this folder. It is plain files — put it in git, share it, fork it.</div>

<!--
The mental model for everything that follows, and the dctl principle carried
over: one folder is the whole config. Say the three kinds in one breath —
images are toolchains, pieces are concerns, manifests are combinations — and
then the two facts that make the folder trustworthy. First, vivarium only ever
reads it: no scaffolding, no generated edits, nothing moved behind the user's
back. What ships with the tool is examples to copy, so a name in a manifest
resolves to exactly one file here and nowhere else — no bundled library can
shadow it. Second, it is plain files, so "version it and share it" is git, not
a feature.

A name is a bare kebab-case identifier and resolution is mechanical: the flat
file first, a directory of the same name second. The directory form exists so
an artifact can keep helper files beside it.

Sources: spec/02 (the config root and the libraries, ADR-0045), spec/03 (the
three artifact kinds, ADR-0003), ADR-0004 (the TOML manifest), ADR-0061
(examples ship, not a second namespace), N13 in spec/08 (config is read-only
to the tool).
-->

---
layout: center
class: text-center
---

<div class="section-kicker">The tool in use</div>

# Ana's week

<div class="section-lead">
A Python API called <code>snackbar-api</code>, an AI agent, and two co-workers.
Five steps, and every step is one file or one command.
</div>

<!--
Scene-set, ten seconds. Ana is the same character the dctl talk followed
through Docker and dev containers; if the room saw that talk this is a
deliberate callback, and if it did not, the slide stands alone. The claim to
plant: everything in the week ahead is one file or one command, and each step
gets a slide.
-->

---
layout: default
---

<div class="story-tag">Step 1 · Monday, nine o'clock</div>

# Describe the sandbox

<div class="story-grid">

<div>

<div class="kind">
  <div class="kind-name"><carbon-copy /> Start from a shipped example</div>
  <div class="kind-note">vivarium never writes her config, so Ana copies an example manifest and edits a few lines.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-list-checked /> Every name is a file</div>
  <div class="kind-note"><code>python</code> is <code>images/python.nix</code>; each piece is one file in <code>pieces/</code>. Nothing hidden resolves behind them.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-firewall /> The network is part of the description</div>
  <div class="kind-note">Four destinations the agent may reach. Everything else is refused inside the VM's own network, before it leaves the host.</div>
</div>

</div>

<div>

<div class="code-label">manifests/snackbar-api.toml</div>

```toml
image  = "python"
pieces = [ "git", "ssh-agent", "claude-code" ]

[resources]
mem_mib = 4096

[env]
PYTHONDONTWRITEBYTECODE = "1"

[egress]
mode  = "allowlist"
allow = [ "pypi.org", "*.pythonhosted.org",
          "github.com", "api.anthropic.com" ]
```

</div>

</div>

<!--
Walk the file, right column, top to bottom — it is short enough to read out.
One image, three pieces, a resource ceiling, the guest environment, and the
network policy, and this file is most of what Ana will ever author. She did not
write it from a blank page: vivarium ships annotated examples generated from the
tool's own config types, and copying one is the intended path, because the tool
never scaffolds her config root for her.

Two details worth naming if the room is technical. The `[env]` table is guest
environment, and this particular variable is a nice accident of the
architecture: the workspace is shared live with the host, so keeping Python
from writing bytecode caches keeps the host tree clean. It is also the table to
warn about — a manifest is compiled into the generated flake and lands in the
world-readable store, so `[env]` is fine for a setting and never for a token,
whatever the launch-channel classification suggests. The `*.pythonhosted.org`
entry shows the wildcard form; a pattern may also be an address or a CIDR block,
and it never carries a port.

The left column's middle point is the pedagogical one: every name is a file
she can open. claude-code is one piece, and adopting it brings the whole
concern — the CLI package, its config mount, its env — with nothing to
re-declare in the manifest.

The egress table deserves a beat: the allowlist is enforced in the VM's own
network namespace, with a default-deny ruleset installed before any packet
path exists, and the only DNS the guest sees releases an answer only after
that name's addresses are allowed. Measured: a denied name refused in 9 ms.
This is the part the container flow keeps in a wiki page, when it has it at
all.

Sources: spec/03 (the manifest grammar, ADR-0057), ADR-0012 (generated
examples), ADR-0021 (a piece carries mounts and env), spec/07 (a compiled
manifest reaches the store, N10), spec/05 (destination pattern forms) and
implementation-status (egress, both modes enforced and measured).
-->

---
layout: default
---

<div class="story-tag">Step 2 · Monday, ten past nine</div>

# Declare it, boot it

<div class="story-grid">

<div>

<div class="cmd-step">
  <div class="cmd-num">1</div>
  <div>
    <div class="cmd-name"><code>[[workspaces]]</code> — declare the project tree</div>
    <div class="cmd-note">The personal <code>snackbar-api</code> manifest owns this directory. Every later command derives what to build.</div>
  </div>
</div>

<div class="cmd-step">
  <div class="cmd-num">2</div>
  <div>
    <div class="cmd-name"><code>viv start</code></div>
    <div class="cmd-note">Compiles the manifest, builds the VM, and boots it. Later starts reuse the build.</div>
  </div>
</div>

<div class="cmd-step">
  <div class="cmd-num">3</div>
  <div>
    <div class="cmd-name"><code>viv shell</code></div>
    <div class="cmd-note">A prompt inside the VM — with the project at the same path it has on the host.</div>
  </div>
</div>

</div>

<div>

```console
$ cd ~/projects/snackbar-api
$ viv start
$ viv shell

$ pwd        # now inside the VM
/home/ana/projects/snackbar-api
```

<div class="code-label">in the personal manifest</div>

```toml
[[workspaces]]
source = "/home/ana/projects/snackbar-api"
```

</div>

</div>

<div class="story-note">Nothing was committed to the repo. The manifest is personal; derived cache, build, and state live under vivarium's own directories.</div>

<!--
The declaration is the concept and not just a setup step: a manifest describes
a sandbox and explicitly owns one or more project trees. Every later command —
start, shell, exec, status — derives the unique owner from the invoking path,
which is why they all take no manifest argument in ordinary use.

The second block is worth reading aloud as a sentence: this manifest owns this
path. The cache that accelerates the reverse lookup is derived and disposable;
the declaration remains the source of truth.

The repo gains no config file, deliberately: a teammate who does not run
vivarium clones a repo with nothing to ignore. vivarium writes nothing into the
workspace.

If asked how one project gets two sandboxes: --manifest overrides selection
for a single run, and VIVARIUM_MANIFEST for a shell, neither persisted.

The pwd is the teaching moment: the project is not mounted "somewhere in the
VM", it is mounted at the same absolute path it has on the host. Same prompt,
same paths, so git worktrees, editor sessions, and absolute-path tooling keep
resolving on both sides of the boundary.

The first build takes minutes and the costs slide owns that honestly; every
later start reuses it.

Sources: spec/01 (selection, start, shell), ADR-0107 (the manifest is the
sandbox key), ADR-0100 (the workspace mirrors its host path), spec/12
(ensure-running and the PTY contract).
-->

---
layout: default
---

<div class="story-tag">Step 3 · The rest of the week</div>

# One command per task

<div class="day">

<div class="day-row">
  <span class="day-cmd"><carbon-play /> <code>viv start</code></span>
  <span class="day-note">Morning. Reuses Monday's build and boots the VM.</span>
</div>

<div class="day-row">
  <span class="day-cmd"><carbon-bot /> <code>viv exec -t -- claude</code></span>
  <span class="day-note">The agent, inside the boundary — no prompts to babysit.</span>
</div>

<div class="day-row">
  <span class="day-cmd"><carbon-terminal /> <code>viv shell</code></span>
  <span class="day-note">A second shell beside it, for the tests and the dev server.</span>
</div>

<div class="day-row">
  <span class="day-cmd"><carbon-flash /> <code>viv exec -- pytest -q</code></span>
  <span class="day-note">Run one-shot commands; the exit code comes back.</span>
</div>

<div class="day-row">
  <span class="day-cmd"><carbon-meter /> <code>viv status</code></span>
  <span class="day-note">State, memory and disk in use, each against its ceiling.</span>
</div>

<div class="day-row">
  <span class="day-cmd"><carbon-power /> <code>viv stop --all</code></span>
  <span class="day-note">Done for the day — every project's VM, volumes kept.</span>
</div>

</div>

<div class="story-note">This is the YOLO premise cashed in: unrestricted is the correct mode once the wall is real.</div>

<!--
Read the commands, not the notes — the shape is the point: one verb per task,
no workspace-folder ceremony, no wrapper scripts. Two rows carry the deck's
earlier promises. The claude row is the YOLO premise from the second slide:
the agent runs unrestricted because the boundary is a separate kernel, and
the network it can reach is the allowlist from Monday's manifest. The shell
row is podbox's wall answered: a second entry into a running VM only works
because an agent is listening inside — this row is why vivarium exists rather
than podman plus krun.

Honesty for a live demo: start, exec, shell, stop, and status all run today;
stop --all is specified but not yet implemented (implementation-status.md),
so sweep projects one at a time if someone asks to see it.

Sources: spec/01 (the verb table), spec/12 (exec and shell, guest exit codes
returned verbatim), ADR-0016 and ADR-0065 (the in-guest agent).
-->

---
layout: default
---

<div class="story-tag">Step 4 · Wednesday</div>

# A second project lands

<div class="story-grid">

<div>

<div class="kind">
  <div class="kind-name"><carbon-document-add /> The cost is one file</div>
  <div class="kind-note"><code>order-engine</code> is a Rust service. Ana copies Monday's manifest and changes one line.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-branch /> The pieces are shared, not copied</div>
  <div class="kind-note">Both manifests name the same three files. Fix <code>git.nix</code> once and both projects carry the fix on their next build.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-virtual-machine /> Two VMs, side by side</div>
  <div class="kind-note">Each with its own kernel — the multi-tasking premise. <code>viv status -g</code> shows the whole fleet against the host's memory.</div>
</div>

</div>

<div>

<div class="code-label">manifests/snackbar-api.toml</div>

```toml
image  = "python"
pieces = [ "git", "ssh-agent", "claude-code" ]
```

<div class="code-label">manifests/order-engine.toml</div>

```toml
image  = "rust"
pieces = [ "git", "ssh-agent", "claude-code" ]
```

<div class="code-label">pieces/git.nix — named by both</div>

```nix
{ pkgs, ... }:
{
  environment.systemPackages = [ pkgs.git ];
  programs.git.config.pull.rebase = true;
}
```

</div>

</div>

<!--
The composability payoff, and the slide to go slow on: the two manifests
differ by one word. The pieces are the same three files, not copies — a fix
in git.nix reaches both projects at their next build, because the generated
flake is compiled fresh from the config root every build rather than from a
per-project snapshot. This is the "edit once, every project picks it up"
property dctl had for containers, now for VMs.

The third block is that shared file, and it is worth pointing at for two
reasons. It is a plain NixOS module — no vivarium schema to learn, so the whole
of nixpkgs' option surface is available inside a piece. And it is genuinely
small: one line installs the package into the guest, one line configures it, and
both manifests reach it by its bare identifier. Note what the install line is
not — there is no download and no registry. `pkgs.git` names a package in the
pinned nixpkgs the whole guest is built from, so the tool arrives through the
same build the kernel does. A real git piece would also carry its host `.gitconfig` as
a mount and any env it needs, through the typed vivarium options, which is what
"a piece carries everything its concern needs" means — adopting it re-declares
nothing in the manifest.

The third point earns the multi-tasking premise: two VMs is two kernels, and
the disk cost of the second is only what the two toolchains do not share —
the store-sharing row on the Why Nix slide, two ahead. status -g, the fleet
view with the host's own headroom beside it, is specified but not yet
implemented; project-local status runs today.

Sources: ADR-0003 (pieces as the reuse unit), ADR-0058 (the generated flake
is regenerated, never a snapshot), ADR-0038 and spec/17 (why the second VM is
cheap), spec/01 (status -g).
-->

---
layout: default
---

<div class="story-tag">Step 5 · Thursday</div>

# The co-workers

<div class="story-grid">

<div>

<div class="kind">
  <div class="kind-name"><carbon-repo-source-code /> The config folder is the team repo</div>
  <div class="kind-note">Images and pieces are the shared surface. Bruno clones them and resolves the same files Ana does.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-user /> The manifest stays personal</div>
  <div class="kind-note">Bruno's manifest can add memory or a piece without touching what the team ships.</div>
</div>

<div class="kind">
  <div class="kind-name"><carbon-locked /> Pinned together, when it matters</div>
  <div class="kind-note">Commit a <code>flake.lock</code> beside the manifest, and every machine resolves the same versions of everything underneath.</div>
</div>

</div>

<div>

```console
bruno $ git clone snackbar/vivarium-config \
    ~/.config/vivarium

bruno $ cd ~/projects/snackbar-api
bruno $ viv start
```

</div>

</div>

<div class="story-note">Onboarding is one clone and two commands. What boots was built from the same files Ana's was.</div>

<!--
The team story, and the split to state precisely: images and pieces are the
shared surface; the manifest is deliberately personal. There is exactly one
manifest per project per user and nothing imports another user's, so Bruno
can raise mem_mib or add a piece without forking anything the team
distributes — and anything that must hold for everyone lives in a piece the
team controls, set at a priority no manifest overrides.

The lock line is the honest version of "same VM": by default, two machines
resolving one manifest on different days may resolve different input
versions, because vivarium refuses to write into the team's repo on its own.
A team that wants byte-agreement commits a flake.lock beside the manifest
(which requires the directory manifest form); it wins over every per-machine
pin, and viv update refuses while it is in force — moving that pin is the
team's own act, in their own repo.

Sources: spec/07 and ADR-0040 (the shared/personal split), spec/02 and
ADR-0062 (the team override lock, and why update refuses under it), ADR-0002
(mkForce floors in shared pieces).
-->

---
layout: default
---

# What vivarium quietly handles

<div class="quiet-grid">

<div class="quiet-item">
  <div class="quiet-head"><carbon-key /> Keys never cross</div>
  <div class="quiet-text">SSH and GPG reach the guest as relayed sockets. Code inside can use a key for the session — it can never read or copy it.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-undo /> Every build is kept</div>
  <div class="quiet-text">A rebuild that goes wrong is one <code>viv generations rollback</code> away from the VM that worked yesterday.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-stethoscope /> It diagnoses itself</div>
  <div class="quiet-text"><code>viv doctor</code> checks virtualization, tooling, permissions, and disk — every failure names what, where, why, and a hint.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-piggy-bank /> Disk is a ceiling, not a cost</div>
  <div class="quiet-text">Volumes are sparse: a 64 GiB limit occupies what it holds. <code>viv trim</code> hands memory back, <code>viv volume trim</code> disk.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-script /> Built to be scripted</div>
  <div class="quiet-text">Every reader takes <code>--json</code>; every failure is one JSON object and a stable exit code. Your agents can drive it too.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-clean /> It leaves no residue</div>
  <div class="quiet-text">Everything lands in standard XDG homes, never in the repo. <code>viv destroy</code> returns a project to a clean first run.</div>
</div>

</div>

<div class="story-note">None of this needed a wrapper script or a wiki page. They are the defaults.</div>

<!--
The recap dctl's deck earned with credential forwarding; here each item is a
subsystem. Keys: forwarding is a relay over a dedicated vsock port, never a
mounted socket — the guest gets a socket it can use for the session while the
key material stays on the host, and for GPG only the restricted extra socket
is ever offered. Say the limit too: a compromised guest can use the key while
the session lasts; what it cannot do is take it.

Scripted: every reader takes --json, a failure is one JSON object on stderr,
and exit codes are BSD sysexits held stable as an API — spec/01 names coding
agents as an intended consumer, so viv is a tool your agent can drive.

Demo honesty, as of this deck's commit: the keys relay, destroy, and the
JSON and exit-code contract are implemented; generations rollback, doctor,
trim, and volume trim are specified but not yet built
(implementation-status.md). Present those as the design; demo the others.

Sources: spec/07 and ADR-0071 (the agent channel and its limits), spec/11
(generations), spec/13 (doctor), spec/17 (trim, ceilings not reservations),
ADR-0015 and ADR-0028 (the output and exit-code contract), spec/02 (XDG
layout), spec/01 (destroy, and agents as consumers).
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
  <div class="con-text">vivarium names the versions its shipped base builds with. A fix reaches you when this project ships an update — or, once you own the base's inputs, when you move your own pin with <code>viv update</code>.</div>
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

Security updates coming from us is what "we lock every version" costs, and the
unqualified form stopped being true when users gained the base's inputs. Locking
is why a teammate gets the same VM, and the same mechanism means a fix in the
hypervisor does not arrive through your package manager. It arrives when this
project publishes a new lock and you update — or, for a fix already reachable
from a reference you own, when you move your own pin. The shipped default still
has a named owner rather than being left implicit.

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

---
layout: default
---

# Where it stands

<div class="quiet-grid">

<div class="quiet-item">
  <div class="quiet-head"><carbon-checkmark-outline /> The daily path runs</div>
  <div class="quiet-text">Bind, build, boot, exec, shell, stop, destroy, volumes, egress — sixteen surfaces implemented, each with a trial that passes.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-document /> Some of it is still design</div>
  <div class="quiet-text"><code>update</code>, <code>generations</code>, <code>doctor</code>, <code>trim</code>, <code>volume rm</code>, <code>unbind</code> — specified in full, written down, not yet built.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-warning-alt /> Barely tested beyond my hosts</div>
  <div class="quiet-text">Every figure in this deck was measured on my own machines. One clean run is evidence of possibility, not of reliability.</div>
</div>

<div class="quiet-item">
  <div class="quiet-head"><carbon-user-multiple /> Where you come in</div>
  <div class="quiet-text">A repro, a patch, or an argument that the design is wrong — all welcome. Run it on a machine that is not mine and tell me what broke.</div>
</div>

</div>

<div class="story-note">The living answer is <code>docs/reference/implementation-status.md</code>: every surface, the level it sits at, and the trial behind it.</div>

<!--
The slide that keeps the deck from being an advertisement. Everything before it
described a design; this says how much of the design exists, out loud, before
anybody discovers it themselves.

Be exact if asked. Implemented means the command runs and its trial passes:
init, config and its readers, manifest list and show, start, status, stop,
destroy, exec, shell, volume list and prune, the workspace mount, declared
volumes, and both egress modes. Designed means specified with nothing written
against it. Between the two sits one honest middle: viv gc's grammar runs and
the sweep behind it does not. Narrower gaps live inside implemented verbs —
stop --all does not sweep, status -g does not enumerate, start runs no
admission check — and each refuses by name rather than half-answering.

The testing point is the one not to soften. The trials pass, and they pass on
hosts I own, which is a claim about possibility rather than about reliability.
Nobody has run this on hardware I have never seen.

On contribution: the most useful thing is not a patch. It is a machine that is
not mine, a project that is not mine, and a report of where it broke — because
the failures I can find alone are already found.
-->

---
layout: default
---

# Closing thoughts

<div class="journey">

<div class="journey-step">
  <div class="journey-when"><carbon-virtual-machine class="inline" /> &nbsp; the point</div>
  <div class="journey-what">One kernel per project. A boundary, not a policy.</div>
</div>

<div class="journey-step">
  <div class="journey-when"><carbon-document class="inline" /> &nbsp; the payoff</div>
  <div class="journey-what">Declared once, composed, shared, repeatable.</div>
</div>

<div class="journey-step">
  <div class="journey-when"><carbon-chat class="inline" /> &nbsp; the ask</div>
  <div class="journey-what">Break it, and tell me where it broke.</div>
</div>

</div>

<!--
Same three-beat shape as `So I built one`, on purpose: the first-person turn
opened that way and this closes the bracket.

The point is the one sentence to leave in the room. Every native sandbox on the
quotes slide is a policy enforced by the kernel it constrains, and a policy is a
thing that can be written wrong; a separate kernel behind a virtualization
boundary is not. That is the whole argument, and Nix is how it gets described
rather than what makes it safe.

The payoff is the second half of the deck in one line, and the honest reading of
`repeatable` is the one from the premises slide — the lock is what makes it true,
and a team that wants byte-agreement commits a shared one.

The ask is the real close. This is a tool I use daily and nobody else has stressed
yet, which is the cost I named two slides ago: millions of people have found
Podman's bugs, nobody has found mine. So the invitation is adversarial on purpose.
Pick one project, run it for a week, and bring me the place it broke.

The slide makes no claim about what is built, so the honesty is the speaker's to
carry, and the specifics if asked are: `init`, `start`, `stop`, `destroy`,
`exec`, `shell`, `status`, the config and manifest readers, volumes, the
workspace mount, and egress are implemented; `update`, `generations`, `doctor`,
`trim`, and `volume rm` are specified and not yet built. The living answer is
docs/reference/implementation-status.md, not this deck.
-->

---
layout: center
class: text-center
---

<div class="section-kicker">Thank you</div>

# vivarium

<div class="section-lead">
One manifest. One microVM per project.
</div>

<div class="closing-link">github.com/gubasso/vivarium</div>

<!--
The bookend of the cover, and the slide to leave up through the questions. Same
poster, same one word, so the talk ends where it started with the room now
knowing what the word buys.

The repository is the only address given: the specification, the decision
records, and this deck all live in it, and the docs are the part to point at
rather than the code — anyone who wants the argument in more depth reads
docs/reference/spec, and anyone who wants the reasoning reads docs/decisions.
-->
