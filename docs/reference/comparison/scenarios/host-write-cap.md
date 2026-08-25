# Host write cap

An agent that fills a disk has not escaped anything; it has just stopped the machine. Record whether the tool bounds how much the guest can write to host storage, and what happens at the bound.[^read]

1. Start the sandbox against a project on the host.
2. From inside, write until something stops.
3. Record what stopped it, how much had landed on the host, and whether the host had free space left.

## vivarium

No, by decision. The workspace is the host tree, shared at its own absolute path, and [`06-workspace-and-project-environment.md`](../../spec/06-workspace-and-project-environment.md) records that it carries no capacity bound: vivarium bounds the storage it creates and does not bound a tree the user shared. The other half of the disk model is where the bound lives — a declared volume carries `size_gib`, and the guest store's upper layer sits on one — and it does not reach a share, because a share has no image to size. Capping one would mean interposing a copy-on-write layer between the user and their own directory, which is a different product than a mount. A guest that fills the workspace fills the disk the project already sat on, and the write was authorized.

## flake-pilot

At the firecracker route, n/a: nothing crosses, and `overlay_size` bounds a second virtio-blk drive that is guest storage rather than a host share.

At the `krun` route, no: a registered `--volume` is stored and replayed, and the mount it produces is an ordinary bind mount with no quota.

## glaipnir

No: the workspace mounts under `/home/aiuser/` from the host tree, and nothing bounds writes into it.

## bunkerbox

Yes, and it is the default. In `cow` mode the guest sees an overlay whose lower layer is the repository read-only and whose upper layer is a loopback ext4 image at `.bunkerbox/upper.img`. The image is created at a fixed size before boot, so the guest can write at most that many bytes to host storage no matter what it does. Writes land in the upper layer and are synced back when the container exits, which is what makes the review-the-diff workflow possible at all.

The quota is computed rather than guessed: `auto` walks the repository skipping the excluded directories, adds ten percent, and floors at 5 GB. Excluded directories still live inside the image, so their output counts against the cap; excluding them only keeps build output from inflating the estimate.

```yaml
project:
  quota: auto      # or "20G", "500M", "1048576"
  exclude:
    - target/
    - node_modules/
```

Two modes give it up. `direct` mounts the repository straight through, which upstream labels no guardrails and for trusted workloads only, and `isolated` clones the tree with `git worktree` into an uncapped copy.

[^read]: Read at `bunkerbox` `b7f14f3` on 2026-08-25, and at `vivarium` `162f230`. The `flake-pilot` and `glaipnir` verdicts are derived from the mount mechanisms already recorded at [the host-path row](./host-path.md), not from a fresh read.
