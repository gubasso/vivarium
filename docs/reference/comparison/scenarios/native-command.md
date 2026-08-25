# Native command

Record what the user types to run the sandboxed tool, and whether it names the sandbox.[^read]

1. Complete the tool's setup for one application.
2. Record the exact invocation a user types afterwards.

## vivarium

No: running something inside is `viv exec -- <command>` or `viv shell`, so the sandbox is always named — legible but not invisible. flake-pilot's symlink puts the sandboxed `claude` on `PATH`. An open gap: no decision for or against a shim.

## glaipnir

No: `glaipnir run claude` names the sandbox, the same as vivarium's does, and one word shorter is not a different answer. What glaipnir does buy with that word is a roster the invocation can be checked against — the built-in opinion [the credential-scoping row](./per-tool-credentials.md#glaipnir) measures from both ends, and where the two tools part company.

## bunkerbox

Yes, and it is the organizing idea rather than a convenience. A packaged command is a symlink to the bunkerbox binary; bunkerbox reads the name it was invoked as and loads the runtime config of the same name. One binary therefore serves many commands, each with its own image, workspace mode, and network policy, and the user types the tool's own name with nothing in front of it.

```text
/usr/bin/opencode -> /usr/bin/bunkerbox
/usr/share/bunkerbox/opencode.conf
```

The same mechanism as flake-pilot's symlink launcher, reached differently: flake-pilot registers an app and writes the link, bunkerbox has a package install both the link and the config it will resolve.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
