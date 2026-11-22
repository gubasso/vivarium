# KI-0006 — the spectrum git host does not answer, and a cold fetch of it hangs

- Status: `mitigated`
- Severity: medium. Every CI lane that ran `viv` hung on a cold runner, and a fetch that hangs fails no check by itself.
- External system: the git host `spectrum-os.org`. The [microvm.nix](https://github.com/microvm-nix/microvm.nix) flake declares `spectrum = { url = "git+https://spectrum-os.org/git/spectrum"; flake = false; }`, so [`nix/flake.lock`](../../../../nix/flake.lock) locks it as a `git` input.
- Affected checks: the `test-local` and `test-eval` lanes on a GitHub runner, first seen 2026-11-20.
- Guard: the lanes fetch only the two baseline inputs they pin, under a deadline, and fail with the reason when the deadline passes.

The host accepts no TCP connection over IPv4 or IPv6. A git fetch has no timeout of its own, so a fetch of the `spectrum` input blocks until something kills it. A warm store hides the problem, because Nix never fetches a locked input that the store already holds.

- [`issue.yaml`](./issue.yaml) supplies the machine-readable case metadata.
- [`investigation.md`](./investigation.md) owns the evidence, the guard, and the recheck condition.

No upstream report exists. The fault is an unreachable host, not a defect that a tracker can take.
