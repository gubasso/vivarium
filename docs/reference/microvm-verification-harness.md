# microVM verification harness

`scripts/first-microvm-check` builds the first microVM, boots it on a real host, and checks the things only a real host can answer. It is the base the host lane of [`testing-lanes.md`](./testing-lanes.md) grows from, and it is run by hand today.

`scripts/store-gc-interlock-check` is its sibling, described below: same shapes, separate script because it mutates the host store.

`scripts/store-density-check` is a third sibling, and the only one that never boots anything. It measures the host store's bytes-per-inode distribution and reads a freshly created store volume's inode table with `dumpe2fs`, both of which answer ADR-0091 questions that no amount of booting could reach. It needs Nix and nothing else — no `/dev/kvm`, no systemd.

`scripts/store-pressure-check` drives the guest store to a real space crossing. Its `--arm c` fakes free space through upstream Nix's own test hook, so it proves the trigger and its arithmetic and nothing about reclamation; `--arm e` boots a **different image** whose thresholds are scaled at build time and whose store daemon has no hook at all, which is the only shape that can measure a real collection.

`scripts/share-benchmark-check` is the fifth, and the only one that boots more than once per invocation: it sweeps virtiofsd's worker-pool size across four launcher variants that share one guest closure, and measures what working through a share costs against the guest's own volume.

**Since ADR-0095, every probe unit lives in a measurement image rather than the shipped one.** The lanes that boot build `nix#first-microvm-measurement` or a purpose-built variant; `packages.first-microvm` — the artifact a user gets — contains no probe, no upstream test hook, and no way to stop itself. That last point is deliberate: the harness stops it with `ch-remote power-button` over the API socket, which is the path a user's `stop` will take.

**The microVM is its own flake, at `nix/flake.nix`.** The repository root's flake is the development environment — Rust toolchain, pre-commit runtimes, the devShell direnv activates — and carries no product input or output, which is why every lane resolves `path:$REPO_ROOT/nix` and only the lint checks reach back to the root for their tools.

This page is the lookup material for all five: what each check proves, and the register of what has actually been verified. It holds no design rationale — that lives in the ADRs each section names.

Each script also accepts `--clean`, which removes that lane's retained images, logs and orphaned run directories and exits. It is opt-in on purpose: the retained store volume is what makes the _warm_ half of the persistence pair possible, and the console log is a run's primary evidence, so neither may be tidied away as a side effect of a normal exit. A run directory whose `.pid` files still name a live process is kept, not removed.

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

## The sibling script: `scripts/store-gc-interlock-check`

ADR-0085's measurement runs from its own script, not from `first-microvm-check`, because it **deletes from the invoking user's real host store** — which must never be a side effect of the routine harness — and because it needs a prerequisite the harness does not: a store this user may delete from. It emits the same four result kinds and the same stable check inventory.

```console
$ scripts/store-gc-interlock-check
```

Two properties are worth knowing before reading a result from it.

- **`FAIL` means the experiment could not be performed** — no read before the deletion, no handshake, no deletion, no console. A symptom is never a `FAIL`: ADR-0085 has no prediction to falsify, so every symptom is a `[RECORD]` and the run derives one `symptom-class` line from them.
- **Two gates decide whether any symptom is attributable at all.** A _control_ path is realised, never touched by the guest, and deleted in the same host step; if the guest can still read it, the deletion did not propagate and every symptom check is emitted as `[SKIP]`. And the guest must have genuinely read the target _before_ the deletion — removing a path nothing cached proves nothing — which is a hard `FAIL` if it did not. Both gates were confirmed to fire by deliberate-negative runs: a rooted canary skips the whole lane rather than passing, and a suppressed before-phase read fails rather than reporting symptoms.

The guest half is the `vivarium-gc-interlock` unit, which is inert in the ordinary lane: with no instruction file in the workspace it reports `no-instruction` and exits, writing nothing. A plain `first-microvm-check` run is unchanged by its presence.

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

That budget is **derivable, and this number confirms the derivation.** The daemon subtracts a fixed internal reserve — 609, plus one per worker in the pool — from whichever descriptor limit it gets, and hands the rest to the guest: at vivarium's pool size, 524,288 − 613 = 523,675 exactly. So the run also reports the limit the daemon actually received, 524,288 rather than the million it aims for, because unprivileged it cannot raise its own hard limit. The consequences are [`ADR-0093`](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md)'s.

Decided in [`../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md`](../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md). With the path masked, collections complete in seconds on both boots. Note this is not a `local-overlay` defect: the public prior art's lower layer is a generated store image holding store paths and nothing else, so the directory is simply not there. It appears here because [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md) shares the host's literal store.

One cosmetic consequence remains unfixed and is recorded so nobody reads it as data: a collection reports deleting tens of thousands of paths and freeing an absurd quantity, because the collector counts every merged path it visits while `deleteStorePath` correctly does nothing for the lower-only ones.

### The store volume's inode provisioning holds, and the lazy-table hazard did not appear

Measured on both boots. At one inode per 8 KiB a 32 GiB volume carries **4,194,304** inodes, of which the guest used 228 after a cold boot — so [`../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md`](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md)'s ratio is now measured at the provisioning end, though not yet at the exhaustion end, which needs a workload rather than a boot.

That decision's open hazard — a lazily initialised inode table inflating the sparse image by about a gibibyte after first mount — **was not observed**: the image's allocated blocks were 327 MiB after the cold boot and 356 MiB after the warm one, against a fully written table of roughly 1 GiB. The measurement is honest about its own reach: each VM lived about three minutes, which is not long enough to rule out a background initialisation that a long-running guest would complete. Eager initialisation stays unnecessary on this evidence and unproven against a long session.

### A host collection is loud for a fresh lookup and silent for a path the guest still holds

Measured by `scripts/store-gc-interlock-check` (see [`testing-lanes.md`](./testing-lanes.md)) on a real host: the guest read a store path in full, the host deleted it with `nix-store --delete` while the guest ran, and the guest read again. This is the measurement [`../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md`](../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) left open. The symptom class is **MIXED**, and — the part the decision turns on — **nothing was ever corrupt**: every read that succeeded returned bytes identical to the host's pre-deletion digest.

| Access shape                                                    | After the host deletion                                                                    |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Control path, never read by the guest                           | `ENOENT` — so the deletion did propagate                                                   |
| Re-read by name, caches warm                                    | succeeds, bytes identical                                                                  |
| `stat`, caches warm                                             | succeeds, same inode, `nlink=1`                                                            |
| `readdir`, caches warm                                          | succeeds, both entries listed                                                              |
| Read from a descriptor opened before the deletion               | succeeds, bytes identical                                                                  |
| **Sibling file, after `drop_caches`, nothing pinning it**       | **`ENOENT`**                                                                               |
| Payload after `drop_caches`, still pinned by an open descriptor | succeeds, bytes identical                                                                  |
| `readdir` after `drop_caches`                                   | succeeds, **and lists nothing** — while a read by name of an entry it omits still succeeds |
| After a further 60 s                                            | unchanged from the round above                                                             |

The mechanism accounts for the split. virtiofsd runs with `--inode-file-handles=never` (`nix/launch-arguments.nix`), so it holds an open descriptor per inode the guest knows; the host's `unlink` frees no blocks while that lasts, and reads keep returning the real bytes. Once the guest is made to forget the inode, virtiofsd closes its descriptor and a lookup by name reaches a host path that is gone — which is why the sibling, which nothing pinned, is the one that went loud. The share's `cache = "always"` sets an entry timeout of 86400 s, so a timeout-driven revalidation is structurally out of reach inside one boot; the 60 s round records that rather than hoping otherwise.

Two consequences worth keeping separate. The realistic hazard — a build that resolves a store path it does not already hold open — is **loud**, which is the bet ADR-0085 made. The residual is narrower than "silent": a process holding the file open keeps reading correct bytes, which is not a correctness fault at all. What has no clean reading is the empty-`readdir`-with-successful-read state, which is precisely the "behaviour becomes undefined" the kernel documents rather than any particular error.

**Stated gap: `mmap` is not measured.** Bash cannot hold a mapping across the deletion, and a `SIGBUS` death is indistinguishable from a `timeout` kill, so a successful read of freed blocks _through a mapping_ is outside this script's reach. No check is emitted for it — a check that cannot fail is worse than none. Closing it needs a small compiled probe.

### `set -e` is prepended to every NixOS `script`, and it silently killed a probe unit

Found while building the check above, and general enough to record. NixOS emits `set -e` ahead of a `systemd.services.<name>.script` body. For a unit whose purpose is to run commands that are _expected_ to fail and report the status they failed with, that is fatal: the first such probe took the shell down inside a command substitution, before it could print its own result.

It failed **invisibly**. systemd stops writing unit status to the console once boot has completed, and this unit runs after `multi-user.target`, so the console showed a missing marker and nothing else — indistinguishable from a probe that returned no output. The fix is `set +e` in the body with a comment saying why; the diagnosis came from an `ExecStopPost` that echoes `$SERVICE_RESULT`/`$EXIT_CODE`/`$EXIT_STATUS` to the console, which runs even when the main process dies by signal. Any future probe unit here should carry both.

### The store's aggregate density clears ADR-0091's ratio, but the median store path does not

Measured by `scripts/store-density-check` on a real host, over three populations. Bytes are the sum of regular-file sizes; inodes are counted as **directory entries**, because a store path is materialised from a NAR and a NAR has no hardlink concept — every link becomes its own file in a guest.

| population                         | paths | aggregate | p10   | median | p90     |
| ---------------------------------- | ----- | --------- | ----- | ------ | ------- |
| random host store sample           | 300   | 9,518     | 205   | 3,915  | 150,092 |
| the `first-microvm` runner closure | 552   | 22,540    | 38    | 4,979  | 112,835 |
| this repo's devShell closure       | 170   | 18,621    | 1,496 | 20,380 | 732,653 |

All figures are bytes per inode. The **aggregate** corroborates [`../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md`](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md)'s 11 KiB argument well enough — at 9.5–22.5 KiB per inode, bytes bind before inodes at a provisioning ratio of 8192, which is the outcome that decision wants. The **median path does not**: at 3,915 and 4,979 bytes per inode, two of the three populations sit _below_ 8192, so a guest store dominated by ordinary small paths exhausts inodes while the volume still reports free space — the exact failure ADR-0091 exists to prevent and [`../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md`](../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md)'s block-reading trigger is structurally unable to see.

The spread is why an aggregate was the wrong statistic to bet the ratio on: the p90 column is three orders of magnitude above the p10, and a single large path (`rustc` alone measures ~287 KiB per inode) moves an aggregate that no typical path resembles.

Two directional caveats, both recorded rather than corrected for. The host store is hardlink-deduplicated — **567,161 entries** in `/nix/store/.links` on the measured host — so its inode count is optimistically low. The guest masks `.links` with a tmpfs (ADR-0092) and can never deduplicate. **Both point the same way: the guest's realised density is worse than the numbers above, not better.** Measuring it in a guest needs a workload rather than a boot and is not closed by this entry.

### `mkfs.ext4` leaves three quarters of the store volume's inode table unwritten, so ADR-0091's hazard is real and merely deferred

Measured directly by `scripts/store-density-check` with `dumpe2fs`, on an image created with the launcher's own arguments read out of the built launch-arguments JSON rather than reconstructed (`sizeMiB=32768`, `inodeRatio=8192`, `label=vivarium-store`). This replaces ADR-0091's inference from allocated-block sampling with a reading of the filesystem's own metadata, before any mount.

The table is **4,194,304 inodes × 256 bytes = 1,073,741,824 bytes** across 257 block groups. Immediately after `mkfs.ext4`, the sparse image holds **273,104,896 bytes** of allocated blocks — about 25% of the table. The feature list includes `metadata_csum`, and `/sys/fs/ext4/features/lazy_itable_init` is present on the host, so mke2fs's lazy default applies and the kernel's `ext4lazyinit` thread zeroes the remainder in the background after first mount. **So roughly 800 MiB of allocation is owed the moment the volume is first mounted, with the guest writing nothing** — which is the interaction with [`../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../decisions/ADR-0037-volume-disk-format-and-reclamation.md)'s sparse-image promise that ADR-0091 flagged.

This also reinterprets the persistence spike's own numbers. That run recorded 327 MiB allocated after a cold boot and 356 MiB after a warm one, and read them as "no inflation occurred". Against the 273 MiB that `mkfs` alone accounts for, the cold boot had completed only ~54 MiB of background zeroing — so those readings do not show the hazard failing to appear, they show a three-minute VM catching it barely started. **Stated gap:** how far `ext4lazyinit` actually gets, and what the image weighs when it finishes, still needs a guest that lives long enough to find out.

### ADR-0089's trigger fires, and frees exactly `max-free` minus available

Measured by `scripts/store-pressure-check --arm c` on a real host, guest Nix 2.34.7, through upstream's own `_NIX_TEST_FREE_SPACE_FILE` hook — the one `tests/functional/gc-auto.sh` uses — which makes the daemon read free space from a file instead of `statvfs`. The guest reported `min-free=4294967296`, `max-free=8589934592`, `min-free-check-interval=5`, `auto-optimise-store=false`.

| free space presented | auto-GC announced | bytes the collector asked to free |
| -------------------- | ----------------- | --------------------------------- |
| 16 GiB               | no                | —                                 |
| 8 GiB                | no                | —                                 |
| 6 GiB                | no                | —                                 |
| 5 GiB                | no                | —                                 |
| 4 GiB                | no                | —                                 |
| 3 GiB                | **yes**           | 5,368,709,120 (5 GiB)             |
| 2 GiB                | **yes**           | 6,442,450,944 (6 GiB)             |

Three things follow. The trigger **fires**, which [`../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md`](../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md) asserted and nothing had shown. It fires **strictly below** `min-free`, not at it — 4 GiB exactly is not a crossing. And the amount is exactly **`max-free` minus available** (8 − 3 = 5, 8 − 2 = 6), so the decision's "collect until `max-free` is free again" is arithmetic, confirmed on the running daemon rather than read off a source file.

**Stated gap, and it is not a small one.** With free space faked, `availAfterGC` is faked too, so this entry proves the trigger and its arithmetic and says **nothing** about real reclamation — whether a real collection on real ext4 actually frees what the collector asked for. That is the open half.

### `statvfs` on the merged store does report the upper filesystem

Measured across every sample of both pressure arms: the raw statvfs fields for `/nix/store` and for `/nix/.rw-store` were identical in all of them — free blocks, block size, total blocks and file counts alike. This is the premise [`../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md`](../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md) rests on when it says the collector "measures the store volume and nothing else", and it had been argued from `ovl_statfs`'s behaviour rather than observed. The samples also carry `/nix/store/.links` as `tmpfs`, which is ADR-0092 in effect, and `auto-optimise-store=false` — together the reason identical ballast cannot be deduplicated and the density readings mean what they say.

### The collection thresholds cannot be reached from outside `nix.settings`, which bounded what arm D could measure

Measured by `scripts/store-pressure-check --arm d`, and recorded because it is the reason a real-reclamation number is still missing. The arm writes real ballast into the real store volume with `min-free` moved up close to actual free space, so a few gibibytes cross a real threshold. Twelve iterations wrote 3 GiB; free space fell from 29.03 GiB to 26.02 GiB, past a planned `min-free` of 27.53 GiB; no iteration hit `ENOSPC`; and **no auto-GC was ever announced**.

The cause is not ADR-0089. `LocalStore::autoGC` reads `settings.minFree` **inside the daemon**, and two routes to change it were tried and both failed silently: a client-side `--option min-free` is accepted by `nix-build` and ignored by the collector, and `NIX_USER_CONF_FILES` pointed at a config the daemon unit was told to read did not reach it either — with the file on disk and `systemctl restart nix-daemon` returning 0. The daemon kept the real 4 GiB throughout, and a store with 26 GiB free was right not to collect. **The only route proven to reach `autoGC` is `nix.settings` at image-build time**, which is how the real thresholds get there.

The run is not wasted: it is the evidence that the ballast premise holds (see the dead-path series below) and the source of the host-image entry that follows. **Stated gap: real reclamation on real ext4 remains unmeasured.** Closing it needs a measurement image whose `nix.settings` carry scaled thresholds, not a runtime override, plus two constraints read out of the pinned collector's own source rather than guessed. The metric is a `statvfs` delta in the guest and the host image's allocated-block delta — **not** the collector's `bytesFreed`, which is apparent size and diverges from what the filesystem returns by construction. And a second pass is damped: `autoGC` returns early while available space is still above 97% of the previous `availAfterGC`, so an image built to watch repeated collections must cross that too, or the silence reads as a broken trigger.

### A guest-side store collection returns no blocks to the host image

Measured across both pressure arms by sampling the store image's allocated blocks from the host every ten seconds while the guest ran. Arm D's series runs from **273,108,992 bytes** at first mount to a peak of **3,659,214,848**, and the last sample equals the peak: the image never shrank. That is [`../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../decisions/ADR-0037-volume-disk-format-and-reclamation.md)'s expected behaviour — reclamation there is periodic and discard-driven, not continuous — but it had not been observed for this volume, and it is the concrete reason a guest store that collects internally still costs the host its high-water mark until something trims it.

The same sampling closes part of ADR-0091's lazy-inode-table question from the other side. By the time the measurement unit ran, `dumpe2fs` inside the guest reported **256 of 257** block groups already carrying `ITABLE_ZEROED`, and the count was unchanged at the end of the run. So `ext4lazyinit` completes early in a guest's life rather than lingering — which is why the three-minute VMs of the persistence spike saw only partial allocation, and why the hazard is a first-minutes cost rather than a long-session one. What the host image shows over the same window is a rise from 273 MB to about 456 MB in arm C, well short of the full 1 GiB table, so the zeroing does not materialise as a gibibyte of host allocation.

### ADR-0089's trigger fires on a real filesystem, and the collection frees nothing

Measured by `scripts/store-pressure-check --arm e` on a real host — guest kernel 6.18.38, guest Nix 2.34.7, cloud-hypervisor 52.0, virtiofsd 1.13.3, store image on the host's btrfs. This is the arm the earlier ones could not reach: arm C drove the trigger through upstream's own free-space **test hook**, so `availAfterGC` was faked along with everything else, and arm D never reached the collector at all. Arm E boots an image whose `nix.settings` carry scaled thresholds — the only route into `LocalStore::autoGC` — and whose store daemon has **no test hook at all**, so every number below comes from real `statvfs` on real ext4.

Image: 4 GiB store volume, `min-free` 1 GiB, `max-free` 1.5 GiB. The gap is 33% of `max-free` against `autoGC`'s 3% re-arm damper, so silence between passes could not have been the damper.

**The trigger fires, and its arithmetic is exact on real free space.** Twelve 256 MiB ballast iterations drove free space from 3,843,940,352 down past `min-free`. On the iteration that crossed, the daemon announced one auto-GC and asked to free **754,675,712** bytes. Available at that moment was 855,937,024, and `1,610,612,736 − 855,937,024 = 754,675,712` exactly. ADR-0089's "collect until `max-free` is free again" is therefore confirmed against a real filesystem, not only against a hook.

**And the collection freed nothing.** This is the finding.

| quantity                                   | before the collection | after       |
| ------------------------------------------ | --------------------- | ----------- |
| guest free bytes (`statvfs`, merged store) | 855,937,024           | 587,325,440 |
| dead paths (`nix-store --gc --print-dead`) | 24,923                | 24,949      |

Free space did not recover — it fell by the 256 MiB the same iteration wrote — and the dead-path count **rose monotonically across the whole run and never fell**. No path was reclaimed. The host image's allocated blocks tell the same story from the outside.

**Stated gap: the cause is a hypothesis, not a measurement.** The dead set here is ~24,900 paths, which is essentially the entire host store seen through the overlay's lower layer, and a `local-overlay` store deliberately declines to delete a path that is valid only below. The collector's own `bytesFreed` is **apparent size** rather than allocated blocks — already on record — so it could satisfy a byte target by walking lower-only paths that cost the upper filesystem nothing, and stop. That is consistent with everything observed and **none of it is measured**. Closing it needs the collector's own deletion decisions instrumented per path, not another boot of this shape.

**What this does not say.** It does not say ADR-0089's policy is wrong, and it does not license a second collection mechanism. It says the policy does not currently bound the store volume in _this topology_, and that the reason is unknown.

### Guest `fstrim` does return blocks to the host, measured on a probe the collector cannot confound

Same run, same host. The trim that follows the collection above returned nothing — but that is uninterpretable, because a discard can only return blocks the **filesystem** freed and the collection had freed none. So the discard chain is measured separately, with a plain file and no Nix involvement at all: write 1 GiB into the store volume, `sync`, delete it, `sync`, `fstrim`.

| point               | guest free space | host image allocated blocks |
| ------------------- | ---------------- | --------------------------- |
| before the probe    | 560.1 MiB        | 3,206,057,984 (3058.0 MiB)  |
| probe written       | 0.0 MiB          | 4,132,691,968 (3941.2 MiB)  |
| deleted and trimmed | 560.1 MiB        | 3,331,567,616 (3177.1 MiB)  |

**The host image shrank by 801,124,352 bytes (764.0 MiB).** Blocks written inside the guest and then freed were returned to the host, through `fstrim` → `VIRTIO_BLK_T_DISCARD` → `fallocate(PUNCH_HOLE)`. Every layer of that chain was already verified in upstream source; this is the number that was missing, and it is [`../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../decisions/ADR-0037-volume-disk-format-and-reclamation.md)'s sparse-image promise holding on real hardware.

The guest's own view corroborates: the driver reported `discard_max_bytes=2199023255040` and `discard_granularity=512`, so cloud-hypervisor advertised DISCARD under `sparse=on` and the guest negotiated `VIRTIO_BLK_F_DISCARD`. `fstrim` itself reported 1017.7 MiB "trimmed", which is the **length of the ranges handed to the kernel** and not host bytes freed — the host-side allocated-block delta is the reclamation number, and the two differ by construction.

**Why the two legs had to be separated.** A single "collect, then trim, then look" experiment produces one null result with four possible causes. Splitting it gives two findings with two different homes: the collector frees nothing (open), and discard works (closed).

### The `st_blocks` metric is valid on this host, checked before it was relied on

Every reclamation number above is an allocated-block delta on an image file, and this host's `/home` is copy-on-write btrfs (`rw,relatime,ssd,space_cache=v2`, no compression), where delayed allocation could in principle hide a punched hole until a transaction commits. Checked rather than assumed: write 256 MiB, `sync`, read `st_blocks`; punch a 128 MiB hole with `fallocate --punch-hole --keep-size`; read again immediately, after `sync`, and after `sync` plus 35 s — longer than btrfs's 30 s default commit interval.

All three reads returned **134,217,728 bytes freed, exactly the hole, with no wait required**. The metric is sound here.

This is a statement about **this host**, recorded so the numbers above can be read. It is not a claim about host filesystems in general and must not become one in `docs/` — which is also why the reclamation lane was left where it already runs rather than relocated to find a friendlier filesystem.

### The worker-pool sweep does not discriminate, because the workload the backlog names has no concurrency

Measured by `scripts/share-benchmark-check` across `--thread-pool-size` {0, 1, 2, 4}, four boots sharing **one guest closure** — the pool size is launch-channel, and the lane asserts that closure equality before spending the boots. 50,000-file tree, generated once host-side and copied to the volume so both filesystems hold byte-identical content, digests compared on every boot. The guest page cache is dropped before **every** repetition, because the workspace share is `cache = "auto"` and a warm cache issues no filesystem traffic at all — which is the condition under which this measurement would say nothing.

| workload                               | pool 0 | pool 1 | pool 2 | pool 4 |
| -------------------------------------- | ------ | ------ | ------ | ------ |
| `git status`, virtiofs (ms, best of 3) | 271    | 309    | 275    | 304    |
| `rg`, virtiofs (ms, best of 3)         | 693    | 922    | 884    | 1058   |
| `git status`, ext4 volume (baseline)   | 60     | 58     | 59     | 68     |
| `rg`, ext4 volume (baseline)           | 296    | 337    | 305    | 315    |

**The metadata leg does not separate the pool sizes**: the spread _within_ one pool's three repetitions (271–369 ms at pool 0) is wider than the spread _between_ pools.

**The content leg does separate them, and it runs backwards** — repetitions are tight (693/695/703 at pool 0; 1058/1085/1090 at pool 4), so a 1.5× penalty at pool 4 is not noise.

**This does not move [`../decisions/ADR-0051-share-worker-pool-small-non-zero-uniform.md`](../decisions/ADR-0051-share-worker-pool-small-non-zero-uniform.md)'s constant, and the reason is the important part.** A non-zero pool exists to serve **concurrent** requests; with the pool disabled the daemon executes every request for that share on one thread, in order, with head-of-line blocking. Both workloads here are a single process, so neither exercises the thing the pool is for. What the numbers show is per-request dispatch overhead with nothing to overlap — a real cost, measured, and not the case the decision turns on. `todo.md`'s prescription of `git status` + `rg` cannot settle this constant; a concurrent workload is needed.

### Working over the share costs about four times a local volume for metadata

Same run. Against a byte-identical tree on the guest's own ext4 volume, `git status` over virtiofs is **4.5–5×** slower (271–304 ms against 58–68 ms) and `rg` is **2.3–3×** slower (693–1058 ms against 296–343 ms). That is the first measured figure behind [`spec/06`](./spec/06-workspace-and-project-environment.md)'s "keep regenerable caches off the share", which until now was argued from negative-lookup semantics alone.

**The cache-placement leg did not discriminate** — 62–81 ms with the tool cache on the share and on the volume alike, across every pool size — and it was a _proxy_ for a cache workload rather than a cache workload. So `spec/06`'s guidance keeps its semantic argument and gains a ratio, and the claim that the layout choice is "the larger win" stays unmade, exactly as `todo.md` requires.

Block throughput on the guest's volume, for context: sustained writes **1.4–1.5 GB/s** and reads **6.8–7.8 GB/s** with `O_DIRECT`, uniform across pool sizes as expected — the block path does not go through the filesystem daemon. First-pass writes ranged from 64 MB/s to 1.5 GB/s and are **not** comparable: the volume is a sparse image on copy-on-write btrfs, so a first write measures allocation. The second pass is the number.

**Stated gap: `guest_userspace_ms` came back empty on all four boots.** `systemd-analyze time` produced nothing from inside the measurement unit, so the guest-side half of boot timing is unmeasured. The host-side interval — launcher `exec` to console socket — was 247–371 ms across the four boots, and the shipped image reached `multi-user.target` in **7.1 s**. Neither is upstream Cloud Hypervisor's `boot_time_ms`, which measures a kernel-internal debug-I/O-port interval; see the naming note below.

### Cloud Hypervisor's published metric names are not reusable, so vivarium's benchmark uses its own

Not measured on a host — established from upstream source at the pinned v52.0, and recorded because `todo.md` instructed the opposite and the instruction was a bet that lost.

`docs/performance_metrics.md` at that tag is a catalogue of names with **no definitions**. The implementation supplies them, and they are not what vivarium measures: `boot_time_ms` is the interval between guest debug-I/O-port codes `0x40` (kernel start) and `0x41` (user-space start) — a kernel-internal window that excludes everything before kernel entry — and `block_read_MiBps`/`block_write_MiBps` come from `fio --direct=1 --bs=4k --ioengine=io_uring` against a **raw dedicated block device**. vivarium's figures are a launcher-to-ready wall-clock interval and `dd` against a mounted filesystem.

Publishing different measurements under upstream's names would be this register's own method note failing at the level of naming, and it is the shape most likely to survive review and mislead a later reader. Every vivarium benchmark figure therefore carries a vivarium name — `workspace_git_status_ms`, `home_volume_read_MiBps`, and so on — and the harness states the parameters with the number. Reproducing upstream's instrumentation was considered and refused: it is machinery in service of a comparison this project does not need.

### The base image boots and stops without carrying anything that measures it

Measured on a real host after the probe units moved out of the shipped image. `nix#first-microvm` reached `Reached target Multi-User System` **7.1 s** after the launcher was executed, with its console showing ordinary systemd status and no vivarium unit but `vivarium-volume-prepare.service`. The host then stopped it with `ch-remote power-button` over the API socket: the guest ran its full shutdown transaction and reached `System Power Off` **2.0 s** later, and the runtime directory was empty afterwards.

Two things this closes. The ACPI power-button path was **unverified** — the guest module configures no `logind` policy and no ACPI handling, so whether a power button reached a poweroff depended on kernel and `systemd-logind` defaults neither of which had been confirmed by booting. It does. And it is the mechanism [`../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md`](../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md) relies on, since the shipped image deliberately has no way to stop itself.

The shutdown log also shows `Stopping Create swap on /dev/zram0`, which is [`../decisions/ADR-0094-guest-memory-posture-takes-the-distribution-defaults.md`](../decisions/ADR-0094-guest-memory-posture-takes-the-distribution-defaults.md)'s posture **activated** rather than merely configured — the distinction that matters, because the swap device is generated rather than declared and its own upstream documents a reset race that can leave one initialised but unusable.

### Relocating the probe units changed nothing about what they measure

Not a host measurement — a build-time comparison, recorded because it is what licenses _not_ re-running three already-closed lanes.

The three legs that produced this register's existing entries — `vivarium-store-spike`, `vivarium-gc-interlock`, `vivarium-store-pressure` — resolve to the **same script derivation** before and after the move, which is stronger than a textual diff because the derivation is content-addressed by its body. Their `After=` dependency sets are identical; only the _order_ systemd emits them in changed, which it treats as a set. `/etc/tmpfiles.d/00-nixos.conf` is an identical set on both sides.

Two deltas are intended and are the point of the change: the shipped image's unit set is now exactly `vivarium-volume-prepare.service`, with no upstream Nix test hook on its store daemon and no `systemctl poweroff` anywhere; and a composed `vivarium-measurement-stop` unit owns stopping, so an image built with any leg selection stops instead of only the one that happened to include the diagnostic.

Confirmed by boot: `scripts/first-microvm-check` against the measurement image returned `PASS=33 FAIL=0 SKIP=1`, identical to the cold-boot result recorded for the pre-refactor image.

## The method note

Five of this harness's own checks have been defects of the same shape — a check whose _form_ encoded a wrong assumption, so it passed or failed for the wrong reason:

- demanding a seccomp filter of a thread-group leader that never carries one;
- a `[PASS]` that tested process exit status while claiming the guest diagnostic had completed;
- a burst-fidelity check comparing **bytes** against a threshold that the guest log daemon's own line prefixes cleared unaided, while lines were genuinely missing;
- an inode-headroom gate that read a free-inode count of zero as exhaustion, when a filesystem with no fixed inode table reports zero for **both** the total and the free count and means "not applicable". It failed a host with 160 GiB free;
- an image that carried upstream Nix's free-space **test hook** into an arm measuring a real crossing. The hook was seeded far above the threshold, so it was inert _by value_ — and the arm needed it absent _by construction_. A daemon reading a file that says one tebibyte never consults `statvfs` at all, so the run reported "the trigger did not fire" about a trigger that was never shown the crossing.

A check that fails is cheap. A check that passes vacuously, or fails for the wrong reason, sends the fix to the wrong layer — and the third one above hid a real transport defect behind a green result for two rounds. When a check's subject and its assertion can drift apart, assert on the thing the check is named after.

The last two are worth separating from the first three, because they are not about assertions at all. **Inert is not absent.** A knob set to a harmless value is still a knob in the path, and an experiment that needs the path clear must remove it rather than neutralise it. And **a value that is "not applicable" is not a value**: before treating a reading as a quantity, check that the thing being read has one.

These are all defects in _checks_. The same round produced one in the repository's own shape, and it is worth naming beside them because the failure mode is identical: the root flake had been accumulating product outputs one plausible line at a time, until a dev-environment file was building guests. Prose alone would not have caught the next one, so `scripts/check-flake-boundary` now asserts it as a pre-commit hook — on the flake's **evaluated attribute names**, not its source text, because an output merged in with `//` is invisible to a grep and that is exactly the shape that got through. The rule itself lives in `AGENTS.md`.

One more, from the same round and cheaper to state: a lane that **restates** a build-time constant instead of reading it out of the artifact will drift the moment the artifact gains a parameter. The threshold gate here re-declared the scaled variant's attrset, drifted, and refused a correct image — the same two-copies-of-one-constant defect the build contract exists to avoid. It now reads the guest system out of the launcher's own `--cmdline`.
