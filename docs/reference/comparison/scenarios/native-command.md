# Native command

Record what the user types to run the sandboxed tool, and whether it names the sandbox.

1. Complete the tool's setup for one application.
2. Record the exact invocation a user types afterwards.

## vivarium

vivarium `ceb0027`, 2026-08-18. No: running something inside is `viv exec -- <command>` or `viv shell`, so the sandbox is always named — legible but not invisible. flake-pilot's symlink puts the sandboxed `claude` on `PATH`. An open gap: no decision for or against a shim.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `glaipnir run claude` names the sandbox, the same as vivarium's does, and one word shorter is not a different answer. What glaipnir does buy with that word is a roster the invocation can be checked against, which is [the next row](./any-tool.md) and where the two tools part company.
