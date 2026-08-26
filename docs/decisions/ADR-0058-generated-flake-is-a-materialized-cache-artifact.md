# ADR-0058: The generated flake is a materialized cache artifact

## Context and Problem Statement

[`ADR-0004-toml-manifest-compiles-to-flake.md`](./ADR-0004-toml-manifest-compiles-to-flake.md) decided that a manifest compiles to a generated flake, and its own Consequences flag the hole: the flake "is an intermediate artifact users may need to inspect when debugging" — but nothing says where it lives or how the resolved modules reach it.

## Considered Options

- Generate a thin flake that takes the config root as a `path:` input.
- Materialize the resolved modules into a generated tree under the cache root.
- Generate into the project's own tree beside the workspace.

## Decision Outcome

Chosen option: materialize into the cache root, at `flakes/<project-id>/<target>/`.

- Cache, because the tree is derived and regenerable — the rule `spec/02` already states. State is wrong (`viv destroy` clears it); the project tree (N9) and config (N13) are forbidden.
- Materialize rather than reference. A local path input re-locks on evaluation even when its content is unchanged, making the lockfile a moving target under N3. Worse, referencing the config root makes every image, piece, and other manifest a build input — so an unrelated edit changes the output path, the freshness key (N4), and every project rebuilds.
- `images/` and `pieces/` are copied wholesale; `manifests/` is excluded. A library member may keep helper modules beside it (ADR-0045), so a per-file copy would break an image that imports its own base.
- The tree is written to a temporary sibling and renamed, so no concurrent run reads a half-written flake.
- Inspection needs no new verb: `viv config` prints the path. `--keep-generated` stays retired (ADR-0026).

## Consequences

- Good: the artifact ADR-0004 named is locatable, stable, and safe to delete.
- Good: an unrelated library edit no longer invalidates every project's build.
- Bad: copying over-invalidates — touching any image or piece regenerates the tree and costs one re-evaluation. Narrowing to the import closure is a later optimization, not a correctness fix.
- Bad: the manifest text is copied into the store, launch-channel tables included — N19 bounds what a build output may depend on, not what the store holds. So a secret in a manifest reaches the world-readable store, which N10 forbids; the invariant is unchanged, its reach now explicit.

## Status

Accepted

Amended by [`ADR-0063-extends-requires-the-directory-manifest-form.md`](./ADR-0063-extends-requires-the-directory-manifest-form.md) — `manifests/` is still never copied wholesale, but the selected manifest's own directory is materialized when that manifest names `extends`, which is why the directory form is required in that one case.

Amended by [`ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md`](./ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md) — the generated flake gains an `inputs` set, unioned from the input declarations the copied images and pieces carry. Those declarations are read from the config root at compile time; the copies that land in the store are never re-read.

Amends [`ADR-0004-toml-manifest-compiles-to-flake.md`](./ADR-0004-toml-manifest-compiles-to-flake.md) — the generated flake's location, contents, and inspection path are fixed here; the compile-to-a-flake decision is unchanged.

Amends [`ADR-0010-secrets-never-in-nix-store.md`](./ADR-0010-secrets-never-in-nix-store.md) — the prohibition now demonstrably reaches the manifest, because compiling it into the flake copies its text into the store. Stated in [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).

Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).

Amended by [`ADR-0112-the-selected-image-carries-the-base-flake.md`](./ADR-0112-the-selected-image-carries-the-base-flake.md) — the generated flake gains one flake input, `path:./images/<name>`, pointing into its own materialized `images/` copy when the selected image carries a base flake. Materialize-not-reference survives: the node is relative and hashless, so the lock is not the moving target the rejected absolute form was, and the tree stays self-contained and regenerable.
