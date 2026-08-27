# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/gubasso/vivarium/compare/vivarium-v0.1.0...vivarium-v0.2.0) - 2026-08-27

### Added

- *(trim)* bring memory and volume space back without a stop
- *(update)* move the pins the user's own base flake now names
- *(start)* check the room before building and stream the boot
- *(stop)* walk the whole ladder and sweep the fleet
- *(status)* measure the fleet and count its sessions
- *(generations)* root every retained build and ship the whole family
- *(credentials)* resolve and carry the declared agent channel
- *(workspaces)* compile a declared tree into an ordinary mount
- *(mounts)* [**breaking**] a file mount serves only its file
- *(mounts)* [**breaking**] declared mounts reach the guest
- *(cli)* human-facing terminal UX and the doctor catalog
- *(launch)* [**breaking**] the binary supplies itself — no vivarium flake input
- *(launch)* move to schema 6 and boot inside the namespace pair
- *(net)* implement the four egress seams for real
- *(net)* select the four egress seams and land their skeletons
- *(volumes)* list, prune, and destroy what a project owns
- *(sandbox)* mirror the workspace path and keep writes across a stop
- *(cli/session)* run a command and a shell inside the project's sandbox
- *(cli/lifecycle)* boot the manifest-built guest and own its lifetime
- *(config/eval)* evaluate the bound manifest and read the merge
- *(config/registry)* bind a project to a manifest and read it back
- *(config)* materialize the generated flake and stage its lock
- *(config)* parse the manifest and render the failure contract
- *(config)* resolve xdg roots, artifact names, and project ids
- *(guest-agent)* implement guest control and credential relay
- *(launch)* implement secure launch and supervision
- *(nix/measurement)* close the host measurement gaps of slice 001
- *(nix/guest)* measure store density and collection pressure
- *(nix/guest)* measure the host garbage-collection interlock
- *(nix/guest)* persist the guest store as a local-overlay store
- *(nix/guest)* close the first microVM build after host verification
- *(nix)* build the first microVM on the cloud hypervisor default
- *(tests)* add workflow guides and gated acceptance harness

### Fixed

- *(verification)* boot the lanes that reported on nothing
- *(launch)* hold the handoff connection across teardown
- *(net)* gate queries concurrently and drop the inert dns rule
- *(harness)* teach three host lanes that launch hands off
- *(credentials)* adopt the backend half-close so the relay pool refills
- make the guest agent host lane executable and repair what it found
- *(nix/runner)* clear virtiofsd pid files on shutdown

### Other

- *(spec/trim)* sit each reclaim verb under its resource
- *(reference)* land the sandbox comparison at public sources
- *(gates)* put every gated run's bytes on the heavy drive
- Merge branch 'docs-site' into initial-implementation
- *(launch)* clear the push-stage clippy gate in the new trials
- *(verification)* lean the check surface and the test suite
- *(net)* assert open mode ships no filter and no resolver
- *(plan)* close slice 004 — the egress allowlist is enforced
- *(nextest)* say that a boot width of 1 narrows the readiness flake
- *(harness)* fix what the configured drive made reachable on a host
- *(host-lanes)* point every disk-heavy lane at one configured drive
- *(workflows)* bound how many trials may boot a guest at once
- *(workflows)* give each fixture a project id no other trial shares
- *(status)* settle the failed reason vocabulary and narrow Q-015
- *(plan)* re-slice the plan around a demonstrable MVP
- *(launch)* assert the console reader survives until VM exit
- align repository with the documentation-design standard
- *(nix)* separate verification from the product tree
- adopt the canonical documentation-design standard
- *(nix)* separate the shipped image from what measures it
- *(decisions)* provision the store volume for inodes
- document the shared-store advantage and its one secret caveat
- *(decisions)* hold the security role solo and make the sandbox disposable
- settle the last four part I design questions
- *(spec)* settle the runtime substrate cluster
- *(spec/runner)* own the runner and pin the backend in the closure
- complete section B user workflows and close design questions
- apply markdown formatting conventions repo-wide
- *(pre-commit)* add shared nix and markdown hook overlays
