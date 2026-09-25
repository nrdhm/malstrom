# Agent Note: Render Mermaid diagrams on the website

Status: implemented

## Problem

The website (`website/`, VitePress, deployed by `.github/workflows/pages.yaml`) had **no
Mermaid renderer**: `website/.vitepress/config.mts` registered only `markdown-it-footnote`, and
`website/package.json` carried no Mermaid dependency. Any ` ```mermaid ` block on the site
rendered as a literal code fence.

The repository already relies on Mermaid diagrams, so this was a gap, not a hypothetical:

- `docs/overviews/04-modules.md` has the crate dependency diagram and a runtime dataflow
  diagram; `docs/overviews/05-architecture.md` has five more. These rendered on the **MkDocs
  `docs/` site** at the time (its `mkdocs.yml` configured the `mermaid2` plugin with a
  dark/light theme expression wired to MaterialX's palette); that surface has since moved to
  mdBook + `mdbook-mermaid` — see
  [mdbook-rustdoc-migration](2026-09-21-mdbook-rustdoc-migration.md).
- `Status.md` recorded the TODO: *"wire the internals section into the website? want to have a
  mermaid diagram with StartBuild protocol visualized"*.
- `website/internals/StartBuild.md` was a one-line stub, and the `internals/` pages were not in
  the VitePress sidebar.

The internals material existed as prose + Mermaid on the `docs/` side, but the user-facing
website could not show it, and the two doc builds disagreed on whether Mermaid existed at all.

## Decision

The VitePress site renders Mermaid via `vitepress-plugin-mermaid`, and the internals section is
live.

- **Plugin** — `vitepress-plugin-mermaid` (wrapping the config with `withMermaid`) renders
  ` ```mermaid ` fences and follows VitePress dark mode. `mermaid` is declared alongside it in
  `website/package.json`. The plugin lists `mermaid` as a **peer dependency** and imports it
  from its own install location, so the config adds a Vite `resolve.alias` mapping `mermaid`
  to this project's local copy (`node_modules/mermaid/dist/mermaid.esm.min.mjs`) — without it
  the bundle fails to resolve the peer.
- **Package manager** — the Pages workflow moved from **bun to npm** (`npm ci` +
  `npm run docs:build`, Node 20). `bun install` resolves the plugin and VitePress through its
  global cache, where their peer dependencies (`mermaid`, `vitepress`→`vite`) are not visible,
  and the site failed to build; npm's flat `node_modules` resolves them. `package-lock.json` is
  committed; `bun.lockb` is deleted.
- **StartBuild page** — `website/internals/StartBuild.md` now documents the coordinator↔worker
  handshake (`StartBuild` / `StartExecution` / `RuntimeMessage`) with a sequence diagram, drawn
  from the actual code (`malstrom-core/src/worker/worker.rs`,
  `malstrom-core/src/coordinator/cluster.rs`, `.../coordinator/messages.rs`).
- **Sidebar** — an "Internals" group in `themeConfig.sidebar` exposes `internals/KvtTrait` and
  `internals/StartBuild`.
- **Sources stay separate** — the architecture diagrams in `docs/overviews/*` remain on the
  MkDocs surface; nothing from `docs/` is symlinked or copied into `website/`. Website diagrams
  are authored in `website/` pages.

## Alternatives considered

- **`markdown-it-mermaid`** — smaller, no VitePress-specific coupling, but it only renders the
  fence and leaves dark-mode theming and Mermaid client initialisation to us, duplicating what
  MkDocs gets from `mermaid2`. Reasonable fallback, not chosen.
- **Pre-render diagrams to SVG/PNG and commit images** — no client-side JS and works anywhere,
  but diagrams go stale silently and are painful to edit; the `docs/` side already treats
  Mermaid as source. Rejected.
- **Reuse the MkDocs `docs/` site for everything, including internals** — one Mermaid setup, no
  VitePress change. Rejected: `docs/` is the maintainer-facing surface (deliberately separate
  per the [Agent Note rules](../../README.md#relationship-to-the-docs-site)); the website is
  user-facing and the StartBuild TODO is explicitly about the website.
- **Symlink `docs/overviews` into `website/`** — reuse the existing diagrams with no
  duplication. Rejected: violates the documented "no symlink between the two surfaces" rule,
  and mixes internal maintainer docs into the public nav.
- **Keep bun and work around the peer resolution (aliases for every peer)** — brittle; every
  future plugin peer would need its own alias. Rejected in favor of the npm switch.
- **Do nothing / leave diagrams as code blocks** — the internals page could not deliver its
  purpose and the two sites stayed inconsistent. Rejected.

## Consequences

- ` ```mermaid ` blocks in `website/` render as diagrams and follow VitePress light/dark mode.
  Verified with `npm ci && npm run docs:build` (the StartBuild sequence diagram and its encoded
  `graph=` attribute are present in the built page bundle).
- The website and the maintainer book each pin a Mermaid version; shared diagrams must stay in
  a conservative syntax subset that both render. The book (mdBook + `mdbook-mermaid`) is
  covered by [mdbook-rustdoc-migration](2026-09-21-mdbook-rustdoc-migration.md); VitePress's
  dark mode toggles differently from each, so its wiring is the likely source of future
  rendering bugs.
- **Bundle weight** — Mermaid is a sizeable client dependency; the build warns about a >500 kB
  chunk. Lazy per-page init is a possible follow-up if it becomes a problem.
- **Toolchain change** — the website now builds with npm, not bun; the Pages workflow and
  lockfile were updated together. `.gitignore` still ignores `website/node_modules/` and the
  VitePress `cache`/`dist` directories.
- `website/internals/StartBuild.md` is no longer a stub, and the `internals/` pages are
  reachable from the sidebar.

## Related

- [Agent Note rules — relationship to the docs site](../../README.md#relationship-to-the-docs-site)
  is the "keep the surfaces separate" rule the source-of-truth decision follows.
- [overview-refresh-mechanics](../../implemented/process/2026-09-03-overview-refresh-mechanics.md)
  owns the `docs/overviews/` docs the architecture diagrams live in.
- [add-ci-checks](../../implemented/process/2026-08-25-add-ci-checks.md) notes `pages.yaml`
  deploys the docs site; this note changes how it builds.