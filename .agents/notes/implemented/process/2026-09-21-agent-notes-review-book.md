# Agent Note: A dev-only mdBook for reviewing Agent Notes

Status: implemented

## Problem

Agent Notes carry Mermaid diagrams and cross-links, but the notes tree is plain Markdown with
no renderer: local editors and preview tools frequently do not render the Mermaid fences or
resolve the `../../` cross-links, so reviewing a note means reading raw Markdown and mentally
following links. The published docs book (`mdbook/`) does not help — it builds from `docs/`,
and the [notes rules](../../README.md#relationship-to-the-docs-site) explicitly keep the notes
out of it.

mdBook (already the maintainer-docs toolchain, with `mdbook-mermaid`) is the obvious renderer,
but mdBook requires a `SUMMARY.md` **inside its `src` root**. Placing one in `.agents/notes/`
is not an option: [`verify-agent-notes.py`](../../../../scripts/verify-agent-notes.py)
validates every `.md` there (except `README.md`/`AGENTS.md`) as a note, and a `SUMMARY.md`
would fail that check.

## Decision

Add a **dev-only** mdBook project, `notes/`, that renders the Agent Notes locally with Mermaid
and sidebar navigation. It never publishes and never modifies the notes.

- **Generated staging `src`.** `notes/dev.sh` copies `.agents/notes/` into `notes/src/`
  (git-ignored) and generates `notes/src/SUMMARY.md` (grouped by lifecycle → class). mdBook
  builds from the copy, so the notes tree is untouched and no `SUMMARY.md` enters it. This
  keeps `verify-agent-notes.py` and the "notes are not the docs build" rule intact. The
  grouping uses a **draft chapter per class** (`- [architecture]()`) with the notes nested
  under it — mdBook only honours level-1 `#` headings as part titles, so a `## class` heading
  is silently dropped and the class grouping would not appear.
- **Escaping links.** A handful of notes link to files outside `.agents/notes/`
  (`docs/overviews/...`, `AGENTS.md`, `scripts/...`). Those cannot resolve inside the book, so
  the script rewrites them, **in the disposable copy only**, to stable GitHub blob URLs —
  and only when the target file actually exists in the repo (so illustrative examples in the
  rules README are left alone). All in-tree cross-links stay relative and work.
- **Mermaid.** `notes/book.toml` uses the same `mdbook-mermaid` preprocessor as `mdbook/`. The
  `mermaid.min.js` library is installed by `mdbook-mermaid install .` (git-ignored); the
  `mermaid-init.js` is **committed** with the same theme-aware init as `mdbook/`, so diagrams
  follow the light/dark theme (the stock init does not).
- **Usage.** `notes/dev.sh` serves with live reload on **port 3001** (the maintainer-docs
  book `mdbook/` uses 3000, so both can run at once); `notes/dev.sh build` writes `notes/book/`.
  Requires `mdbook` + `mdbook-mermaid`.
- **The rule is unchanged in substance.** `.agents/notes/README.md` gains one sentence:
  notes remain unpublished and separate from the docs site; `notes/` is a local review tool.
  The `.agents/notes` ↔ `docs`/`mdbook` split still holds, and there is still no symlink.

## Alternatives considered

- **Commit `SUMMARY.md` in `.agents/notes/`.** Simplest to build, but it is not a note and
  would fail `verify-agent-notes.py` (which would then need a special-case), and it adds a
  maintained index that drifts. Rejected.
- **A symlink `notes/src -> ../.agents/notes`.** Avoids copying, but the `SUMMARY.md` still
  has to live in the notes tree (same problem), and it re-introduces the symlink pattern the
  rules already rejected for `docs/notes`.
- **Fold the notes into the existing `mdbook/` book.** One book, one build — but mdBook has a
  single `src` root, so this needs either copies/symlinks of both `docs/` and the notes or a
  staging build; and it mixes decision records into the maintainer-docs narrative the rules
  keep separate. Rejected.
- **Rely on a Mermaid-capable editor / GitHub rendering.** Zero repo change, but not
  dependable across contributors' tools and not offline. Rejected — the point is a reliable,
  in-repo renderer.
- **A non-mdBook static generator just for the notes.** More machinery for the same result;
  `mdbook-mermaid` is already a dependency of the docs toolchain.

## Consequences

- Reviewing a note now means `notes/dev.sh` and a browser: Mermaid renders, the sidebar lists
  every note by lifecycle/class, and in-tree cross-links are clickable.
- `notes/src/` and `notes/book/` are git-ignored build products; only `notes/book.toml`,
  `notes/dev.sh`, and the `.gitignore` entries are committed.
- The notes tree stays exactly as `verify-agent-notes.py` expects; the notes rules still say
  notes are not published.
- Copy-on-build means the review book is always current with the working tree (no stale
  duplicate to maintain). The cost is a file copy per run, negligible for ~37 notes.
- The GitHub blob URLs in the rendered copy are cosmetic; the source notes keep their
  repo-relative links (which resolve in the repo/editor and on GitHub).
- Reuses the `mdbook` + `mdbook-mermaid` toolchain already required for the docs, so no new
  tool to install.

## Related

- [mdbook-rustdoc-migration](2026-09-21-mdbook-rustdoc-migration.md) — the maintainer-docs
  mdBook (`mdbook/`) this mirrors.
- [Agent Note rules — relationship to the docs site](../../README.md#relationship-to-the-docs-site)
  — the "notes are not the docs site" rule this tool must not violate.