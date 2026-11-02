# KI-0003 investigation

## Symptom

`dist generate --mode ci` at cargo-dist 0.32.0 writes a `release.yml` that `dist plan` accepts and that both workflow linters reject. Measured 2026-11-02 on the unmodified output, with zizmor 1.25.2 and actionlint 1.7.12:

- `zizmor --no-progress --offline`: 18 findings, 7 suppressed, 4 informational, 0 low, 3 medium, 4 high.
- `actionlint`: SC2086 at lines 124 and 283 (twice), and SC2129 at lines 152 and 196.

## Root cause

The template, `cargo-dist/templates/ci/github/release.yml.j2`, writes each expression into the `run:` body rather than through `env:`:

| Severity | Audit                 | Where                                                                                                |
| -------- | --------------------- | ---------------------------------------------------------------------------------------------------- |
| High     | template-injection    | the `plan` step expands `github.ref_name`, the pushed tag, into `dist host --steps=create --tag=...` |
| Medium   | template-injection    | `run: ${{ matrix.install_dist.run }}` in `build-local-artifacts`                                     |
| Medium   | template-injection    | `${{ matrix.packages_install }}` in `build-local-artifacts`                                          |
| Medium   | template-injection    | `${{ matrix.dist_args }}` and `${{ needs.plan.outputs.tag-flag }}` in the `Build artifacts` step     |
| High     | unpinned-images       | `container: ${{ matrix.container && matrix.container.image \|\| null }}`, two findings               |
| High     | excessive-permissions | `contents: write` at the workflow level                                                              |

The shellcheck findings come from the same template: `$GITHUB_PATH`, `$RUNNER_TEMP/notes.txt`, and `$PRERELEASE_FLAG` are unquoted, and `$GITHUB_OUTPUT` takes one redirect per line. At the tag `v0.34.0` the template still holds the `plan` step expansion, the container image, `matrix.install_dist.run`, `packages_install`, and `dist_args`.

## The mask

vivarium follows cargo-dist natively: the generated file is never edited, and no check rewrites it. Two parts make that possible while the template fails:

- zizmor runs nowhere: no hook, no devshell package, and no `.github/zizmor.yml`.
- actionlint keeps every workflow, and `.github/actionlint.yaml` accepts SC2086 and SC2129 for `release.yml` alone. Every other rule still judges it.

## Recheck condition

Recheck on each cargo-dist release, as [`../../tracking.yaml`](../../tracking.yaml) schedules. In a scratch copy, set `cargo-dist-version` to the newest release, run `dist generate --mode ci`, then run `zizmor --no-progress --offline .github/workflows/release.yml` and `actionlint` without the `release.yml` entry of `.github/actionlint.yaml`.

The case resolves when both pass. Then bump `cargo-dist-version`, commit the generated `release.yml`, restore the zizmor hook and the devshell's zizmor, delete the `release.yml` entry of `.github/actionlint.yaml`, and collapse this dossier under `resolved/`.
