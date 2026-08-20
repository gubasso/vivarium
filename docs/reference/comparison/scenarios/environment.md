# Environment

Record who decides which of the host's environment variables are visible inside: the person starting the sandbox, or the tool.

1. Export a distinctive variable on the host.
2. Start the tool without naming that variable, and record whether it appears inside.
3. Name it by the tool's documented mechanism, and record whether that mechanism is open to any variable or fixed to a list the tool ships.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the host environment is deny-by-default against a fixed allowlist, and `--env KEY` copies a named variable from the host only when it exists while `--env KEY=VAL` supplies a literal. The caller names what crosses, per invocation, and the manifest names what crosses durably.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: at the firecracker boundary there is no environment passthrough at all, so nothing crosses until the registration says so. What it lacks is a policy of its own — at the container backend the set is whatever the registration froze into `--opt` lines, and a `%VAR` placeholder with no matching variable becomes the literal name rather than failing.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: an explicit list crosses rather than a wholesale copy — `TERM` and `COLORTERM`, `GOOGLE_CLOUD_PROJECT` and `VERTEX_LOCATION` when set, plus five computed `AI_*` values — but the list is the tool's rather than the caller's. The four host-sourced names are forwarded whenever they exist, and there is no argument or key that adds a fifth. A closed set is a real guarantee; it is not this row's question.
