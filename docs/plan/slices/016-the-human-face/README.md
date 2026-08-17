# 016 — The human face

## Goal

Every command speaks to a human at a terminal — color, live progress, `--help`, `--version`, and the global verbosity flags — enacting the output contract ADR-0015 fixed and nothing has implemented.

## Appetite

3 implementation sessions.

## Core

A `src/ui/` layer resolves the whole suppression rule in one place, every long operation shows progress on a TTY, `--help`/`--version`/`-v`/`-q` exist, and every byte-exact assertion and every acceptance workflow passes unchanged off a TTY.

## In scope

Ordered; when the appetite binds, cut from the bottom.

1. Build the pure foundations: `src/ui/chain.rs` carrying the first-party `NO_COLOR > FORCE_COLOR > isatty` chain from [`spec/01`](../../../reference/spec/01-command-surface.md), `src/ui/style.rs` carrying a `Palette` whose colored form always forces styling so no crate's own detection overrides the chain, and [`src/cli/grammar.rs`](../../../../src/cli/grammar.rs) `Streams` gaining `stdout_is_tty` and `stderr_is_tty` beside `stdin_is_tty`.
2. Parameterize the diagnostic skeleton by the palette: one renderer, `Display` delegating with the plain palette, so the byte-exact tests in [`src/diagnostic.rs`](../../../../src/diagnostic.rs) pass untouched and a new test asserts the colored render strips back to the plain one — the identity [`spec/14`](../../../reference/spec/14-exit-codes.md) promises. The `--json` failure arm never receives a palette.
3. Add the global grammar: `Verbosity`, a verb-keyed pre-pass lifting `-v`/`-vv`/`-vvv`/`--verbose`/`-q`/`--quiet` before dispatch so placement is free per [`ADR-0026`](../../../decisions/ADR-0026-global-flags-and-config-precedence.md), stopping at the first `--` and never consuming a value flag's argument; plus `--help` and `--version`, which do not exist at all today.
4. Thread a `Ui` handle as a `Context` field resolved purely from streams, environment, verbosity, and output — silent face first, with the full acceptance suite required byte-identical before anything animates. The bare `eprintln!` mid-command in [`src/cli/mod.rs`](../../../../src/cli/mod.rs) becomes the handle's warn channel.
5. Replace the four buffered `.output()` child runs — build, evaluation, runner render, handoff — with a streaming watcher that drains both pipes concurrently, forwards stderr lines under `-v`, and captures the complete text so every diagnostic `why` slot stays byte-identical.
6. Turn on the terminal face: the `cliclack` theme, steps for build and boot, a ticking elapsed message in the stop poll loop — the only evidence under `--timeout -1` that the process is alive — and the two guest-agent waits.
7. Style stdout: a first-party fixed-column table in `src/ui/table.rs` measured ANSI-aware, the palette through [`src/cli/render.rs`](../../../../src/cli/render.rs), and the confirm prompt restyled with its answer table unchanged.

## Out of scope

- `viv doctor` and the probe catalog; slice 017 owns it.
- The always-on diagnostic log file and `--log-*` flags; slice 005 owns them.
- New lifecycle verbs, `--attach`, `--global`, `--all`, `gc`.
- Any change to a `--json` record's shape or to the exit-code taxonomy.
- Coloring or reshaping machine output; JSON stays byte-plain.

## Governed by

- [`../../../decisions/ADR-0103-presentation-layer-and-terminal-face.md`](../../../decisions/ADR-0103-presentation-layer-and-terminal-face.md) — fixes the dependency split and what stays first-party; this slice enacts it.
- [`../../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../../decisions/ADR-0015-cli-output-and-failure-contract.md) — fixes the stream split, TTY-only progress, and the color chain.
- [`../../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../../decisions/ADR-0026-global-flags-and-config-precedence.md) — fixes `-v`/`-q` as global and `--json` as per-command.
- [`../../../decisions/ADR-0068-human-error-presentation-and-diagnostic-ids.md`](../../../decisions/ADR-0068-human-error-presentation-and-diagnostic-ids.md) — fixes the skeleton this slice colors without changing.
- [`../../../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md`](../../../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md) — freezes the surfaces that must survive byte-identical.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns the streams, global flags, and color chain.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns the skeleton and the color-identity sentence.
- [`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md) — owns the three faces and what `-v`/`-q` tune.

## Acceptance

When any command runs with stderr not a terminal, its `--json` records, its notes, its diagnostics, and every test-pinned surface SHALL be byte-identical to the pre-slice binary; the unpinned human tables MAY change shape, because reshaping them is this slice's deliverable.

When a command runs on a terminal, progress SHALL appear on stderr only, SHALL erase itself before the result prints, and SHALL be absent under `-q` and under `--json`.

When color is resolved, the order SHALL be `NO_COLOR` over `FORCE_COLOR` over the stream's own TTY fact, per stream, and stripping the colored diagnostic render SHALL yield exactly the plain render.

When `viv --help`, `viv <verb> --help`, or `viv --version` is invoked, it SHALL print usage or the version on stdout and exit `0`.

When `-v` is given, the child's stderr SHALL stream live during build and evaluation, and the diagnostic `why` slot on failure SHALL carry the same complete text as before this slice.

While `viv stop` waits on a unit, the terminal face SHALL show elapsed time, including under `--timeout -1`.

## Rabbit holes

- Restyling drifts into reshaping `--json` or the skeleton — escape: ADR-0075 freezes them; only escape sequences may differ.
- The pre-pass grows into an argument-parser rewrite — escape: lift only the global tokens, verb-keyed for value flags, and leave every verb parser as it stands.
- Progress text becomes a tested contract — escape: spec/01 makes progress TTY-only and deliberately loose; assert channels and suppression, never spinner text.
- The watcher grows caps or ring buffers — escape: the `why` slot is published; capture everything, always.
- Theming expands into cliclack's prompts replacing `src/cli/prompt.rs` wholesale — escape: restyle output; the answer table and the closed-stdin rule stay ours.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, [`ADR-0103`](../../../decisions/ADR-0103-presentation-layer-and-terminal-face.md) reaches `Implemented` with its enactment linked, and the [`milestones.md`](../../milestones.md) row flips to `done` with slice 017 next.

## Revisions

Recorded 2026-08-17, at the slice's start: the first acceptance assertion originally required all off-terminal stdout byte-identical, which contradicts item 7 — restyling the unpinned human tables is the slice's point, and they change off-terminal too (layout is not color). The assertion now scopes byte-identity to the published surfaces: `--json` records, notes, diagnostics, and every test-pinned string. The step-4 gate was still run against the strict form and passed, because at that point nothing had been restyled.

Recorded 2026-08-17, at close. The seven items landed in one merged pass under the appetite, nothing cut. The acceptance evidence, in order: the step-4 gate ran the full acceptance lane with the silent face wired and nothing animating — 21 trials green, byte-identical — and the same lane ran green again after the terminal face and the restyle (21 trials, 377 s); the color chain holds by unit test over closure environments and by hand (`NO_COLOR=1` on a terminal plain, `FORCE_COLOR=1` piped colored, `--json` piped clean); the diagnostic identity is `color_changes_no_text`, asserting the colored render strips to the plain one over every slot; `viv --help`, `viv start --help`, and `viv --version` answer on stdout at `0`; the streaming watcher's capture equals a buffered run byte for byte including an unterminated last line, and both pipes filling 1 MiB complete without deadlock (`ui::watch` tests); a real boot on this host wore the whole gutter — intro, evaluated, built, booted, `running · 8192 MiB · 8 vcpu` — and a real stop ticked its elapsed wait and closed with the stopped line. One deliberate seam: cliclack's spinner exposes no suspend, so the animation runs on `indicatif` directly, themed to match, exactly the fallback ADR-0103 anticipated.
