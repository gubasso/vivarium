# 02 — Config and XDG layout

vivarium stores all state under standard per-user XDG directories, split by durability. The governing rule and rationale are in [`../../decisions/ADR-0005-xdg-user-config-layout.md`](../../decisions/ADR-0005-xdg-user-config-layout.md): **config is authored, cache is derived, state is runtime, data is pinned inputs.**

## Where the roots live

Each root is named by an XDG environment variable, and vivarium's own subtree is `vivarium/` under it. A variable that is unset, **empty, or not an absolute path** is treated as unset — a relative path is invalid and ignored rather than resolved against the working directory.

| Root    | Variable           | When unset                       |
| ------- | ------------------ | -------------------------------- |
| Config  | `$XDG_CONFIG_HOME` | `~/.config`                      |
| Data    | `$XDG_DATA_HOME`   | `~/.local/share`                 |
| State   | `$XDG_STATE_HOME`  | `~/.local/state`                 |
| Cache   | `$XDG_CACHE_HOME`  | `~/.cache`                       |
| Runtime | `$XDG_RUNTIME_DIR` | **required — fail closed, `77`** |

Where a default applies and `$HOME` cannot be resolved, the command fails `78` ([`14-exit-codes.md`](./14-exit-codes.md)) — a misconfigured environment, not a failing filesystem.

The runtime root is the one row that refuses, and the asymmetry is deliberate: unlike the other four it has no literal default, because it is a session-scoped, `0700`, tmpfs-backed directory a login manager creates and removes. vivarium **requires** it rather than synthesizing a replacement — a host that has none also has no user session manager to place a VM's processes in, so a synthesized directory would only buy a launch that fails an invariant later ([`../../decisions/ADR-0055-runtime-directory-is-required.md`](../../decisions/ADR-0055-runtime-directory-is-required.md)). Every run that needs it validates it (absolute, owned by the user, not group- or world-accessible) and reports the specific fault, not a generic one; the check is `runtime-dir-usable` in [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md).

## The four roots

The four **durable** roots — the durability split ADR-0005 draws. The runtime root is outside it by construction: it holds nothing that survives the session.

- **Config root** — the user's source of truth. Holds the global config file, the `images/` library, the `pieces/` library, and the `manifests/` library. Everything here is hand-authored and may be version-controlled by the user. **vivarium only reads the config root; it never writes, creates, or scaffolds anything here** (N13 in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) — config is read-only to the tool). The tool's own writes go to state, data, or cache only.
- **Data root** — pinned inputs: the per-target **lockfile** that pins what a project's build resolves to (see below), and any external module library pulled in as an input.
- **State root** — per-project runtime state the tool writes: the built VM's store output reference, a stable VM identity that survives restarts, the **project registry** (`registry.toml` — the project→manifest binding — see below), the per-project **build generations** (see below), and the diagnostic **log** (`logs/vivarium.log`, written by default — see [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)).
- **Cache root** — derived, regenerable artifacts: the **generated flake** compiled from each manifest (see below), the Nix evaluation cache, and built VM images. Safe to delete; the tool rebuilds it.

The config, data, state, and cache roots hold, respectively, what the user edits, what is pinned as input, what a run produces, and what can be rebuilt. Any new artifact is placed by asking which of those four it is — and because config is read-only to the tool, anything the tool must write is by definition state, data, or cache, never config.

## The runtime root

`$XDG_RUNTIME_DIR/vivarium/<project-id>/<target>/` holds the files that exist only while a VM is running — the per-target `flock`, the control socket, the pid file, and the boot record. Their names and the ensure-running protocol that reads them are owned by [`12-exec-and-shell.md`](./12-exec-and-shell.md); the layout mirrors the state root's `projects/<project-id>/<target>/` component for component, for the reason [`15-project-identity.md`](./15-project-identity.md) gives.

Nothing here is durable, so nothing here is ever swept: the directory is tmpfs-backed and torn down with the session that owns it. That is also why a VM does not outlive the user's final logout — the boundary is stated in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md) and decided in [`../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md`](../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md).

## Global config file

The config root holds one global config file, `config.toml`, carrying user-wide defaults. It is hand-authored and read-only to the tool; [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md) governs the annotated example a user copies from.

It carries defaults for **cross-cutting concerns** — the settings that apply to every command rather than to one project. Today that is the logging family specified in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md); each later cross-cutting knob joins the same chain. It sits one rung below the environment in the precedence standard, which is **flag > environment variable > global config file > built-in default** ([`../../decisions/ADR-0046-global-config-file-and-precedence.md`](../../decisions/ADR-0046-global-config-file-and-precedence.md), amending [`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md)).

Two things it deliberately does not carry. It does **not** hold the project→manifest binding: that binding is machine-specific, non-portable runtime state, so it lives in the state root, not here — and manifest selection keeps its own chain (below), because a per-project binding has no meaningful user-wide default. It does **not** hold a colour setting: colour follows the environment-only chain `NO_COLOR > FORCE_COLOR > isatty` with no flag and no file key ([`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md)).

## Project registry (state)

The **project registry** is the single home for project→manifest bindings: a map from a project directory to the manifest it resolves to. It lives under the **state root** because it is machine-local, tool-managed, and not portable — a record of what the tool has bound on this machine, keyed by the project's absolute path. The tool writes it only on an explicit, user-directed action (`viv init --write`, see [`01-command-surface.md`](./01-command-surface.md)), never as a side effect of a normal command.

The manifest **binding** has no file inside the repository: a project is bound by a registry entry, not by a committed or gitignored pointer. The one thing vivarium does write into a project's own tree is its self-ignored `.vivarium/` **identity** marker — which carries the `<project-id>` only, never the binding (N9, N21; [`15-project-identity.md`](./15-project-identity.md)). Identity is tracked in a separate state-root index, `identity.toml`, distinct from this `--write`-gated binding — a different key, a different write gate, and a different lifetime, which is why the two are separate files.

### On-disk shape

The registry is `registry.toml` under the state root, a TOML array of tables:

```toml
[[projects]]
path = "/home/alice/backend"
manifest = "rust-web"
```

`path` is the project directory's **canonical, symlink-resolved** absolute path — canonicalized exactly as identity resolution canonicalizes it ([`15-project-identity.md`](./15-project-identity.md)), so one directory can never acquire two bindings. `manifest` is the bare kebab-case **name**, never a resolved file path: resolution is a config-root function (below) and the config root is user-mutable behind the tool's back (N13), so a stored path would be a stale pointer for no gain.

Those two keys are a **supported interface**. `viv init` prints exactly this block for the user to paste ([`01-command-surface.md`](./01-command-surface.md)), and vivarium accepts a hand-written entry. Nothing else about the state root is specified — no other file name, and no other schema; the supported readers for the rest are `viv config --json` and `viv status -g --json`. Decided in [`../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md`](../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md).

The registry carries **no schema version**, and an unknown key fails closed. Because there is no version field to read, that failure's **message** is the whole compatibility signal — what it must say is fixed once, in [`14-exit-codes.md`](./14-exit-codes.md), and applies here and to the manifest alike. The grammar evolves additively; a genuinely breaking change would signal out of band through a new filename rather than through a field the incompatible parser must already understand. This is the manifest's rule ([`../../decisions/ADR-0047-manifest-carries-no-schema-version.md`](../../decisions/ADR-0047-manifest-carries-no-schema-version.md)) applied to the other file a user writes into.

### Writing and concurrency

Both state files are written **atomically**: serialize to a temp file in the same directory, flush it, rename it over the target, then fsync the parent directory. A concurrent reader sees the old file or the new one, never a partial one. Both files are `0600` and the state root is `0700`. The same permissions cover everything else vivarium writes under that root, the diagnostic log included ([`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)): the state root is a private directory, not a shared one.

A writer takes an exclusive `flock(2)` on a sidecar `<file>.lock`; read-only diagnostics take a shared one. Every lock vivarium takes obeys one total order — **registry → identity index → per-target `flock` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)) → Nix profile** — acquired in that order, released in reverse, and never held across a VM boot or a Nix build. A lock that cannot be taken promptly is `75` rather than a hang ([`14-exit-codes.md`](./14-exit-codes.md)). Decided in [`../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md).

### When a state file cannot be read

Absent is not corrupt, and vivarium never silently rebuilds one:

| On disk                       | Behavior                                              | Exit |
| ----------------------------- | ----------------------------------------------------- | ---- |
| absent                        | empty collection                                      | `0`  |
| zero-length                   | empty collection                                      | `0`  |
| unreadable (permissions, I/O) | fail closed                                           | `74` |
| malformed TOML                | fail closed, naming the file and the failing line     | `78` |
| unknown key                   | fail closed, naming the accepted keys and CLI version | `78` |

A malformed registry is a **configuration** defect rather than an I/O failure, because a user may have written the entry by hand; `74` is reserved for the channel genuinely failing. The two need different messages: a user who never wrote `identity.toml` cannot be told to go fix their typo. Both faults also surface ahead of the command that would hit them, as the soft `state-files-parse` check in [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md). Wholesale self-repair is never attempted — an identity entry is re-adopted one project at a time from its `.vivarium/id` marker ([`15-project-identity.md`](./15-project-identity.md)), and a lost binding is re-created by `viv init --write`.

A registry entry whose project directory has vanished is **warned about, never removed automatically**; see `viv status -g` and `viv unbind` in [`01-command-surface.md`](./01-command-surface.md) and [`../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md`](../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md).

## Per-project VM state

Each project's runtime VM state lives under the state root at `projects/<project-id>/<target>/`, where `<project-id>` is the project-identity key that scopes all of a project's state and `<target>` names the VM instance within that project — both defined in [`15-project-identity.md`](./15-project-identity.md), which also explains why `<target>` is always `default` today. This holds the project's **build generations** — a per-project Nix profile whose numbered symlinks pin retained build outputs as garbage-collector roots — and its **persistent volumes** (`volumes/<name>.img`, always including `default`). Volumes live under state, not cache, because their contents are user data and not regenerable. Generation layout and lifecycle are specified in [`11-generations-and-build-history.md`](./11-generations-and-build-history.md); the volume model in [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md) and [`../../decisions/ADR-0019-volume-model.md`](../../decisions/ADR-0019-volume-model.md).

## The generated flake and the lockfile

Building a project produces two tool-owned artifacts outside the state root, and a team may add a third the tool only ever reads. They sit in different roots because they have different durability, and the split is the whole point: one is regenerable, one is the pin that makes regeneration mean the same thing twice, and one is a team's own artifact.

```text
$XDG_CACHE_HOME/vivarium/flakes/<project-id>/<target>/     # generated flake — regenerable
$XDG_DATA_HOME/vivarium/projects/<project-id>/<target>/flake.lock   # pinned inputs — not regenerable
$XDG_CONFIG_HOME/vivarium/manifests/<name>/flake.lock      # optional team override — read-only to the tool
```

The **generated flake** is what `nix build` actually reads: the compiled manifest plus copies of the resolved image, pieces, and `extends` module ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md), [`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md)). It is regenerated wholesale, written to a temporary sibling and renamed into place so a concurrent run never reads a partial tree, and never hand-edited. `viv config` prints its path, which is how a user inspects it ([`01-command-surface.md`](./01-command-surface.md)).

The **lockfile** is data, not cache, because deleting it does not rebuild anything — it re-resolves, which is exactly what N3 forbids happening by accident. It is created by the **first build or the first `viv update`, whichever comes first** — each reports what it pinned — and thereafter moves only under `viv update` ([`../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). Creating a lock is not re-resolving one: a build may write the file that does not yet exist, but no build ever moves a pin that does. One lock per target rather than one per user: a global lock would make updating one project an unannounced update to every other.

### The team override lock

A team that wants one shared pin places a read-only `flake.lock` **beside its manifest**, at `manifests/<name>/flake.lock`. It wins over the per-target lock whenever it is present and is never written by the tool (N13) — the config root is where a team's shared guarantees already live ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). It is scoped to one manifest for the same reason the tool-owned lock is scoped to one target: a pin covering the whole config root would make adopting one team's pin an unannounced pin of every unrelated project. Because it sits inside a library directory without being a member of it, resolution never sees it and no listing enumerates it ([`../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md`](../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md)); it therefore requires the directory manifest form, which `extends` requires too.

While an override is in force, **`viv update` refuses and writes nothing** — not even the per-target lock it shadows, because a pin nothing reads today would become effective the moment the override were removed, which is precisely the unannounced input jump N3 exists to prevent. It exits `78` naming both files; moving the pin is the team's own act, outside vivarium ([`14-exit-codes.md`](./14-exit-codes.md), [`../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md`](../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)). Whichever lock is in force is the one `viv config` reports ([`01-command-surface.md`](./01-command-surface.md)) and the one a generation retains ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).

Without an override, two people building one manifest may resolve different inputs; that is the honest cost of a tool that may not write a project's own tree (N9).

## Resolution precedence

The effective manifest is resolved highest-wins, per [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md):

1. `--manifest` command-line flag — a single-invocation override, never persisted.
2. `VIVARIUM_MANIFEST` environment variable — a runtime override, never persisted.
3. Project-registry entry (in the state root) for the project's path.
4. Otherwise, **fail closed** with a copy-pasteable snippet to bind the project.

## The libraries

`images/`, `pieces/`, and `manifests/` under the config root hold the composable artifacts. Their shapes are specified in [`03-artifact-model.md`](./03-artifact-model.md). Images and pieces are Nix modules; manifests are TOML that the tool compiles, per [`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md).

A manifest names its layers by bare identifier — `image = "rust"`, `pieces = [ "git" ]` — and the tool resolves each identifier to one file in the matching library. A **name** is kebab-case: `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`. Resolution tries the flat form first and the directory form second, using the library's extension — `.nix` for `images/` and `pieces/`, `.toml` for `manifests/`:

| Library      | Tried first             | Tried second                    |
| ------------ | ----------------------- | ------------------------------- |
| `images/`    | `images/<name>.nix`     | `images/<name>/default.nix`     |
| `pieces/`    | `pieces/<name>.nix`     | `pieces/<name>/default.nix`     |
| `manifests/` | `manifests/<name>.toml` | `manifests/<name>/default.toml` |

The directory form exists so a multi-file artifact can keep its helper modules beside it; an image that imports a shared base is the motivating case ([`03-artifact-model.md`](./03-artifact-model.md)). Anything in a library directory that is not a member by these rules — a helper module, a README, an override lock, a nested name that is not kebab-case — is invisible to the readers and never enumerated. For manifests the directory form is **required** in two cases, both because the directory becomes the unit the tool copies or reads: a manifest that names `extends` ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md`](../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md)), and one that carries a team override lock. Everywhere else it stays a fallback. Both spellings of one name present is an ambiguity, not a precedence question: resolution fails closed naming both paths ([`14-exit-codes.md`](./14-exit-codes.md)). The resolved path is what `viv images list`, `viv manifest list`, and `viv manifest show` report as `path` ([`01-command-surface.md`](./01-command-surface.md)). Decided in [`../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md`](../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md).

**The config root is the whole search path.** There is no bundled library behind it and no fallback: what vivarium ships is examples to copy, which resolve nowhere until a user places them here ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md`](../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md)). So a name has exactly one meaning, a listing needs no provenance column, and an artifact is missing rather than silently satisfied from somewhere the user cannot edit.
