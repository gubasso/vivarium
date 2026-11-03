<!-- BEGIN release-kit -->

## Workflow terms

A term below names the actions the operator authorizes by using it in a request. A request that carries no term authorizes the file changes alone.

| Term                    | What the operator authorizes by using it                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `implement-and-request` | The file changes, then the branch and its worktree, the commits, and the integration up to the point a human decides. It stops before the merge. |
| `implement-and-merge`   | Everything `implement-and-request` names, then the integration itself, then pruning the branch and its worktree. It stops before the release.    |
| `full-implement`        | Everything `implement-and-merge` names, then the release, operated through `rk method operate` to a published version.                           |

Each term's integration step resolves through the recorded integration mode: under `forge` it is the push and the pull or merge request, and under `local` it is `rk integrate` and the separate trunk push. A request naming `--forge` or `--local` selects the other authority for that one execution. `rk method integration` states both paths.

Write this project's own terms below the end marker. Release-kit owns the lines between the markers and rewrites them at every upgrade.

<!-- END release-kit -->
