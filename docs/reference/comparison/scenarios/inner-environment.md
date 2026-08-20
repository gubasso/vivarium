# Inner environment

A project usually already describes its own toolchain — a `flake.nix` with direnv, a version manager, or an equivalent. Record what happens to that description inside the sandbox.

1. Take a project whose toolchain is declared in its own repository.
2. Start the sandbox and enter the workspace.
3. Record whether the project's toolchain is active, and what the user had to change to get there.

## vivarium

vivarium `ceb0027`, 2026-08-20. Yes, specified and not yet built: [`06-workspace-and-project-environment.md`](../../spec/06-workspace-and-project-environment.md) makes this a design requirement rather than a convenience. The project's environment is the inner layer — owned by the repository, never modified by vivarium, and required to work identically whether or not the sandbox is in use. Two Nix evaluations exist and must not be conflated: the outer one builds the VM from the manifest on the host, the inner one builds the project's environment inside the guest when a shell enters the workspace, with separate files, separate lockfiles, and separate times. For that to work the base must ship a Nix toolchain with flakes enabled and direnv, and `viv shell` enters as a login-interactive shell so direnv can load it. The `*` is the shipped guest: it enables neither `nix-command` nor `flakes` globally and installs no direnv, leaving the entry-time half of the requirement unmet.

## flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: there is no project to enter. A registration is per application, and at the firecracker rung the guest's init is `sci`, which evaluates the single `run=` command from the kernel command line, executes it, and reboots. Nothing mounts a project tree and nothing runs a login shell in it, so a repository's own toolchain has neither a place to be nor a moment to load.

## glaipnir

glaipnir `21ef389`, read 2026-08-20. Partial: the workspace is mounted, so the project's own files — including its `flake.nix` or version-manager config — are visible inside. What is missing is anything that reads them. The base is openSUSE Tumbleweed fixed in the `Containerfile`, and it carries no Nix, no direnv, and no version manager, so entering the sandbox leaves the project's declared toolchain inert. A run hook is where a user would add one, at their own expense.

## podman

podman 5.x, 2026-08-20. Partial: bind-mount the project and its files are there, and an image that happens to ship direnv or a version manager will load them. Nothing in podman asks for that, so whether the project's toolchain activates is a property of the image someone chose rather than of the tool — the same file works for one colleague and not another.
