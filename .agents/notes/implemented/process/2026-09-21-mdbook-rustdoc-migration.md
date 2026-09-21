# Agent Note: Maintainer docs as mdBook + rustdoc

Status: implemented

## Problem

Malstrom maintained **two** documentation surfaces with two different toolchains:

- `website/` — the user-facing site (VitePress/npm), deployed to GitHub Pages.
- `docs/` — the maintainer surface: `overviews/` (7 numbered, code-linked architecture docs)
  and `reviews/`. Built with **MkDocs + Material for MkDocs**, pinned in `pyproject.toml` as a
  virtual `uv` project (`mkdocs 1.6.1`, `mkdocs-materialx 10.1.8`, `mkdocs-mermaid2-plugin
  1.2.3`, `properdocs 1.6.7`) and served by `docs/docs-dev.sh`.

Two costs motivated the change:

1. **A second language toolchain for one surface.** A pinned Python `uv` environment (and a
   MkDocs *fork*, `properdocs`) rendered ~1,100 lines of Markdown for a Rust project.
2. **No Rust API documentation.** `cargo doc` worked but emitted **12 rustdoc warnings** and
   nothing published or linked the API reference. The overviews described the architecture in
   prose while the public extension surface was only in the source.

## Decision

The `docs/` surface is built with **mdBook (prose) + rustdoc (API reference)**, both
Rust-native. Content and the `overviews/` conventions are unchanged; only the toolchain moved.

- **mdBook.** The book project lives in **`mdbook/`** (matching the convention of other Rust
  projects): `mdbook/book.toml` sets `src = "../docs"` (so the existing Markdown and its
  internal links stay in `docs/`) and `[build] build-dir = "book"` (output inside the project
  directory, keeping the repo root clean). `docs/SUMMARY.md` is the book's table of contents
  (introduction, overviews, reviews).
- **Mermaid.** `[preprocessor.mermaid] command = "mdbook-mermaid"` plus
  `additional-js = ["mermaid.min.js", "mermaid-init.js"]`. The two asset files are produced by
  `mdbook-mermaid install .` run **inside `mdbook/`** — mdBook resolves `additional-js`
  relative to the `book.toml` directory. They are git-ignored and regenerated in CI (and on
  first `mdbook/docs-dev.sh` run). All **7** diagrams (`04-modules.md`: 2,
  `05-architecture.md`: 5) render.
- **Link rewrites.** mdBook serves `src/` as the book root and does not copy files outside it,
  so the 12 links that escaped `docs/` (`../../malstrom-core/src/...`, `../../.agents/notes/...`)
  were rewritten to stable GitHub blob URLs
  (`https://github.com/MalstromDevelopers/malstrom/blob/main/<path>`). Links *within* `docs/`
  (`../reviews/...`, `../overviews/...`) stay relative and resolve normally.
- **rustdoc.** The **12 rustdoc warnings** were fixed (unresolved intra-doc links in
  `alignment.rs`, `operator_io.rs`, `coordinator/api.rs`, `runtime/threaded/mod.rs`,
  `stream/operator.rs`, `stream/operator_logic.rs`, `types/mod.rs`, `spsc.rs`;
  `invalid-html-tags` in `vec_sink.rs`; private-link and bare-URL cases in
  `malstrom-operators`/`malstrom-testkit`/`malstrom-macros`). `RUSTDOCFLAGS="-D warnings"
  cargo doc --workspace --no-deps` now passes and is a CI gate.
- **Toolchain removed.** `pyproject.toml`, `uv.lock`, `mkdocs.yml`,
  `docs/assets/palette-toggle-reload.js`, and the `properdocs`/MaterialX/mermaid2 pins are
  deleted. The dev script moved to `mdbook/docs-dev.sh` and runs `mdbook serve` from `mdbook/`.
- **CI.** `.github/workflows/ci.yaml` gained a `Rustdoc` step (in `check`) and a `docs` job
  that installs mdBook + `mdbook-mermaid` and runs `mdbook-mermaid install .` + `mdbook build`
  with `working-directory: mdbook`.

## Alternatives considered

- **Keep MkDocs, do nothing** — zero churn, and MkDocs-Material is best-practice. Lost: the
  second toolchain and the API-reference gap.
- **Consolidate onto the user-facing VitePress site** — one toolchain, Mermaid already present.
  Rejected: the Agent Note rules keep the two surfaces deliberately separate, and rustdoc output
  does not belong in a hand-authored VitePress build.
- **Sphinx + MyST** — stronger reference/API story, but swaps one Python tool for a heavier one
  and does not remove the second-language cost.
- **rustdoc only** — the API reference is the missing piece, but the prose overviews/reviews are
  not API docs and would lose book structure, diagrams, and the freshness mechanics.
- **Symlink the repo roots into `docs/` so mdBook copies them** — validated in a spike (mdBook
  does follow symlinks under `src`), but it pollutes the docs tree with repo-root symlinks and
  copies source into the book output. Rejected in favour of the URL rewrite.
- **Zola / Cobalt (Rust SSGs)** — single-binary, fast, but site-oriented: no sidebar, search, or
  book structure out of the box.
- **Publish to `docs.rs`** — the standard home for crate API docs and a good *eventual* target
  once crates are published; it complements mdBook rather than replacing it.

## Consequences

- The `docs/` surface builds with `mdbook build` (Rust-native) and is reproducible in CI; the
  Python `uv` environment is gone.
- The Typora-friendly prose is unchanged, but **12 links now point at GitHub blob URLs** — they
  open the repository rather than local files, which is the price of mdBook's `src`-rooted
  layout.
- **Two Rust build tools must be installed** (`mdbook`, `mdbook-mermaid`) and pinned; CI
  installs them via `cargo install --locked`. `mdbook-mermaid` warns that it was built against
  mdBook 0.5.0 while the toolchain is 0.5.4 — harmless, but a version pin to watch.
- **Generated assets are git-ignored** (`mdbook/mermaid.min.js`, `mdbook/mermaid-init.js`,
  `mdbook/book/`); a local build must run `mdbook-mermaid install .` in `mdbook/` once (or use
  `mdbook/docs-dev.sh`, which does it on first run).
- **rustdoc is now gated** (`-D warnings`), so new public items must have resolving intra-doc
  links; three private-link/bare-URL cases were de-linked rather than exposed.
- `scripts/refresh-overviews.sh` still operates on `docs/overviews/*` paths and the overviews
  README index, so its stamping, index, and dependency-drift checks keep working.
- The repo still has three documentation surfaces (`website/` VitePress, mdBook, rustdoc). This
  removed one *toolchain* (Python); it did not reduce the number of *surfaces*.

## Related

- [overview-refresh-mechanics](../../implemented/process/2026-09-03-overview-refresh-mechanics.md)
  owns the `overviews/` conventions, stamps, and `scripts/refresh-overviews.sh`.
- [website-mermaid-diagrams](../../implemented/process/2026-09-21-website-mermaid-diagrams.md)
  owns the user-facing VitePress site and its Mermaid setup.
- [Agent Note rules — relationship to the docs site](../../README.md#relationship-to-the-docs-site)
  is the "keep the two surfaces separate" rule that shaped the consolidation decision.