# Overviews

Concise, code-linked documentation about this repository. One topic per file, numbered in
reading order. Each file carries a `> **Last refreshed:** <date> (<branch> @ <commit>)`
header — stamped by `scripts/refresh-overviews.sh` (run it after any notable change; it
updates the stamps and diffs the dependency list in `03-dependencies.md` against
`malstrom-core/Cargo.toml`).

| # | Doc | Scope |
|---|-----|-------|
| 01 | [project.md](01-project.md) | What Malstrom is: goals, repo layout, core concepts, Kubernetes story, docs site, current state |
| 02 | [branch-new-scheduler.md](02-branch-new-scheduler.md) | Work done on the `new-scheduler` branch vs `main`: async conversion, execution architecture, distributed routing rework, current (broken) state |
| 03 | [dependencies.md](03-dependencies.md) | Every non-dev dependency of `malstrom-core`: what it's used for, in which modules, plus dead-code findings |
| 04 | [modules.md](04-modules.md) | How the `malstrom-core/src/*` modules connect: ground-up layers, mermaid dependency diagram, runtime dataflow, edge inventory |

## Conventions

- **One topic per file** — if a doc needs a second topic, give it its own numbered file.
- **Link to code, don't duplicate it** — cite `src/...` paths and let the code be the source
  of truth; keep facts (numbers, commit ids) minimal and stamped.
- **Scope + freshness header** — every file starts with `> **Last refreshed:** …`; add
  `> **Branch/commit:** …` when the content is branch-specific.
- **Branch docs are transient** — once a branch merges (or is abandoned), update
  `02-*.md` in place or drop it; don't accumulate dead branch histories.
- **Update mechanically what can be mechanical** — `scripts/refresh-overviews.sh` handles
  stamps and the dependency diff; edit the prose by hand.

## Refreshing

```bash
scripts/refresh-overviews.sh
```

Prints what it stamped and any dependency-list drift (new deps to document, removed deps to
delete). Requires GNU `sed` and `git` on `PATH`.

## Related

- [`../.agents/notes/`](../../.agents/notes/README.md) — RFC-style decision records and
  proposals (Agent Notes) for the codebase, organized by lifecycle and class. The
  `new-scheduler` fixes that were previously tracked as a task list now live there as
  implemented notes (compilation, runtime execution/termination, test suite) plus proposed
  follow-ups.
- [`docs/reviews/`](../reviews/README.md) — code reviews with remediation sketches
  (currently: the `sources` module).
