# slides

The vivarium presentation deck, built with [Slidev](https://sli.dev/) and published to GitHub Pages.

This directory is a self-contained npm sub-project. Nix owns the Node runtime through the repository devShell; npm owns Slidev itself, pinned by the committed `package-lock.json`. [`ADR-0103`](../docs/decisions/ADR-0103-the-published-deck-is-its-own-presentation-tree.md) records why the deck lives outside `docs/` and why the repository's Markdown regime does not apply here.

## Run it

```bash
just slides-install   # once, and again whenever package.json moves
just slides-dev       # http://localhost:3030, hot-reloads on save
just slides-build     # production build into slides/dist
```

## Layout

| Path | Purpose |
| --- | --- |
| `slides.md` | The deck. Slidev per-slide front matter, slides separated by `---`. |
| `style.css` | Deck-wide theme tokens and layout overrides. |
| `public/` | Static assets served at the deck root. Create when something needs one. |
| `dist/` | Build output. Gitignored. |

## House rules

No `v-click` or other click-reveal directives. The deck is published as a page people scroll as often as a talk someone presents, and content gated behind a click is invisible to the first reader.

No emphasis-by-syntax where a token will do. `slides/` is exempt from the repository's no-bold rule so a slide can carry a beat, but `style.css` is the better tool for anything structural.

Before adding an icon, install the `@iconify-json` set it comes from and name it in `package.json`. An icon from a set that is not installed fails the build with `Icon <set>/<name> not found`, and a written-down list of "available" sets drifts from the installed one the moment either moves.

## Deployment

Pushing to `develop` with changes under `slides/` deploys the deck to <https://gubasso.github.io/vivarium/> through [`.github/workflows/pages.yml`](../.github/workflows/pages.yml). [`../docs/guides/author-and-publish-the-deck.md`](../docs/guides/author-and-publish-the-deck.md) is the full runbook, including the one-time repository setting the first deploy needs.
