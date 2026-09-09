# Edit a project from both sides of the sandbox

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Every tree the manifest declares with `[[workspaces]]` is mounted read-write inside the same sandbox at the absolute path it has on the host. One path string names each tree on both sides, so an editor on the host and a build inside the guest work on the same bytes and refer to them the same way. The mount set and its confinement are specified in [workspace and project environment](../reference/spec/06-workspace-and-project-environment.md), and [the decision to mirror the host path](../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md) explains what that buys and what it costs.

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

When a linked worktree and its main repository both belong to the sandbox, declare both as workspaces. Their absolute pointers then resolve on both sides without assigning either tree a guest-only target. Workspace paths must be disjoint: declaring one inside another is refused before boot and names both paths.

## What not to keep on the mount

Keep regenerable caches off it. Dependency trees, package-manager caches, and build output belong on a persistent volume, where name lookups and memory-mapping are local operations rather than round trips to the host. The default volume already puts everything under the guest's home on that side of the line, and `[volume].persist` covers a toolchain that insists on writing inside the repository.

## Acceptance coverage

`workflow_09_workspace_round_trip` in [`boot_workflows.rs`](../../tests/boot_workflows.rs) covers one tree and host ownership. `workflow_20_many_workspaces_one_sandbox`, in the same file, covers two trees in one VM, round-trips an edit through both, and proves `exec` and `shell` preserve the exact invoking tree or subdirectory. [`../../tests/host/base-image-check`](../../tests/host/base-image-check) covers the guest half independently, comparing what the bind unit reported against the guest's own mount table so the two can be seen to disagree.
