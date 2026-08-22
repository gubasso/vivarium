# 02 — Config and XDG layout

vivarium stores all state under standard per-user XDG directories, split by durability. The governing rule and rationale are in [`../../decisions/ADR-0005-xdg-user-config-layout.md`](../../decisions/ADR-0005-xdg-user-config-layout.md): config is authored, cache is derived, state is runtime, data is pinned inputs.

## Where the roots live

Each root is named by an XDG environment variable, and vivarium's own subtree is `vivarium/` under it. A variable that is unset, empty, or not an absolute path is treated as unset — a relative path is invalid and ignored rather than resolved against the working directory.

| Root    | Variable           | When unset                   |
| ------- | ------------------ | ---------------------------- |
| Config  | `$XDG_CONFIG_HOME` | `~/.config`                  |
| Data    | `$XDG_DATA_HOME`   | `~/.local/share`             |
| State   | `$XDG_STATE_HOME`  | `~/.local/state`             |
| Cache   | `$XDG_CACHE_HOME`  | `~/.cache`                   |
| Runtime | `$XDG_RUNTIME_DIR` | required — fail closed, `77` |

Where a default applies and `$HOME` cannot be resolved, the command fails `78` ([`14-exit-codes.md`](./14-exit-codes.md)) — a misconfigured environment, not a failing filesystem.

The runtime root is the one row that refuses, and the asymmetry is deliberate: unlike the other four it has no literal default, because it is a session-scoped, `0700`, tmpfs-backed directory a login manager creates and removes. vivarium requires it rather than synthesizing a replacement — a host that has none also has no user session manager to place a VM's processes in, so a synthesized directory would only buy a launch that fails an invariant later ([`../../decisions/ADR-0055-runtime-directory-is-required.md`](../../decisions/ADR-0055-runtime-directory-is-required.md)). Every run that needs it validates it (absolute, owned by the user, not group- or world-accessible) and reports the specific fault, not a generic one; the check is `runtime-dir-usable` in [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md).

## The four roots

The four durable roots — the durability split ADR-0005 draws. The runtime root is outside it by construction: it holds nothing that survives the session.

- Config root — the user's source of truth. Holds the global config file, the `images/` library, the `pieces/` library, and the `manifests/` library. Everything here is hand-authored and may be version-controlled by the user. vivarium only reads the config root; it never writes, creates, or scaffolds anything here (N13 in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) — config is read-only to the tool). The tool's own writes go to state, data, or cache only.
- Data root — pinned inputs: the per-target lockfile that pins what a project's build resolves to (see below), including the nodes for any external module library a shared artifact declares ([`03-artifact-model.md`](./03-artifact-model.md)). The lock carries nodes for `nixpkgs`, `microvm`, and the artifact-declared inputs — never a node for vivarium itself, because the installation supplies the tool and its product tree ([`../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../decisions/ADR-0102-the-installation-supplies-vivarium.md)). What the data root holds is the pin, never the fetched source — that lives in the Nix store like every other build input.
- State root — per-sandbox runtime state the tool writes: build records, persistent volumes, and the diagnostic log (`logs/vivarium.log`, written by default — see [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)). Each sandbox directory is keyed by its manifest name.
- Cache root — derived, regenerable artifacts: the workspace-owner index, the generated flake compiled from each manifest (see below), the Nix evaluation cache, and built VM images. Safe to delete; the tool rebuilds it.

The config, data, state, and cache roots hold, respectively, what the user edits, what is pinned as input, what a run produces, and what can be rebuilt. Any new artifact is placed by asking which of those four it is — and because config is read-only to the tool, anything the tool must write is by definition state, data, or cache, never config.

## The runtime root

`$XDG_RUNTIME_DIR/vivarium/<manifest>/<target>/` holds the files that exist only while a VM is running — the per-target `flock`, the control socket, the pid file, and the boot record. `<manifest>` is the selected manifest's artifact name and therefore the sandbox key. The layout mirrors the state root's `projects/<manifest>/<target>/` component for component. Their names and the ensure-running protocol that reads them are owned by [`12-exec-and-shell.md`](./12-exec-and-shell.md).

The control socket's rendered path MUST fit a Unix socket address, whose `sun_path` holds 108 bytes. Two rules bound it, and they are two rules for one failure because only one of them can be fixed in advance. A manifest name used as a sandbox key is refused at resolution when it exceeds 48 bytes, which is a bound on what a user declares; `$XDG_RUNTIME_DIR` is host-variable, so the rendered path is computed and refused at `host.runtime-socket-path-too-long` when it does not fit. A host with a long runtime directory can therefore refuse a name the byte rule accepts, and a green length check is not coverage of the socket path.

Nothing here is durable, so nothing here is ever swept: the directory is tmpfs-backed and torn down with the session that owns it. That is also why a VM does not outlive the user's final logout — the boundary is stated in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md) and decided in [`../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md`](../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md).

## Global config file

The config root holds one global config file, `config.toml`, carrying user-wide defaults. It is hand-authored and read-only to the tool; [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md) governs the annotated example a user copies from.

It carries defaults for cross-cutting concerns — the settings that apply to every command rather than to one project. Today that is the logging family specified in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md); each later cross-cutting knob joins the same chain. It sits one rung below the environment in the precedence standard, which is flag > environment variable > global config file > built-in default ([`../../decisions/ADR-0046-global-config-file-and-precedence.md`](../../decisions/ADR-0046-global-config-file-and-precedence.md), amending [`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md)).

Two things it deliberately does not carry. It does not bind a working directory to a manifest: explicit `[[workspaces]]` declarations and the resolution chain below own that relationship. It does not hold a colour setting: colour follows the environment-only chain `NO_COLOR > FORCE_COLOR > isatty` with no flag and no file key ([`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md)).

## Workspace ownership index (cache)

Manifest selection has three precedence rungs: `--manifest`, then `VIVARIUM_MANIFEST`, then ownership derived from the manifest library. The derived rung parses each manifest's explicit `[[workspaces]]` rows and selects the one whose expanded source contains the invoking directory. `[[mounts]]` never establish ownership. No owner and more than one owner both fail closed at `78`; two owners are named together. See N7 in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) and [`../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md`](../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md).

Scanning is accelerated by `$XDG_CACHE_HOME/vivarium/workspace-index.json`. The index stores only manifest names, resolved manifest paths and modification times, and unexpanded workspace source tokens. Every field is recomputable from the manifest library. A missing, malformed, or stale index is a cache miss: vivarium rebuilds it before resolving ownership. A changed library snapshot invalidates it. The selected manifest's current text is checked again before its ownership is trusted, so the cache narrows candidates and never becomes authority.

The old state-root `registry.toml` has no role in selection. If one remains from an older version, vivarium neither reads, writes, nor deletes it. There is no project-local binding or identity file, and no command writes inside a workspace (N9 and N13).

### Lockless publication

The index has no lock. Concurrent writers derive the same library snapshot and each publishes a complete value: create a same-directory temporary file, write and flush it, rename it over the cache, then fsync the parent directory. A reader sees an old complete index, a new complete index, or a cache miss it rebuilds — never a partial file. The cache directory is `0700` and the index is `0600`.

The first surviving mutable lock is the per-target `flock` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)); a future Nix profile lock follows it. Locks are acquired in that order, released in reverse, and never held across a VM boot or Nix build. [`../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) records the historical order and its amendments.

### Retained state without a manifest

Deleting or renaming a manifest can leave state and data under `projects/<manifest>/`. vivarium reports these retained paths through the soft `state-manifest-orphans` doctor probe and never deletes them automatically. This carries forward ADR-0054's report-rather-than-reap judgment; cleanup remains an explicit operator decision.

### State diagnostic ids

The `state.` namespace is documented here, which is where [`14-exit-codes.md`](./14-exit-codes.md) places it. `state.no-manifest` and `state.workspace-owner-ambiguous` are selection refusals at `78`. The two refusals a selected manifest can raise about itself carry the `manifest.` namespace and are documented with it in [`03-artifact-model.md`](./03-artifact-model.md). Tool-owned volume-record failures use `state.unreadable`, `state.syntax`, `state.type`, `state.write`, `state.write-temporary`, and `state.write-sync`. Build and volume provisioning use `state.build-record` and `state.volume-directory`.

The derived index uses `state.index-source-unreadable`, `state.index-unreadable`, `state.index-encode`, `state.index-parent`, `state.index-permissions`, `state.index-stage`, `state.index-temporary-collision`, `state.index-write`, `state.index-sync`, `state.index-publish`, and `state.index-directory-sync`. A malformed cache has no diagnostic id because it is rebuilt rather than reported as an authored defect. Permission-denied writes return `77`; other owned-channel I/O failures return `74`.

## Per-sandbox VM state

Each sandbox's runtime VM state lives under the state root at `projects/<manifest>/<target>/`. `<manifest>` is the selected manifest name. `<target>` reserves the named VM instance within that sandbox and is always `default` today: no flag selects another and no manifest key declares one.

The target component exists now so adding another target later does not relocate state. State and runtime layouts use it symmetrically, and the reason is that the two are read together: the ensure-running protocol resolves a runtime directory and its state directory from one pair of components, so a layout that spelled the pair differently on the two sides would make every such resolution carry a translation nobody can verify from either path alone. Multiple `exec` or `shell` sessions attach to one target rather than creating targets ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). This directory holds build records and persistent volumes (`volumes/<name>.img`, always including `default`). Volumes live under state, not cache, because their contents are user data and not regenerable. Generation layout and lifecycle are specified in [`11-generations-and-build-history.md`](./11-generations-and-build-history.md); the volume model in [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md) and [`../../decisions/ADR-0019-volume-model.md`](../../decisions/ADR-0019-volume-model.md).

## The generated flake and the lockfile

Building a project produces two tool-owned artifacts outside the state root, and a team may add a third the tool only ever reads. They sit in different roots because they have different durability, and the split is the whole point: one is regenerable, one is the pin that makes regeneration mean the same thing twice, and one is a team's own artifact.

```text
$XDG_CACHE_HOME/vivarium/flakes/<manifest>/<target>/     # generated flake — regenerable
$XDG_DATA_HOME/vivarium/projects/<manifest>/<target>/flake.lock   # pinned inputs — not regenerable
$XDG_CONFIG_HOME/vivarium/manifests/<name>/flake.lock      # optional team override — read-only to the tool
```

The generated flake is what `nix build` actually reads: the compiled manifest plus copies of the resolved image, pieces, and `extends` module ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md), [`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md)). It is regenerated wholesale, written to a temporary sibling and renamed into place so a concurrent run never reads a partial tree, and never hand-edited. `viv config` prints its path, which is how a user inspects it ([`01-command-surface.md`](./01-command-surface.md)).

### Diagnostic ids

Generated-tree and pin failures carry stable ids owned here. Permission-denied forms return `77`; other owned-channel I/O forms return `74`; a differing concurrent first pin returns `75`; a team override lock pinning a `vivarium` input returns `78`.

| Id                               | Condition                                                            |
| -------------------------------- | -------------------------------------------------------------------- |
| `store.create-parent`            | the owned cache parent cannot be created                             |
| `store.create-temporary`         | a temporary generated-tree sibling cannot be created                 |
| `store.temporary-collision`      | bounded unique-directory allocation is exhausted                     |
| `store.copy-inspect`             | a copied source entry cannot be inspected                            |
| `store.copy-create-directory`    | a copied destination directory cannot be created                     |
| `store.copy-read-directory`      | a copied source directory cannot be enumerated                       |
| `store.copy-read`                | an ordinary source file cannot be read                               |
| `store.copy-read-link`           | a source symbolic link cannot be read                                |
| `store.copy-create-link`         | a source symbolic link cannot be recreated                           |
| `store.copy-permissions`         | ordinary file or directory permissions cannot be preserved           |
| `store.unsupported-file-type`    | a copied source is not a directory, ordinary file, or symbolic link  |
| `store.write-parent`             | a generated destination parent cannot be created                     |
| `store.write-file`               | a rendered or staged generated file cannot be created or written     |
| `store.write-sync`               | a generated file cannot be flushed                                   |
| `store.sync-read-directory`      | a generated directory cannot be traversed before flushing            |
| `store.sync-inspect`             | a generated entry cannot be inspected before flushing                |
| `store.sync-directory`           | a generated directory cannot be flushed                              |
| `store.publish-inspect`          | the live generated-tree destination cannot be inspected              |
| `store.publish`                  | an initial complete tree cannot be renamed into place                |
| `store.publish-exchange`         | an existing tree cannot be atomically exchanged with its replacement |
| `store.publish-directory-sync`   | the cache parent cannot be flushed after publication                 |
| `store.cleanup-stale`            | the new tree is live but its exchanged old sibling cannot be removed |
| `lock.inspect`                   | an override or owned lock candidate cannot be inspected              |
| `lock.generated-read`            | a successful first build's generated lock cannot be read             |
| `lock.persist-parent`            | the owned data directory cannot be created                           |
| `lock.persist-temporary`         | a same-directory staged lock cannot be created                       |
| `lock.persist-write`             | staged first-pin bytes cannot be written                             |
| `lock.persist-sync`              | staged first-pin bytes cannot be flushed                             |
| `lock.persist-publish`           | a first pin cannot be installed with no-replace semantics            |
| `lock.persist-directory-sync`    | the data directory cannot be flushed after pin installation          |
| `lock.winner-read`               | a concurrently installed first pin cannot be read for comparison     |
| `lock.temporary-collision`       | bounded unique-file allocation is exhausted                          |
| `lock.first-pin-race`            | another process installed different first-pin bytes                  |
| `lock.read`                      | the lock in force cannot be read for planning                        |
| `lock.override-carries-vivarium` | a read-only team override pins a `vivarium` input no flake declares  |
| `lock.migrate-write`             | staged migrated-lock bytes cannot be written                         |
| `lock.migrate-sync`              | staged migrated-lock bytes cannot be flushed                         |
| `lock.migrate-install`           | a migrated lock cannot be installed over the retained one            |

The private `internal.generated-path`, `internal.artifact-parent`, `internal.manifest-parent`, `internal.generated-parent`, `internal.lock-parent`, and `internal.lock-persist-contract` ids report violated call or tree-shape invariants rather than authored or host failures.

The lockfile is data, not cache, because deleting it does not rebuild anything — it re-resolves, which is exactly what N3 forbids happening by accident. It is created by the first build or the first `viv update`, whichever comes first — each reports what it pinned — and thereafter moves only under `viv update` ([`../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). Creating a lock is not re-resolving one: a build may write the file that does not yet exist, but no build ever moves a pin that does. One lock per target rather than one per user: a global lock would make updating one project an unannounced update to every other.

One migration is sanctioned beside that rule. A retained owned lock from before [`ADR-0102`](../../decisions/ADR-0102-the-installation-supplies-vivarium.md) carries a dead `vivarium` node; the next preparation sheds that node, its root edge, and the nodes only it reached, announces the shed on stderr, and moves no surviving pin. A team override lock carrying one is refused with `78` under `lock.override-carries-vivarium` instead — the override is read-only to the tool, so its owner regenerates it.

### The team override lock

A team that wants one shared pin places a read-only `flake.lock` beside its manifest, at `manifests/<name>/flake.lock`. It wins over the per-target lock whenever it is present and is never written by the tool (N13) — the config root is where a team's shared guarantees already live ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). It is scoped to one manifest for the same reason the tool-owned lock is scoped to one target: a pin covering the whole config root would make adopting one team's pin an unannounced pin of every unrelated project. Because it sits inside a library directory without being a member of it, resolution never sees it and no listing enumerates it ([`../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md`](../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md)); it therefore requires the directory manifest form, which `extends` requires too.

While an override is in force, `viv update` refuses and writes nothing — not even the per-target lock it shadows, because a pin nothing reads today would become effective the moment the override were removed, which is precisely the unannounced input jump N3 exists to prevent. It exits `78` naming both files; moving the pin is the team's own act, outside vivarium ([`14-exit-codes.md`](./14-exit-codes.md), [`../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md`](../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)). Whichever lock is in force is the one `viv config` reports ([`01-command-surface.md`](./01-command-surface.md)) and the one a generation retains ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).

Security urgency does not weaken that refusal. Moving a pin in response to a published advisory is the same act as any other pin move, so under an override it is still the team's to make and `viv update` still refuses — which is why an advisory has to say so ([`../../../SECURITY.md`](../../../SECURITY.md), [`../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md)).

Without an override, two people building one manifest may resolve different inputs; that is the honest cost of a tool that may not write a project's own tree (N9).

## Resolution precedence

The effective manifest is resolved highest-wins, per [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md):

1. `--manifest` command-line flag — a single-invocation override, never persisted.
2. `VIVARIUM_MANIFEST` environment variable — a runtime override, never persisted.
3. The unique manifest whose explicit `[[workspaces]]` set contains the invoking directory, accelerated by the derived cache-root index.
4. Otherwise, fail closed naming the directory and the exact `[[workspaces]]` block to add. Two owners fail closed naming both manifests.

## The libraries

`images/`, `pieces/`, and `manifests/` under the config root hold the composable artifacts. Their shapes are specified in [`03-artifact-model.md`](./03-artifact-model.md). Images and pieces are Nix modules; manifests are TOML that the tool compiles, per [`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md).

A manifest names its layers by bare identifier — `image = "rust"`, `pieces = [ "git" ]` — and the tool resolves each identifier to one file in the matching library. A name is kebab-case: `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`. Resolution tries the flat form first and the directory form second, using the library's extension — `.nix` for `images/` and `pieces/`, `.toml` for `manifests/`:

| Library      | Tried first             | Tried second                    |
| ------------ | ----------------------- | ------------------------------- |
| `images/`    | `images/<name>.nix`     | `images/<name>/default.nix`     |
| `pieces/`    | `pieces/<name>.nix`     | `pieces/<name>/default.nix`     |
| `manifests/` | `manifests/<name>.toml` | `manifests/<name>/default.toml` |

The directory form exists so a multi-file artifact can keep its helper modules beside it; an image that imports a shared base is the motivating case ([`03-artifact-model.md`](./03-artifact-model.md)). Anything in a library directory that is not a member by these rules — a helper module, a README, an override lock, an `inputs.toml`, a nested name that is not kebab-case — is invisible to the readers and never enumerated. The directory form is required in three cases, each because the directory becomes the unit the tool copies or reads: a manifest that names `extends` ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md`](../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md)), a manifest that carries a team override lock, and an image or piece that declares flake inputs ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md`](../../decisions/ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md)). Everywhere else it stays a fallback. Both spellings of one name present is an ambiguity, not a precedence question: resolution fails closed naming both paths ([`14-exit-codes.md`](./14-exit-codes.md)). The resolved path is what `viv images list`, `viv manifest list`, and `viv manifest show` report as `path` ([`01-command-surface.md`](./01-command-surface.md)). Decided in [`../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md`](../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md).

The config root is the whole search path. There is no bundled library behind it and no fallback: what vivarium ships is examples to copy, which resolve nowhere until a user places them here ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md`](../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md)). So a name has exactly one meaning, a listing needs no provenance column, and an artifact is missing rather than silently satisfied from somewhere the user cannot edit.
