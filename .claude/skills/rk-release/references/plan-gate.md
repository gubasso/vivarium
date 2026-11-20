# The plan gate

Standing instructions for the whole task, not one-time steps. Every release-kit skill drives operations that write files, mutate a forge, or publish a version, so each one plans, validates that plan against what actually knows, and only then executes. The agent's prose plan is the review surface: it names every file the run writes and every action it takes, and the operator approves that text.

This gate opens after the task pre-flights of the cell have run, and takes their findings and the pre-flight gate's as inputs: a failed hard probe is why there is no plan yet, and a failed soft probe is a gated step or a stated gap. The pre-flight gate runs whatever the request carries. This gate has one flag.

Hold all three phases for the rest of the task, and apply them to every further request in the same session. Without `--no-plan`, phase 3 runs in a later turn than phase 1, after the plan is approved; with it, the phases run in order in the current turn.

## What a request authorizes

The gate governs how a skill acts; it never widens what the operator asked for. A request authorizes the file writes and the `rk` verbs it names, and nothing else: creating, switching or deleting a branch, creating or removing a worktree, committing, pushing, tagging, and opening, updating or merging a pull request are the operator's moves. Plan each of those as a gated step, state its exact command, and run it only where the operator's request named that action or they answer the gate for it.

A request may carry a term the target's glossary defines. Only a target that opted into the workflow guards carries the workflow terms, in `GLOSSARY.md`. Where it does, read that file and take the term's action list as actions the request named: the term is the operator's own shorthand for the list, so it widens nothing. Restate the list in the plan, action by action, never the term, because an operator reading the plan is where a misreading is caught or nowhere. Where the file is absent, or the word is not a term in it, ask rather than guess: a word that looks like a term is not one. A term authorizes actions and waives nothing else. All three phases still run, the way `--no-plan` waives no pre-flight, and a step outside the term's list stays gated with its exact command.

An approved plan approves its shape, not a standing licence over the repository's git and forge state. A request to implement or change code authorizes the file changes alone: where the work then needs a branch or a commit, say which step comes next, name its command, and stop there. An agent authorized to author a commit, request, or issue writes no agent attribution into it and references no internal planning artifact — the message names the work, never the scaffolding behind it.

## 1. Plan

Do this before the first change of any kind: a file landed in a target, a forge or registry mutation, or any verb run with `--apply`. The agent's own plan file is not such a change — Claude Code's plan mode writes one, and this phase depends on it.

1. Enter plan mode. In Claude Code that is the `EnterPlanMode` tool. In an agent that has no plan mode, state the plan in the reply instead and take the operator's answer before acting.
2. Research read-only. Read the task files the gate picked, the canon they name, and the target's own report: `rk status --target .` observes and never writes. Do not edit, and do not run any verb with `--apply`.
3. Write the plan. It states, in this order:
   - The domain each proposed answer belongs to, with the rule ID that binds it: the project profile and the capability selection by `project-profile:`, the trunk and the checkout mode by `git:`, and what a landing writes by `landing:`.
   - The ordered `rk` verbs to run, each with its flags.
   - The files the run writes, one by one, and the forge or registry state it changes.
   - Every step gated for the operator, with the exact command they run and what it changes.
   - The verification command that closes the task.
   - The open risks and the assumptions the plan rests on.
4. Ask what the plan cannot decide. Use `AskUserQuestion` for a choice that changes the work — the technology binding, the forge, a version line. Do not use it to ask whether the plan is acceptable.
5. Present the plan and end the turn. In Claude Code that is the `ExitPlanMode` tool, whose approval prompt is the gate. Do not pre-approve that tool: approving it automatically is the same as having no gate.

When the request carries `--no-plan`, replace this phase's approval turn: do not call `EnterPlanMode` or `ExitPlanMode`, because plan mode's read-only hold blocks phase 3 and only that approval prompt releases it. Do the same read-only research in the current turn, state the ordered plan in the reply, then continue into phase 2 without ending the turn. Phases 2 and 3 run in full.

## 2. Validate

The plan is a claim about what will happen. Check it against something that knows, never against your own confidence.

1. Preview every verb that has one: `rk init`, `rk setup`, `rk upgrade`, `rk adopt`, and `rk skill uninstall` write nothing without `--apply`. Run each and read what it reports. For a landing, the stage is the surface the plan validates against: `rk stage compare` reads it against the target row by row, and the preview of `rk init`, `rk upgrade`, or `rk adopt` with `--stage` prints one word per destination.
   - The check that fails a run is executable, not this instruction. `rk init --apply`, `rk upgrade --apply`, and `rk adopt --apply` render again from the binary and the target at the moment they run, reading the stage's parameters and never its bytes. Each refuses before its first write on every conflict `--replace` does not name, a missing or newer record, or an unanswered style, and names what it found. Only a named `--replace`, which saves a copy first, passes a conflict.
   - What the gate guarantees: the words a production run prints are the decisions it took, and a refused condition stops the run before the first write. What it does not guarantee: that an agent read the stage correctly, or that a choice the operator made was the right one. Present the plan's file inventory and its questions with their consequences before asking.
2. Validate every action that has no preview — a merge, a tag, a publish, a forge or registry mutation — against what states it instead: `rk guide <topic>` for the commands, the owning method chapter for their order, and a read-only observation of the current state. `rk profile --target .`, `rk assess --target .`, `rk status --check --target .`, and `rk setup check --target .` observe and never write.
3. Compare both against the plan: the destinations, their count, and the steps in their order.
4. Where they disagree, stop. Say what differs, and return to phase 1. Never absorb a surprise by widening the plan silently.

## 3. Execute

1. Run the verbs in the planned order, one at a time. Never batch an `--apply` behind another.
2. Re-observe after each one, and report what the command returned rather than that it succeeded.
3. Gate every step the boundary above leaves to the operator, and every other step they must run by hand: print the exact command, say what it changes and why, wait, then re-observe before continuing.
4. Close on the verification command the plan named. `rk status --check --target .` is the judging mode and exits nonzero while anything is unresolved.
5. Where execution shows the plan was wrong, stop and re-plan. Do not expand the scope of an approved plan.
6. For a landing, a refusal from the production verb wrote nothing, and it returns the task to phase 1 with the named findings as the thing to plan. The stage stays through this phase, so the proof can compare against it, and `rk stage clean <path>` runs last, only where the request's authority includes cleanup.
