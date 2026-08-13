# Edit a project from both sides of the sandbox

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Your project is mounted read-write inside the sandbox at the same absolute path it has on the host. One path string names it on both sides, so an editor on the host and a build inside the guest work on the same bytes and refer to them the same way. The mount and its confinement are specified in [workspace and project environment](../reference/spec/06-workspace-and-project-environment.md), and [the decision to mirror the host path](../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) explains what that buys and what it costs.

## Confirm where the project is

```console
$ cd ~/Projects/demo
$ viv exec -- pwd
/home/you/Projects/demo
```

The guest reports the host's own path, not an invented one such as `/workspaces/demo`. Nothing about your host layout is built into the sandbox to make that true: the path is supplied when the VM starts and applied as the guest boots, which is why the same manifest still builds an identical sandbox on someone else's machine.

## Edit from either side

```console
$ printf 'from the host\n' > notes.txt
$ viv exec -- cat notes.txt
from the host

$ viv exec -- sh -lc 'printf "from the guest\n" >> notes.txt'
$ cat notes.txt
from the host
from the guest
```

A file the guest creates belongs to you on the host, so you can edit, commit, and delete it without a permission fight. The sandbox's user has a fixed identity that the share translates to yours in both directions ([the identity-translation decision](../decisions/ADR-0066-share-uid-gid-translation.md)).

## Why the path matters for worktrees

Git records absolute paths when you add a linked worktree — one pointer under the main repository and one back-pointer in the worktree itself — and it writes the back-pointer from whatever repository your working directory resolves to. A worktree created where the project answers to a sandbox-only path is therefore valid from one side and broken from the other, and it fails quietly: `git worktree list` reports it healthy from both sides while `git worktree remove` fails from one. Matching paths remove that failure rather than detecting it.

One piece is still missing, and it is worth knowing before you rely on this. When the project you bind is itself a linked worktree, its git directory lives under the main repository, which is outside the single directory vivarium shares — so git inside the guest cannot resolve it yet. Declaring that directory as an extra mount is the intended route; see [`../plan/open-questions.md`](../plan/open-questions.md).

## What not to keep on the mount

Keep regenerable caches off it. Dependency trees, package-manager caches, and build output belong on a persistent volume, where name lookups and memory-mapping are local operations rather than round trips to the host. The default volume already puts everything under the guest's home on that side of the line, and `[volume].persist` covers a toolchain that insists on writing inside the repository.

## Acceptance coverage

`workflow_09_workspace_round_trip` in [`user_workflows.rs`](../../tests/user_workflows.rs) covers this: it starts a sandbox, checks that the session's working directory is the project's own host path, carries an edit across the boundary in each direction, and reads back from the host that a file the guest created is owned by the invoking user. [`../../tests/host/first-microvm-check`](../../tests/host/first-microvm-check) covers the guest half independently, comparing what the mirror unit reported against the guest's own mount table so the two can be seen to disagree.
