# Task: provision

Bring `rk` to this host at the version the repository records, and record in the repository how every contributor obtains it. The gate picks it where the pre-flight finds no `rk`, a record newer than the binary, or a first landing or a migration with no pin and no recorded host decision, or with a pin and no check line. `rk guide setup` carries the pin order in its Prerequisites.

## Pre-flight

1. Run `command -v nix mise asdf devbox direnv` to learn which managers and loaders the host carries.
2. Where no `rk` runs on this host, no verb below runs yet. Ask the operator with `AskUserQuestion` how this host gets a first `rk`, and run the answer as a gated step: the install of the manager the repository already pins, or a host install with `cargo binstall release-kit`. Then run this pre-flight again.
3. Run `rk self-depend status --target . --json`. Hold `state`, `wired`, `envrc`, `envrc_check`, and `envrc_old_sync`. The report is the only source: keep no manager list of your own.

## Step A: how the project obtains rk

Where `wired` names a manager or `state` reads `host`, and the request does not change that answer, an answer already closed this question: ask nothing, and go to the last step.

1. Ask the operator for the method with `AskUserQuestion`. The default is the manager the repository already carries. State the cost of each option before the operator picks:
   - a manager the status names: its file and lock record the version, so every contributor runs the same `rk`;
   - a native host install: no file records the version, so each host installs `rk` itself, and the configuration records only the minimum it accepts.
2. For a manager, preview `rk self-depend add --manager <manager> --target . --json` for each candidate the status listed, and offer only the ones whose `support` is not `manual`, because a manual pair names a reason and writes nothing.
3. On a manager, where the preview names a seed, gate the same command with `--apply`. Where it seeds nothing, the manager file exists, and placing the fragment it printed is a gated edit you make.
4. For a native host install, gate `rk self-depend add --host --target . --apply`. It records the answer in `.release-kit/config.toml`, writing that file where the target has none yet, and the landing that follows keeps the answer.
5. Where the operator declines the offer before a first landing or a migration, record the decline as a host decision, the same way.
6. Make the binary run in this session, as a gated step on the operator's host: `nix develop`, `mise install`, `asdf install`, `devbox shell`, or `cargo binstall release-kit` for a host install.

## Step B: how the dev environment loads

1. Ask whether the environment loads through direnv, another loader, or none.
2. With direnv, the loader line the manager needs goes into `.envrc` as a gated edit.
3. Where `wired` names a manager and `envrc_check` is false, ask with `AskUserQuestion` whether the project lands the freshness line. State what it does: on directory entry it checks at most once a week for a newer release and warns, it writes nothing but its cache, and it never moves the pin; moving it stays a gated step the operator approves. Where `envrc_old_sync` is true, the line is the retired sync line: run `rk self-depend clean --target . --apply`, which swaps it for the check line.
4. On yes, run `rk self-depend add --manager <wired> --target . --json`. It writes only a file the target lacks, so an existing `.envrc` takes the served line as a gated edit you make. The flake pair's apply seeds an `.envrc` carrying the line where the target has none, so ask this question before that apply rather than after it.
5. On no, record the answer in the report, so the question reads as asked and answered.
6. Moving the pin later is `rk self-depend sync --apply`, the one step that reaches the network, run only as a gated step the operator approves. After the pin move, load the devshell and run the upgrade through the rk-setup skill of the rk in that devshell, and do every migration it names. Before `rk upgrade`, read it with `rk skill show rk-setup`. After it, the project's .claude/skills/rk-setup is that skill. Never use a host copy or an older skill. Then route to the `upgrade` task.
7. Where `next` of `rk self-depend status --target . --json` names follows lines that the `release-kit` input lacks, add each beside its `url` as a gated edit, then run `nix flake lock`. Each line points a development input of rk at this flake or at the flake's own input of that name, so the lock holds no nested copy.

## Post-flight

1. In the dev environment, `rk --version` names the version the repository records.
2. `rk self-depend status --target . --json` names the wired manager or the host decision, and `envrc_check` matches the answer to the freshness line.
3. Where the `skill-shadow` probe of `rk doctor` fails, gate its `next` line for the operator.
4. Run the general pre-flight again, now with the binary.
