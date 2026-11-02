# KI-0003 upstream escalation

[axodotdev/cargo-dist#2320](https://github.com/axodotdev/cargo-dist/issues/2320), "Good practices for mitigating script injection attacks", asks the template to pass each expression through `env:`. vivarium added its findings in [a comment on 2026-11-02](https://github.com/axodotdev/cargo-dist/issues/2320#issuecomment-6590035957): the zizmor table of the unmodified 0.32.0 output, the unpinned container image, and the workflow-level `contents: write`.

## Upstream state

On 2026-11-02 the issue is open, and the comment is its first. cargo-dist 0.34.0, released 2026-11-01, does not change the reported lines.
