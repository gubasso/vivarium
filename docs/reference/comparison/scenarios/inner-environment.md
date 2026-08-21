# Inner environment

A project usually already describes its own toolchain — a `flake.nix` with direnv, a version manager, or an equivalent. Record what happens to that description inside the sandbox.[^read]

1. Take a project whose toolchain is declared in its own repository.
2. Start the sandbox and enter the workspace.
3. Record whether the project's toolchain is active, and what the user had to change to get there.

## vivarium

Yes, specified and not yet built: [`06-workspace-and-project-environment.md`](../../spec/06-workspace-and-project-environment.md) makes this a design requirement rather than a convenience. The project's environment is the inner layer — owned by the repository, never modified by vivarium, and required to work identically whether or not the sandbox is in use. Two Nix evaluations exist and must not be conflated: the outer one builds the VM from the manifest on the host, the inner one builds the project's environment inside the guest when a shell enters the workspace, with separate files, separate lockfiles, and separate times. For that to work the base must ship a Nix toolchain with flakes enabled and direnv, and `viv shell` enters as a login-interactive shell so direnv can load it. The `*` is the shipped guest: it enables neither `nix-command` nor `flakes` globally and installs no direnv, leaving the entry-time half of the requirement unmet.

## flake-pilot

At the firecracker route, no: there is no project to enter. A registration is per application, and the guest's init is `sci`, which evaluates the single `run=` command from the kernel command line, executes it, and reboots. Nothing mounts a project tree and nothing runs a login shell in it, so a repository's own toolchain has neither a place to be nor a moment to load.

At the `krun` route, reachable and nothing arranges it: the registration mounts a host directory and sets `--workdir` to it, and its target is `/bin/bash`, so a project living under that directory is entered with a shell in it. Whether the project's declared toolchain then activates is a property of the image somebody chose — an image shipping direnv or a version manager loads it, an image without one does not — which is [the same answer podman gets, for the same reason](#podman). What flake-pilot adds is that the choice was made once at registration rather than at each start; what it does not add is any notion that a project has a toolchain to load.

## glaipnir

Reachable, nothing arranges it: the workspace is mounted, so the project's own files — including its `flake.nix` or version-manager config — are visible inside, and the base fixed in the `Containerfile` carries no Nix, no direnv, and no version manager to read them. What closes the gap is the extension surface glaipnir already provides: a build hook installs the loader, a run hook starts it on every start. Both are the user's to write, nothing asks for them, and a project whose toolchain is declared and never activated looks exactly like one that has none.

## podman

Reachable, nothing arranges it: bind-mount the project and its files are there, and an image that ships direnv or a version manager loads them — building that image and pointing `ENTRYPOINT` at a login shell are both ordinary. Nothing in podman asks for any of it, so whether a project's declared toolchain activates is a property of the image someone chose, and the same repository works for one colleague and not another.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `44e3ab2`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-20.
