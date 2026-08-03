# microVM verification harness

`scripts/first-microvm-check` builds the first microVM, boots it on a real host, and checks the things only a real host can answer. It is the base the host lane of [`testing-lanes.md`](./testing-lanes.md) grows from, and it is run by hand today.

This page is the lookup material for it: what each check proves, and the register of what has actually been verified. It holds no design rationale — that lives in the ADRs each section names.

## Running it

```console
$ scripts/first-microvm-check
```

No arguments. It resolves the flake from its own location, so it works from any working directory. Output is one `[PASS]` / `[FAIL]` / `[SKIP]` / `[RECORD]` line per check plus a verdict, and the whole run is meant to be pasted into a review.

Artefacts are retained beside the diagnostic volume under the state root: `console.log` (the raw guest console) and `memfd-series.txt` (the memory series described below). They are the reason to run it, so they outlive the run.

## The two tiers

The split is the **gate**, not the topic.

| Tier           | Gate                                                                                | What it can claim                           |
| -------------- | ----------------------------------------------------------------------------------- | ------------------------------------------- |
| **Evaluation** | Nix present                                                                         | Facts about Nix artifacts, and nothing else |
| **Host**       | `/dev/kvm`, systemd as PID 1, a systemd user manager, `$XDG_RUNTIME_DIR`, `python3` | Facts about a booted guest on this host     |

An evaluation-tier result says **nothing** about target-host behaviour. A skipped host-tier check is **unproven**, never absent. This distinction is the same one `AGENTS.md` draws when it forbids promoting an observation of an execution environment into a fact about a host.

## What the checks prove

### Purity and the build/launch boundary

- **Metamorphism** — the derivation path is unchanged by the invoking environment, the working directory, and the worktree's location on disk. Three separate checks, because each closes a different leak.
- **Negative reference** (ADR-0048) — no upstream runner or supervisor derivation is forced. vivarium generates the launch itself; forcing one of those would mean an upstream runner had written a host path into the build output, violating N5/N19.
- **Sentinel provenance** — neither launch-channel sentinel occurs anywhere in the derivation graph. Any occurrence is a defect.
- **No concrete host paths** in the graph.
- **Token flow, positively** — every scan above is negative, and a launcher that hardcoded a launch-channel value would pass all of them. The positive counterpart renders the launcher's arguments twice with different inputs and asserts each value tracked its argument.

### Backend and closure

- The backend, its control client, and the filesystem daemon are **closure members**, not `$PATH` lookups (ADR-0049).
- **Binary-cache reachability** for the hypervisor and the filesystem daemon on both supported architectures, so no user compiles a virtual machine monitor from source.

### Host tier

- **Landlock is accepted** by the pinned hypervisor.
- **Console fidelity** — see below.
- **Workspace mount contract** — the workspace is present, is the expected filesystem type, and is writable, established by reading the guest's own mount table rather than by writing a probe file into it (N9).
- **Confinement** — one hypervisor plus one filesystem daemon per share, every one owned by the invoking user with no-new-privileges set and a seccomp filter somewhere in its thread group. This is **not** the N20 allowlist test; it checks that confinement is on, not that it is correct.
- **Clean shutdown** and an empty runtime directory afterward, with the volumes retained.
- **The persistent guest store** — the premises, the sharing count, the collection's effect on the lower layer, and both branches of the delete-duplicate scenario. The persistence check itself needs **two runs against one store volume**: `VIVARIUM_SPIKE_COLD=1` removes the store image so the first is provably cold, and a second run without it is the warm half. On a cold run the persistence check reports `[SKIP]`, because there is nothing yet that could have survived.

## Findings register

Verified on a real host. Each entry names the version it applies to; nothing here is inferred from an agent's execution environment.

### Console transport is lossless once the guest log daemon is out of the path

Routing guest output through the guest's log daemon and letting it forward to the console loses bytes: it writes each line with a single best-effort write and never retries a short one, so a congested console truncates mid-byte and reports nothing. Measured at ~0.1% of a 1 MiB burst, with a host reader attached before the guest's first write and for the whole VM life.

Writing to the console device directly makes the burst **lossless across repeated runs**, and removes the daemon's per-line prefix — cutting console traffic about 3.7x for the same payload. Decided in [`../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md`](../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md).

Two further facts, both of which cost a run to learn:

- **The console does not replay.** Bytes written while no client is attached are discarded, so a late-connecting reader gets nothing that came before it. The reader must exist not merely for the VM's whole life but _from before the guest's first write_.
- **Interleaving is not loss.** The guest init writes status lines to the same console device; one landing mid-write splits a token across two lines with every byte still present. The harness distinguishes the two and reports contention separately, because reading one as the other sends the fix to the wrong layer.

### Guest memory does return to the host

Confirmed with the metric [`../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md`](../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) requires. A guest allocation of 512 MiB of guest RAM raised the backing object's allocated blocks by ~504 MiB; releasing it returned ~502 MiB, with proportional set size tracking the same curve. N22 and ADR-0035's headline hold on this host.

No adverse interaction was observed between reclaiming guest memory and the filesystem daemons' shared mappings of that same memory — both shares stayed functional across the reclamation and the guest completed and shut down cleanly. This was an explicitly flagged risk; it is now observed-not-reproduced rather than merely assumed.

### The shared store's validity stops at the boot closure

This is the important negative result, and it is measured as a **pair** — a single question could not have produced it:

| Question                                                                 | Answer |
| ------------------------------------------------------------------------ | ------ |
| Is a path inside the guest system's own closure valid to the guest?      | yes    |
| Is an arbitrary host store path, physically present in the share, valid? | no     |

Only the guest system's closure is registered in the guest's Nix database at boot. Every other host store path is **physically present and formally unknown**. The observed consequence: the project's own inner `nix develop` declines a path it can see (`is not valid`) and falls through to fetching it — which the boot used for this run, having no egress, could not do.

So [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md)'s benefit is proven for the boot closure and **disproven for the inner layer**.

Two later corrections to what this result was taken to mean, neither of which touches the measurement:

- **The failure is bounded by egress mode, not universal.** This boot had none. Egress is open by default ([`../decisions/ADR-0007-default-open-egress.md`](../decisions/ADR-0007-default-open-egress.md)), and an inner environment on a default sandbox substitutes normally into the writable overlay. The severity recorded here is the `allowlist`-mode severity, which the original wording did not distinguish. **Re-running this check must fix the egress mode explicitly and say which one it fixed** — a boot that silently has egress turns this finding into its opposite.
- **It is no longer a defect to close.** [`../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md`](../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md) rules that the inner layer provisions its own store and vivarium never bridges host bytes into it, so this pair now measures a **stated property** rather than a gap. It stays in the register because it is the evidence that property rests on, and because a future change that made an arbitrary host path valid to the guest would be a regression this pair detects.

One thing the pair does **not** measure, and which the design that followed rests on: **where the fetched bytes land.** "Falls through to fetching it" was observed; that the fetch materializes the path in the overlay's writable layer rather than reusing the lower one was not. The next entry answers that from upstream source, so a re-run with egress available now **confirms a known mechanism** rather than deciding an open question — but it should still watch the writable layer directly, and it should state which egress mode it fixed.

The contract is in [`spec/06-workspace-and-project-environment.md`](./spec/06-workspace-and-project-environment.md).

### Nix replaces a store path by deleting it first

Not measured on a host — established from upstream source, and recorded because it settles the premise the entry above leaves open.

Nix never writes into a store path in place. Both the substitution path (`LocalStore::addToStore`) and the build path (`deletePath` followed by `movePath`) unlink the destination before restoring or renaming into it. Over an overlay, a path that is physically present in the lower layer but invalid in the guest's database is therefore **unlinked through the merged view and copied in full**, not reused — which is exactly the materialization [`../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) assumed.

The corollary is the one neither decision stated: the whiteout hazard is reachable from the **ordinary write path**, not only from a collection. That widens what [`../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) is protecting against without changing the response, because `local-overlay` guards the write path too — on the **lower store's database**, not on physical presence.

### Nix's collector reads the store directory, not only its database

Not measured on a host — established from upstream source, and recorded here because it is the premise a guest-side design rests on and it contradicts the documentation.

`LocalStore::collectGarbage` (`src/libstore/gc.cc`) `readdir()`s the store directory and disposes of **every** entry that is not a valid path in its own database, whether or not the name parses as a store path. Nix 2.3's comment states the intent outright: _"immediately delete all paths that aren't valid"_. Only `.`, `..`, `.links` and locked temporaries are spared, and `nix-collect-garbage`, `nix store gc` and `nix-store --gc` all reach the same function. Upstream's own functional test litters the store with untracked files and then asserts the directory is empty.

The manual describes only the other half — _"all paths in the Nix store not reachable … from a set of roots are deleted"_ — so this is **behaviour, not contract**, and must be cited as such.

Why it matters here: inside a guest whose `/nix/store` is an overlay over the host's, every unregistered host path is a deletion candidate, and deleting one writes a whiteout into the writable layer instead of touching the read-only host store. [`../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) is the response. What still needs a real host is the other direction — that a `local-overlay` store leaves lower-only paths alone in this topology, which upstream tests but not over virtiofs.

### A guest process outside the workspace's identity map cannot use the workspace

The workspace share maps exactly one guest identity to the invoking user's, and forbids the rest — so to a guest process running as any other identity, every file in the workspace belongs to someone else. Nix's own git handling refuses to open a repository under that condition, and the refusal is what a naive probe measures instead of whatever it meant to test.

This is the identity contract in [`spec/06-workspace-and-project-environment.md`](./spec/06-workspace-and-project-environment.md) working as specified, not a defect. Anything touching the workspace must run as the mapped project identity. Relaxing the map, or suppressing the ownership check, would trade the contract for a diagnostic's convenience.

### The share's cache policy cannot reach the overlay's stale-handle hazard

Not measured on a host — established from the daemon's own source and the kernel's documentation, and recorded because it narrows a premise that was carried as the one axis able to contradict an accepted decision.

virtiofsd's aggressive and bounded policies differ in exactly two things: the entry and attribute timeouts (86400 seconds against one), and a keep-cache hint applied on open. Both govern **how quickly a host-side change becomes visible inside the guest**. Neither touches the overlay's upper layer, which is an ext4 block device with no FUSE in its path, nor the `trusted.overlay.*` whiteout and opaque machinery that lives there. Upstream Nix attributes the stale-file-handle failure to deleting a duplicated path through the upper layer while the merged view holds handles to it — a mechanism entirely above the lower filesystem.

The daemon documents the aggressive policy as selectable "only when the file system has exclusive access to the directory". vivarium holds that precondition as a decision rather than a hope, and three independent primary sources state the same rule: [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md), Nix's `local-overlay` manual ("deleting or modifying store objects is not allowed"), and the kernel's overlayfs documentation ("changes to the underlying filesystems while part of a mounted overlay filesystem are not allowed"). The policy is therefore sound _because of_ an invariant this project already enforces, and unsound without it.

**Still unverified, and it is what the spike measures:** whether remounting the overlay invalidates the _lower_ filesystem's own dentry and page caches. No primary source was found either way. Under the immutability rule above it does not need to, because the lower bytes are identical before and after — but that is an argument, not a reading.

### Upstream's stale-file-handle scenario needs a writable lower store and is unreachable here

Not measured on a host — established by reading the scenario, and recorded because a plan to run it as written would have produced a confident `[SKIP]` for the wrong reason.

`stale-file-handle-inner.sh` provokes the failure by garbage-collecting the **lower** store three times and building into it twice. vivarium's lower store is a share served read-only and mounted `ro,nodev,nosuid,noexec` inside the guest, and [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md) forbids the lower layer changing under a running guest in any case. The scenario is structurally unreachable, and running it measures the share's read-only-ness rather than the hazard.

The reachable substitute is the `delete-duplicate` shape: materialize into the upper layer a path whose bytes exist below but which the lower database does not know — the ordinary write path, per the entry above — then delete it, assert the upper copy is gone and the merged view now resolves to the lower inode, then build it again and assert no stale handle. That exercises the same delete-through-upper-then-remount mechanism using only writes the topology permits.

Two upstream scenarios adapt cleanly and are worth carrying: `check-post-init`, which asserts the lower and merged stores agree on queries and verification, and `redundant-add`, which asserts a path already valid in the lower database is **not** copied up — the direct test of the sharing claim in [`../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md). Upstream puts every layer on tmpfs deliberately, so none of its scenarios has ever run over virtiofs; that gap is the whole reason for the spike. `optimise` must stay off — hardlinking across overlay layers fails, which is why upstream microvm.nix asserts store optimisation is disabled whenever a writable overlay is configured.

### overlayfs is governed by the guest kernel, and the guest is below the threshold that matters

Not measured on a host — established from the pinned closure, and recorded because the version this project would naturally quote is the wrong one.

Upstream Nix stopped reproducing the stale-handle failure at Linux 6.19 and converted its test into a skip. overlayfs runs **inside the guest**, so the governing version is the guest kernel from the pinned closure — 6.18.38 — not the host's. vivarium therefore sits on the side of that line where the hazard is expected to reproduce, and the remount hook [`../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) requires is load-bearing rather than vestigial. Any run of this harness must record **both** kernel versions and say which one governs.

The 6.19 claim itself is an observation without an explanation: the issue upstream cites reports the symptom and identifies no kernel change. It must be carried as "upstream observes non-reproduction and does not account for it", never as "fixed in 6.19".

### The overlay's upper layer must be a block device, because whiteouts are device nodes

Not measured on a host — established from the kernel's overlayfs documentation and an upstream microvm.nix failure report.

overlayfs records a whiteout as a character device with device number 0/0, and marks an opaque directory with a `trusted.overlay.*` extended attribute. The upper layer must therefore permit `mknod` and carry `trusted.*` or `user.*` xattrs, and must return a valid `d_type` from `readdir`. A virtiofs share offers no device nodes, which is why an attempt to back a writable store overlay with a share fails as `overlayfs: upper fs missing required features`.

This is the formal reason the writable layer in [`../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) is a volume and could not have been a share, and it is independent of the persistence argument that decision makes. ext4's defaults satisfy every requirement with no additional mkfs or mount option; the one provisioning change the store volume does need is in [`../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md`](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md).

### The prior art's module shape is integration-specific and does not transfer whole

Not measured on a host — established by evaluating both shapes against vivarium's own guest configuration, which is an evaluation-tier result and says nothing about host behaviour.

[`shazow/agentspace`](https://github.com/shazow/agentspace) runs the same `local-overlay`-over-virtiofs topology and its module sets `microvm.writableStoreOverlay = lib.mkForce null`, hand-writing the `/nix/store` overlay in its place. Evaluated against vivarium's guest, that renders the `nix-store.mount` drop-in as `What=store` where the shutdown-ordering contract requires `What=overlay`, because upstream microvm.nix re-enables its own drop-in under exactly the three conditions vivarium would then satisfy. The prior art escapes it only by never enabling initrd systemd, which vivarium does. Keeping the option non-null renders `What=overlay` and yields the same overlay upstream already generates.

The five configuration details the prior art paid for are unaffected and still vendored; what does not transfer is one line of its integration. Reasoned through in [`../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md), and guarded by the existing build-to-launch contract check, which already asserts the rendered drop-in.

### The guest's store survives a restart, database and all

Measured on a real host over two boots against one store volume — a cold one and a warm one — with guest kernel 6.18.38, Nix 2.34.7 (CppNix) and cloud-hypervisor 52.0. A single boot structurally cannot show this, which is why the pair is the unit of measurement.

The writable layer is a labelled block device mounted from the initrd (`/dev/vdb`, ext4), and its mount identifier is lower than the merged store's on every run, so it really was established before the overlay built from it — the observational half of what `neededForBoot` promises at evaluation. [`../decisions/ADR-0067-volume-prune-and-first-boot-home.md`](../decisions/ADR-0067-volume-prune-and-first-boot-home.md)'s stage-2 first-boot machinery was confirmed **absent** on it rather than assumed away.

On the warm boot a path the guest had added on the cold boot was **valid before anything was written**, with its bytes still in the writable layer. Validity is a database query, so that is the claim [`../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) rests on, measured rather than argued: the database persisted, not merely the bytes.

The two stores compose as the design says. The lower store's database describes the boot closure and is rebuilt from the boot-time registration each boot; the upper store's database is on the volume. Lower and merged views agreed on queries and verification for a sampled closure path on both boots.

### Not one boot-closure path was copied into the writable layer

Measured on both boots: **0 of 520** requisites of the running system had an entry in the writable layer. This is [`../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md)'s sharing claim as a number rather than as an expectation, and it settles the question that decision left open about what the volume grows by: what the guest fetched for itself, not a closure.

### A collection whiteouts exactly what upstream's code says it whiteouts, and nothing else

Measured on both boots, and it corrects the design checklist rather than confirming it.

`LocalOverlayStore::deleteStorePath` branches on `lowerStore->isValidPath(storePath)` — validity in the lower store's **database**, not presence of bytes below. So there are two behaviours, and the checklist described only one:

- **Path registered in the lower database**: deleted through the upper layer, the store remounted, and the merged view falls back to the lower inode with no whiteout at all. Confirmed by inode identity, not by absence of an error.
- **Path present below but unregistered**: deleted through the merged view, leaving a whiteout. This is upstream's `else` branch, so it is specified behaviour and not a defect — but it is durable now, because the writable layer persists. Every such path is one the guest will re-materialise instead of sharing.

The catastrophic case [`../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) exists to prevent does **not** occur: the lower layer's entry count was identical either side of a rooted collection (22,802 → 22,802), the running system stayed valid and present, a rooted path survived, and **no whiteout landed over a path the lower database knows**. That last number is the one the harness asserts, because counting whiteouts alone reports the specified branch as a failure — which it did, for one round.

Two cheap add-ons on the same boots: a process already executing from the store was **undisturbed** across a collection's remount; and the store share's root holds exactly one non-store name, `.links`, which the collector skips by name — see the next entry for what it does instead.

### A guest collection walked the host's link farm, and that is a consequence of sharing the host's store

Measured on the first host boot, then confirmed in Nix's own source. `LocalStore`'s optimisation directory is `realStoreDir/.links`, which for a `local-overlay` store is the merged `/nix/store` — so it resolved through the lower layer to the **host's** link farm. `removeUnusedLinks` `lstat`s every entry there and `unlink`s any whose link count is one, so a guest collection cost the host's link count and wrote into the guest's layer. The run exhausted the filesystem daemon's per-guest descriptor budget (0 of 523,675 remaining) before one collection finished, and every later probe on that boot failed for that reason rather than on its own merits.

Decided in [`../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md`](../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md). With the path masked, collections complete in seconds on both boots. Note this is not a `local-overlay` defect: the public prior art's lower layer is a generated store image holding store paths and nothing else, so the directory is simply not there. It appears here because [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md) shares the host's literal store.

One cosmetic consequence remains unfixed and is recorded so nobody reads it as data: a collection reports deleting tens of thousands of paths and freeing an absurd quantity, because the collector counts every merged path it visits while `deleteStorePath` correctly does nothing for the lower-only ones.

### The store volume's inode provisioning holds, and the lazy-table hazard did not appear

Measured on both boots. At one inode per 8 KiB a 32 GiB volume carries **4,194,304** inodes, of which the guest used 228 after a cold boot — so [`../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md`](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md)'s ratio is now measured at the provisioning end, though not yet at the exhaustion end, which needs a workload rather than a boot.

That decision's open hazard — a lazily initialised inode table inflating the sparse image by about a gibibyte after first mount — **was not observed**: the image's allocated blocks were 327 MiB after the cold boot and 356 MiB after the warm one, against a fully written table of roughly 1 GiB. The measurement is honest about its own reach: each VM lived about three minutes, which is not long enough to rule out a background initialisation that a long-running guest would complete. Eager initialisation stays unnecessary on this evidence and unproven against a long session.

## The method note

Three of this harness's own checks have been defects of the same shape — a check whose _form_ encoded a wrong assumption, so it passed or failed for the wrong reason:

- demanding a seccomp filter of a thread-group leader that never carries one;
- a `[PASS]` that tested process exit status while claiming the guest diagnostic had completed;
- a burst-fidelity check comparing **bytes** against a threshold that the guest log daemon's own line prefixes cleared unaided, while lines were genuinely missing.

A check that fails is cheap. A check that passes vacuously, or fails for the wrong reason, sends the fix to the wrong layer — and the third one above hid a real transport defect behind a green result for two rounds. When a check's subject and its assertion can drift apart, assert on the thing the check is named after.
