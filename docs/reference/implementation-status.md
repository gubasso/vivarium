# Implementation status

The single source of truth for what nixvault **does today** versus what is **designed only**. The
specification under [`spec/`](spec/README.md) describes the intended design; this page records how
much of it exists in code.

## Current state: design stage

**Nothing is implemented yet.** The repository contains the founding documentation and an empty
binary skeleton. No command in [`spec/01-command-surface.md`](spec/01-command-surface.md) is
functional. Do not treat any spec page as a description of working behavior; treat it as the target.

## Surface status

Every command below is **designed, not implemented**.

| Command | Status |
| ------- | ------ |
| `nixvault init` | Designed |
| `nixvault images list` | Designed |
| `nixvault manifest list` / `manifest show` | Designed |
| `nixvault up` | Designed |
| `nixvault exec` | Designed |
| `nixvault shell` | Designed |
| `nixvault down` | Designed |
| `nixvault show --resolved` | Designed |
| `nixvault doctor` | Designed |
| `nixvault config` | Designed |

## How to update this page

When a command or capability becomes real, change its row from **Designed** to **Implemented** and
link to the code or test that enacts it. Keep this page honest: a reader deciding whether to rely on
a feature consults it first, and a stale "Implemented" here is worse than none. This page owns status;
the spec owns the contract.
