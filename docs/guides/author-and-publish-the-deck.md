# Author and publish the deck

The vivarium presentation deck lives in [`../../slides/`](../../slides/README.md) and is published to <https://gubasso.github.io/vivarium/>. This is the runbook for changing it and getting the change live. [`../decisions/ADR-0104-the-published-deck-is-its-own-presentation-tree.md`](../decisions/ADR-0104-the-published-deck-is-its-own-presentation-tree.md) owns why the deck sits outside the documentation zones and why the repository's Markdown rules do not reach it.

## Before the first deploy

Someone with repository admin has to set Settings, then Pages, then Build and deployment, then Source, to `GitHub Actions`. No workflow can set this, and the first deploy fails without it with an error that names permissions rather than the setting.

## Author

```bash
just slides-install   # once, and again whenever slides/package.json moves
just slides-dev       # serves http://localhost:3030 and hot-reloads on save
```

Edit [`../../slides/slides.md`](../../slides/slides.md) for content and [`../../slides/style.css`](../../slides/style.css) for appearance. A new slide is a `---` followed by markdown; per-slide front matter after that `---` sets the layout. Speaker notes are an HTML comment at the end of a slide. Slidev's own reference covers the syntax: <https://sli.dev/guide/syntax>.

Two house rules the deck keeps, both recorded in [`../../slides/README.md`](../../slides/README.md): no click-reveal directives, because the deck is read as a page at least as often as it is presented, and no icon from an `@iconify-json` set that is not installed, because the build fails on it by name.

## Check the production build

```bash
just slides-build          # builds into slides/dist under the /vivarium/ base
npx serve slides/dist      # preview the built bundle
```

The base path matters only for the deployed bundle. `just slides-dev` serves from the root and needs no base; `just slides-build` carries `/vivarium/` as a literal, and the workflow derives the same value from the repository name so a rename cannot desynchronise them.

The build is also the deck's only automated gate. Every Markdown hook is excluded from `slides/`, so a `slidev-build` hook runs at the pre-push stage for pushes that touch the directory, and [`../../.github/workflows/slides.yml`](../../.github/workflows/slides.yml) repeats it for every pull request and every push to `master`. That lane is separate from [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml), which carries lint, test, build, hooks, and the release gate. The deck lane is not in that gate, so a red deck build reports and holds no merge.

## Publish

Deployment is a push to `master` that touches `slides/`. There is no separate release step. `master` is the one long-lived branch and takes no direct push, so the deck reaches it the same way every other change does: a linked worktree, then a squash-merged pull request.

```bash
rk worktree add docs/<topic> --apply
cd ../vivarium@docs-<topic>
# edit, commit
git push -u origin docs/<topic>
gh pr create --base master
```

Merging the pull request triggers [`../../.github/workflows/pages.yml`](../../.github/workflows/pages.yml). Watch it under Actions, `pages`; the live URL appears in the deploy job's summary.

## Republish without a change

```bash
gh workflow run pages.yml --ref master
```

Use this after changing a repository-level Pages setting, when nothing in the tree has moved.

## Troubleshooting

| Symptom                                                            | Cause                                                                                                                                                                                                 |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Deck builds locally, blank page on Pages with 404s for every asset | The bundle was built without the right base. Reproduce with `just slides-build` and check `slides/dist/index.html` references `/vivarium/assets/...`.                                                 |
| A change merged to `master` but nothing deployed                   | The workflow's path filter covers `slides/**` and the workflow file only. Force it with `gh workflow run`.                                                                                            |
| A direct link to a specific slide 404s                             | Slidev's default hash router is what makes deep links work on Pages. GitHub Pages has no single-page-app rewrite, so `routerMode: history` would need a `404.html` fallback the deploy does not ship. |
| `slidev-build` at push says `slides/node_modules is missing`       | Exactly that. Run `just slides-install`. The hook names it rather than letting npm report a missing script, because the two failures look alike and mean different things.                            |
| A pull request's `slides` job fails while local builds pass        | The job installs with `npm ci` from `slides/package-lock.json`. A dependency added with `npm install` and not committed reproduces exactly this.                                                      |
