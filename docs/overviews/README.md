# Overviews

Concise, code-linked documentation about this repository. One topic per file, numbered in
reading order. Each file carries a freshness header, stamped by `scripts/refresh-overviews.sh`
(run it after any notable change; it re-stamps only docs whose content changed, and diffs the
dependency list in `03-dependencies.md` against `malstrom-core/Cargo.toml`):

- Generic docs — `> **Last refreshed:** YYYY-MM-DD` (date only; which commit the doc reflects
  is git history's job, not the stamp's).
- Branch-scoped docs (named `NN-branch-<branch>.md`) — the same date line plus
  `> **Branch/commit:** <branch> @ <short-commit>`. The branch is read from the filename and
  its tip resolved with git, so the stamp records the branch the doc *describes*, not the
  checkout it was run from.

| # | Doc | Scope |
|---|-----|-------|
| 01 | [project.md](01-project.md) | What Malstrom is: goals, repo layout, core concepts, Kubernetes story, docs site, current state |
| 02 | [branch-new-scheduler.md](02-branch-new-scheduler.md) | Work done on the `new-scheduler` branch vs `main`: async conversion, execution architecture, distributed routing rework, current (broken) state |
| 03 | [dependencies.md](03-dependencies.md) | Every non-dev dependency of `malstrom-core`: what it's used for, in which modules, plus dead-code findings |
| 04 | [modules.md](04-modules.md) | How the `malstrom-core/src/*` modules connect: ground-up layers, mermaid dependency diagram, runtime dataflow, edge inventory |
| 05 | [architecture.md](05-architecture.md) | Distilled high-level architecture: core entities, crate layering, job anatomy, operator loop, message path (local/remote), snapshot coordination |

## Conventions

- **One topic per file** — if a doc needs a second topic, give it its own numbered file.
- **Link to code, don't duplicate it** — cite `src/...` paths and let the code be the source
  of truth; keep facts (numbers, commit ids) minimal and stamped.
- **Scope + freshness header** — every file starts with a single-line `> **Last refreshed:`
  header. Branch-scoped files are named `NN-branch-<branch>.md` and additionally carry
  `> **Branch/commit:** <branch> @ <commit>`; the branch and commit come from the filename
  and git, not from the branch the file happens to be checked out on. Generic docs carry a
  date only.
- **Branch docs are transient** — once a branch merges (or is abandoned), update its
  `NN-branch-<branch>.md` doc in place or drop it; don't accumulate dead branch histories.
  The script flags docs whose branch tip has moved past the stamp and docs whose branch no
  longer exists.
- **Update mechanically what can be mechanical** — `scripts/refresh-overviews.sh` re-stamps
  stale docs (untracked, edited, or committed after their stamp), checks the index table, and
  diffs dependencies; edit the prose by hand. It never rewrites docs whose content is
  unchanged — quiet weeks produce no churn.

## Refreshing

```bash
scripts/refresh-overviews.sh            # stamp stale docs + report (default)
scripts/refresh-overviews.sh --check    # report only; exit 1 if anything is stale
```

Run it after editing an overview (before committing — the script treats uncommitted edits as
stale). `--check` is the CI-friendly gate: it reports what `refresh` would do and exits 1 if
anything is stale or broken, without writing. Prints what it stamped, flags branch docs whose
tip moved or whose branch is gone, verifies the index table, and reports any dependency-list
drift (new deps to document, removed deps to delete). Requires GNU `sed`, `awk` and `git` on
`PATH`.

## Related

- [`../.agents/notes/`](../../.agents/notes/README.md) — RFC-style decision records and
  proposals (Agent Notes) for the codebase, organized by lifecycle and class. The
  `new-scheduler` fixes that were previously tracked as a task list now live there as
  implemented notes (compilation, runtime execution/termination, test suite) plus proposed
  follow-ups.
- [`docs/reviews/`](../reviews/README.md) — code reviews with remediation sketches
  (currently: the `sources` module).
