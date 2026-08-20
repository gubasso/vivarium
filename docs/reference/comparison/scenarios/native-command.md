# Native command

Record what the user types to run the sandboxed tool, and whether it names the sandbox.[^read]

1. Complete the tool's setup for one application.
2. Record the exact invocation a user types afterwards.

## vivarium

No: running something inside is `viv exec -- <command>` or `viv shell`, so the sandbox is always named — legible but not invisible. flake-pilot's symlink puts the sandboxed `claude` on `PATH`. An open gap: no decision for or against a shim.

## glaipnir

No: `glaipnir run claude` names the sandbox, the same as vivarium's does, and one word shorter is not a different answer. What glaipnir does buy with that word is a roster the invocation can be checked against, which is [the next row](./any-tool.md) and where the two tools part company.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18.
