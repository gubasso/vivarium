# KI-0003 — cargo-dist's generated release workflow fails zizmor

- Status: `masked`
- Severity: medium — the release workflow runs with the forge's secrets, and its template expands tag names and matrix values straight into shell commands.
- External system: cargo-dist 0.34.0, the newest release, pinned by `cargo-dist-version` in [`dist-workspace.toml`](../../../../dist-workspace.toml). 0.32.0 had the same findings.
- Affected checks: the `zizmor` hook, and `actionlint` on [`release.yml`](../../../../.github/workflows/release.yml).
- Mask: active. zizmor is out of [`.pre-commit-config.yaml`](../../../../.pre-commit-config.yaml) and of the devshell, and [`.github/actionlint.yaml`](../../../../.github/actionlint.yaml) accepts SC2086 and SC2129 for `release.yml` alone. Revert both when the recheck condition holds.

`release.yml` is generated: the repository holds it exactly as `dist generate` writes it, and the `dist-plan` job fails when the two drift. The generated form fails zizmor with 4 high and 3 medium findings and fails actionlint with five shellcheck findings. A hand-hardened copy would stop cargo-dist's own upgrades from reaching the file, so the checks give way until the template is fixed.

- [`issue.yaml`](./issue.yaml) — supplies the machine-readable case metadata.
- [`investigation.md`](./investigation.md) — owns the findings, the mask, and the recheck condition.
- [`escalation.md`](./escalation.md) — records the upstream report and its state.
