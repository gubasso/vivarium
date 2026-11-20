# Task: migrate

Carry a project that releases through another tool onto the convention as a greenfield: the predecessor's files move into an archive, a clean instance lands, and what the project still needs is carried from the archive one group at a time. Every destructive step saves what it removes first. `rk guide migration` carries the commands, and `rk method migration` carries the why.

## Pre-flight

1. Run `rk migrate status --target . --json`. Where a migration is open, resume at the first step its registry has not closed, and never from memory.
2. Confirm that every file the inventory lists is committed: `git status --porcelain` names none of them.
3. Warn once: the repository-side work runs on a branch the operator owns, so the trunk keeps releasing through the predecessor until that branch merges. The branch, its worktree, and when it merges are the operator's.

## Execution

1. Inventory. Run `rk migrate inventory --target . --json`. It lists every predecessor file with its tool and its kind. `CHANGELOG.md`, `README.md`, `LICENSE`, and `AGENTS.md` stay in place.
2. Archive. Preview `rk migrate archive --target .`, then run it with `--apply` as a gated step. It moves every predecessor file into the migration workspace, saves the forge state and a bundle of every long-lived branch, writes the registry, and records the open migration in the configuration. The repository is then a greenfield.
3. Land. Run the `land` task's pre-flight questions, then the pipeline with `rk init --stage <root> --target .`. No conflict is left, so no `--replace` is named.
4. Map. Read the archived sources one predecessor tool at a time, so the whole mechanism is in view. Propose each item's fate: `replaced` where a landed file now does its job, `carried` where the project needs a step the convention lacks, as a project-owned file, or `dropped` with its reason. Record the approved map with `rk migrate mark`: `<item> mapped --group <group>`, `<group> mapped --destination <path>`, and `<item> dropped --reason "<why>"`.
5. Loop. One group at a time: write what is carried, verify it against its sources, and run `rk migrate mark <group> verified`. Commit through the project's own workflow. The branch then merges, within the request's authority.
6. Forge. Once the branch merged, run the `forge-settings` task on the live forge, then `publish` where the setup proof is not current. Each destructive step refuses while the registry holds no snapshot for it. Where the runtime cannot read the forge, the snapshot reads `unknown`, and those steps wait for a host that holds the credentials.
7. Close. Preview `rk migrate close --target .`, then run it with `--apply` as a gated step. It refuses while an item is unfinished or the setup proof is not compatible, then clears the configuration key. Ask whether to keep or remove the workspace, and remove it only as a gated step.

## Post-flight

1. `rk migrate status --target . --json` reads every item `verified` or `dropped`.
2. `rk status --target . --json` reports the classification the target earns now, which is no longer `migration`.
3. The report names the workspace path and every snapshot in it.
4. Next: cut the first release with the `rk-release` skill, which is the proof that the new path works.
