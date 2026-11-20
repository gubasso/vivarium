# Task: land

A first landing of the convention into a target with no record, no release mechanism, and no release history. It is `rk guide setup` §4, and `rk method setup` carries the why.

## Pre-flight

1. Confirm a clean trunk: `git status --porcelain` prints nothing, and `HEAD` equals the remote trunk.
2. Run `rk profile --target . --json`. It reports every value with the source that answered it, and which capabilities the catalog selects here.
3. Ask only what the report leaves open and release-kit needs to render its files, one default each, with its consequence stated:
   - the technologies and the forge, where the observation named none;
   - the release mode, and for an automatic release its driver and its style. Under either style, the release request waits for the operator's order;
   - the trunk name;
   - the integration mode: `local` for one writer, and `forge` for several writers with a review requirement, because no local gate substitutes for a second person reading a diff. The release request integrates at the forge either way;
   - each capability the catalog can select here, with its cost: the Scorecard workflow posts a result to a public API, the code scanning provider is read from `rk binding <tech>`, the Nix packaging seeds a package expression and a flake pair, and the reporting policy lands `SECURITY.md`, on by default for an automatic release;
   - the workflow guards, off by default. List what they land before asking: the agent and commit policy lines, the glossary, the workflow hooks, and the `merge-cleanup` and `branch-reminder` setup steps.
4. Say pull request or merge request as the forge names it. Never ask for the checkout mode: it is a default the helper verbs read.
5. Continue on zero technologies, an absent forge, or a category this release does not know: each is a valid profile, and the report says what it selects.

## Execution

Fill the pipeline:

- Stage: `rk stage --target . --json`, with every answer as its flag.
- Compare: a workflow the target already runs for a capability is a question. Bring it to the candidate, or leave the capability out. Never land beside it. A capability the catalog reports unavailable here is not offered.
- Land: `rk init --stage <root> --target .`, then the same with `--apply` as a gated step.

## Post-flight

1. The Prove step passed.
2. `SECURITY.md` names this target and its private reporting channel. Where it does not, stage again with the right repository.
3. Answer each sentinel: `rk status --target . --json` lists each one in `sentinel_locations`, by file and line. Write the project's value into the seeded file as a gated edit.
4. Take `rk guide setup` 4b, the artifact plan.
5. Write the proof the landing closes on: `rk setup check --target .`, then `rk setup checkpoint --target .` on a host that holds the bot key. A step this forge cannot run goes in `setup.excluded_steps` with its reason. Until then `rk status --check` reports `setup-proof-absent`.
6. `rk status --target . --json` reports `classification` `current`, with `setup_proof` `compatible`.
7. State in the report that the landed files and `.release-kit/setup-proof.json` reach the trunk together through the project's own workflow, which is `rk guide setup` 4c, and take 4d, the hooks, last.
