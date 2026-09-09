# ADR-0105: A file mount is staged into its own export root

## Context and Problem Statement

virtiofs exports a directory tree, so slice 019 served a declared regular-file mount through its parent directory and hid the siblings guest-side. That shadow bounds ordinary guest processes but not guest root, which can remount the share's tag and reach every entry of the parent. A declaration naming one file granted a whole directory, and for `${HOME}/.gitconfig` that directory is the home.

## Considered Options

- Stage the file: a private user and mount namespace per file share, holding a tmpfs export root whose only entry is the declared file, with the share's existing daemon started inside it.
- Narrow the declaration: refuse regular-file sources before boot and require a dedicated directory.
- Restrict the export in the daemon: a virtiofsd allowlist option, or Landlock around it.
- Hard link, or copy with write-back, into a staging directory.

## Decision Outcome

Chosen option: `Stage the file` — it is the only candidate that keeps the declaration, keeps live host edits, and makes the exposure impossible rather than discouraged, at the cost of one wrapper on one share kind.

Each rejection was measured on the target host against the pinned `virtiofsd 1.14.0` and `util-linux 2.42`, kernel 7.1.4, and recorded in [`the harness findings`](../reference/microvm-verification-harness.md): the daemon offers no entry allowlist; a Landlock domain is inherited and irreversible, so applying it before exec also constrains socket and sandbox setup; a hard link fails across filesystems; a copy stops tracking host edits; and no guest-side masking survives guest root.

## Consequences

- Good: the daemon's own root holds exactly the declared file, so the confinement binds every guest process including root, and the guest-side shadow is deleted rather than trusted.
- Good: no new host premise — `host-userns-available` is already a hard `viv doctor` check, and `unshare` is already a backend program.
- Bad: a file mount pins an inode, so a host-side atomic replacement stops reaching the guest; the guest-side bind pinned it already, and Kubernetes `subPath` documents the same trade-off.
- Bad: the launch record's `source` changed meaning under a file plan, which costs a schema bump to 9.

## Status

Implemented — the launch path in [`src/launch/policy.rs`](../../src/launch/policy.rs) and [`src/bin/vivarium-supervisor.rs`](../../src/bin/vivarium-supervisor.rs), demonstrated by `workflow_22_file_mount_serves_only_its_file` in [`tests/boot_workflows.rs`](../../tests/boot_workflows.rs).
