# Agent Guidelines — vivarium

This file is the authored source of truth for how AI coding agents work in the vivarium repository. `AGENTS.md`-native tools read it directly; Claude Code reads the same guidance through the one-line `@AGENTS.md` import in [`CLAUDE.md`](./CLAUDE.md). These instructions override default behavior.

## What vivarium is

vivarium is a Nix-native tool that boots each project inside its own microVM: a separate guest kernel behind a hardware-virtualization boundary. Nix describes the sandbox and composes reusable images and config pieces through one manifest. The command-line tool resolves that manifest, builds the VM with `nix build`, and runs it; upstream Nix and virtualization components provide the guest kernel, isolation boundary, and configuration merge.

The normative product rules live in [`docs/reference/spec/08-invariants-and-guarantees.md`](docs/reference/spec/08-invariants-and-guarantees.md). Read it before changing runtime, build, or composition behavior.

<!-- self-containment -->

## Self-Containment and External References

The docs describe only vivarium and MUST make complete sense to a reader who has this repository and a public internet connection. The boundary is public versus local.

Do not cite anything scoped to one person or machine: personal absolute paths, contributor-only working trees, private notes, private URLs, or facts justified only by another local repository. Public upstream projects, specifications, standards, CVEs, and articles are welcome when properly named and linked. Carry the substance needed to understand, build, or operate vivarium in this repository so a dead link costs convenience rather than a fact.

## Documentation Maintenance

- Use five reader-need zones under `docs/`: decisions in `docs/decisions/`, task guides and runbooks in `docs/guides/`, exact lookup and diagnostics in `docs/reference/`, current subsystem design and background in `docs/explanation/`, and binding scope, milestones, questions, and slices in `docs/plan/`.
- Keep each subsystem's living design in `docs/explanation/<subsystem>.md`. ADRs preserve the historical why and stay frozen. A finding attached only to a decision dies when that decision is superseded, so findings belong with the topic they describe.
- Name every filled decision `ADR-NNNN-<decision>.md`; `docs/decisions/template.md` is exempt from the prefix.
- Keep filled ADR bodies at or below 350 words with exactly one `## Status` from `Ideation | Proposed | Accepted | Implemented | Deprecated | Superseded | Rejected`. Never delete a record from `Proposed` onward. Supersede or deprecate it, or retain its status and add an `Amended by ADR-NNNN — <change>` line for a partial change. Address-only factual corrections are permitted; claims, numbers, dates, statuses, and the target document of a link remain frozen.
- Give every slice a fixed numeric appetite and a separate non-negotiable core. [`docs/plan/charter.md`](docs/plan/charter.md) owns the appetite unit and what one of them means. Cut ordered remainder before moving the appetite; record any post-start change to Goal, Core, Appetite, or Acceptance under `Revisions`.
- Give every slice one directory under `docs/plan/slices/`, entered through `README.md`. `tasks.md` MUST NOT exist unless the slice is active and work crosses a context reset; delete it when the milestone becomes `done`. `requirements.md` MUST NOT exist without many-to-many acceptance traceability. `design.md` MUST NOT exist.
- Until the current slice is implemented, do not add a specification page or a product/specification ADR outside that slice. Repository-structure decisions may use `docs/decisions/`; a product question that arises goes to `docs/plan/open-questions.md`.
- Work from the current slice and the individual files its `Governed by` section names. Status lives only in `docs/plan/milestones.md`, and open questions MUST name what they block and one exit.
- Keep fixed heading shapes external: each shape owns one `MD043` headings array under `.markdownlint/`, and exactly one filtered hook applies it. The project config MUST NOT mention `MD043`, and documents MUST NOT carry copied lint configuration. The fixed shapes are slice entry documents, `docs/plan/milestones.md`, and decision records plus `docs/decisions/template.md`.
- Keep load-bearing comments beside the boundary they explain: comments retain local rationale, invariants, boundary conditions, and links to owning ADRs; names, types, schemas, and tests own executable behavior contracts.
- Write each durable fact once at its owning home and cross-link with a short reason phrase. Let the filesystem own structure; indexes explain purpose and do not paste directory trees or duplicate inventories.
- Keep scratch work in the gitignored `.draft/` workspace. Promotion is a rewrite into the owning zone, never a move of draft narration into shipped docs.
- Track external-system bugs under `docs/reference/known-issues/`; expand a case while hot and collapse it to a searchable summary when resolved. Track perishable facts in `docs/reference/tracking.yaml` with a cadence and `last_checked` date.
- Use no bold or italics. Put identifiers, paths, flags, and status values in inline code. Use uppercase RFC 8174 keywords for binding requirements only in normative documents. Every fenced block declares a language.
- Update docs only when a change affects durable behavior, operations, or decisions. Report which owner changed, which links were added, and which checks passed.

This project carries the substance of its documentation conventions locally rather than linking contributor-local material, as required by the self-containment rule above.

The specification outranks tests, but silence does not. A shape a test deliberately exercises beats spec silence; an explicit spec line beats an unargued assertion in a test helper. Promote a test-owned contract into its spec in the same change, or fix the test, so the pair never contradicts.

## Working Conventions

- Keep changes scoped and reversible; prefer editing existing files over adding new ones.
- Run the project's lint and test tasks before proposing changes.
- The root `flake.nix` is the development environment, not part of the product. It declares no build outputs or product inputs. What vivarium builds is owned by [`nix/flake.nix`](nix/flake.nix), with its own lock. Adding a hypervisor, guest kernel, or `microvm.nix` to the root flake crosses this boundary; `scripts/check-flake-boundary` enforces it against evaluated attribute names.
- `nix/` is product, `tests/` verifies product, and `scripts/` operates on the repository. Dependencies point from verification to product. The sole reverse edge is `nix/flake.nix` publishing `tests/nix` checks; `scripts/check-verification-boundary` guards it.
- Extensionless `scripts/foo` and `tests/host/foo` files are executable programs. A co-located `foo.sh` is a body Nix loads. Extract embedded shell bodies at roughly five lines; use `runtimeInputs` or unit `path` for binaries, environment for scalars, and `replaceVars` only for required literals.
- A file's job must be defended. When adding a line, ask whether the file still has one job. If the answer needs a paragraph, the line belongs elsewhere.
- Inert is not absent, and "not applicable" is not a value. An experiment that needs a path clear removes a knob rather than neutralizing it; before reading a number as a quantity, verify that the measured system defines that quantity. The harness method note owns the related lesson that assumption-shaped checks can pass vacuously or fail for the wrong reason.
- One clean run is evidence of possibility, not reliability. Repeat observations when the claim depends on stability, variance, or absence of intermittent failure.
- Development pins; the shipped product resolves live. A released vivarium names branches so a user gets today's upstream, pinned from then on by the lock the first evaluation writes. Development does the opposite: every lane, trial, and hand-run check pins its inputs to what this repository's own [`nix/flake.lock`](nix/flake.lock) already locks, reached as local store paths through `nix flake archive`. The difference is not taste. Branch resolution queries the upstream forge on every fresh evaluation, and a suite that evaluates from a clean root trial after trial exhausts an anonymous rate limit and then fails, or worse skips, for a reason that has nothing to do with the change under test. A lane that must reach the network to prove a local fact is a lane that reports on the network. Pin by default, resolve live only where resolving live is the behavior being tested, and keep the two apart with a named seam rather than a flag someone remembers to set.
- Capacity is a preflight, not a discovery. A run that builds an image, realises a closure, or fills a store measures in gigabytes, and a disk that runs short does not refuse cleanly — it fails mid-write, and the resulting half-fetched input reads as a product defect in whatever change happens to be under test. So a disk-heavy lane asks before it writes anything: [`tests/host/disk-preflight`](tests/host/disk-preflight) declares the requirement, prefers a drive named by `VIVARIUM_HEAVY_DRIVE`, prompts a terminal for a path when the current disk is short, and refuses a non-interactive run rather than guessing. Deferring is a first-class answer, because the point of asking early is that the operator can still stop, attach a drive, and start over having lost nothing. Every new lane of this kind calls it first, and names the space it needs rather than hoping.
- The agent's environment is not the target host. Do not promote missing `systemd`, `/dev/kvm`, a session bus, or runtime directories in an agent environment into a target-host fact. Mark load-bearing host premises unverified until confirmed on a real host.
- Durable findings belong in this repository, not in agent-private memory. Put decisions, constraints, verified premises, and conventions in the zone that owns them.
