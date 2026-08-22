# ADR-0109: An undeclared working directory is refused

## Context and Problem Statement

Workspaces are declared rather than inferred ([`./ADR-0108-a-workspace-is-owned-by-one-manifest.md`](./ADR-0108-a-workspace-is-owned-by-one-manifest.md)), because no portable variable can name a working directory without making the mount set a function of the call. Declaring them creates two states nothing decides today: a command invoked from a directory the resolved manifest does not declare, and a directory a second manifest tries to claim. Both are silent gaps rather than errors, and a session that quietly starts somewhere else is worse than one that does not start.

## Considered Options

- Refuse, naming the edit that fixes it.
- Start at a default location — the guest home, or the first declared tree.
- Write the missing declaration on the user's behalf.

## Decision Outcome

Chosen option: `Refuse, naming the edit that fixes it` — a default makes one command mean different things in different directories, and writing the declaration is the side-effect class N9 and N13 exist to forbid.

Every command that resolves a manifest verifies the working directory lies inside a directory that manifest declares as a workspace. Failing that, it exits `78`: a defect the manifest text alone decides, on the `65`/`78` boundary [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md) draws. A second manifest claiming an owned workspace takes the same code and the same message shape.

The message is the deliverable, not a footnote to the code. It names the manifest file that was resolved, the directory that is not declared in it, and the exact block to add — following the what / where / why / hint shape the stderr face already carries.

## Consequences

- Good: no project-local file records which manifest owns a tree, because the manifests answer it; N9 needs no exception.
- Good: the check runs before any build or boot, so a misdeclaration costs a message rather than a VM.
- Bad: a user who moves a project must edit the manifest by hand, where the marker used to follow the move.
- Bad: every manifest-resolving command grows a precondition, so the refusal must be one routine rather than a check per verb.

## Status

Implemented

Enacted by [slice 020](../plan/slices/020-many-workspaces-in-one-sandbox/README.md) for the undeclared directory and [slice 021](../plan/slices/021-the-manifest-is-the-sandbox/README.md) for derived ownership and the second-claimant refusal.
