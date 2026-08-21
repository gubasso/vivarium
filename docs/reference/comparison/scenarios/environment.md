# Environment

Record who decides which of the host's environment variables are visible inside: the person starting the sandbox, or the tool.[^read]

1. Export a distinctive variable on the host.
2. Start the tool without naming that variable, and record whether it appears inside.
3. Name it by the tool's documented mechanism, and record whether that mechanism is open to any variable or fixed to a list the tool ships.

## vivarium

Yes: the host environment is deny-by-default against a fixed allowlist, and `--env KEY` copies a named variable from the host only when it exists while `--env KEY=VAL` supplies a literal. The caller names what crosses, per invocation, and the manifest names what crosses durably.

## flake-pilot

Yes at both routes, and by absence of a policy rather than by one.

At the firecracker route there is no environment passthrough at all, so nothing crosses until the registration names it. At the `krun` route the set is whatever the registration froze into `--opt` lines — the upstream one carries a single `--opt "\-e HOME=%HOME"` — so the same holds, decided once rather than at each start.

What both lack is a policy of their own. A `%VAR` placeholder with no matching variable in the environment becomes the literal name rather than failing, so a registration that expects a variable a colleague does not have carries the string `%VAR` into the guest and reports nothing, which is one of the shapes [the portability row](./portability-enforced.md#flake-pilot) charges for.

## glaipnir

No: an explicit list crosses rather than a wholesale copy — `TERM` and `COLORTERM`, `GOOGLE_CLOUD_PROJECT` and `VERTEX_LOCATION` when set, plus five computed `AI_*` values — but the list is the tool's rather than the caller's. The four host-sourced names are forwarded whenever they exist, and there is no argument or key that adds a fifth. A closed set is a real guarantee; it is not this row's question.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18.
