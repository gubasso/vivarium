---
theme: default
colorSchema: all
title: vivarium — a microVM per project
info: |
  ## vivarium
  A Nix-native tool that boots each project inside its own microVM.
layout: cover
class: text-center
transition: slide-left
---

# vivarium

<div class="tagline">
A microVM per project
</div>

<div class="subtitle">
One manifest. A separate guest kernel behind a hardware-virtualization boundary.
</div>

<div class="footnote">
Nix describes the sandbox &nbsp;·&nbsp; <code>viv</code> resolves, builds, and runs it
</div>

<!--
Landing slide. The whole pitch in one line: your project gets its own kernel,
not a shared one with namespaces drawn around it, and Nix is what describes it.
-->
