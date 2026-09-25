# Agent Note: Overview refresh mechanics — honest stamps, no churn

Status: implemented

## Problem

The overview docs (`docs/overviews/`) are stamped with freshness headers by
`scripts/refresh-overviews.sh`. The old script stamped **every** file with
`> **Last refreshed:** <date> (<current-branch> @ <current-commit>)` on every run. That had
three failure modes:

1. **Churn on quiet weeks** — running the script after touching one doc re-stamped all five,
   producing noise diffs on files whose content had not changed.
2. **A stamp that lied about its subject** — a generic doc (`01-project.md`) got stamped with
   whatever branch the script happened to run on (`new-scheduler @ a4c8fce`), even though its
   content describes the post-split tree that lives on other branches. The stamp recorded
   *where the refresh ran*, not *what the doc describes*.
3. **No enforcement of the "branch docs are transient" rule** — branch-scoped docs
   (`02-branch-new-scheduler.md`) were stamped with the *checkout's* branch, so a stale or
   dead branch doc was never flagged, and the refresh date drifted away from the file's real
   last content change.

## Decision

Keep the doc set's shape (one topic per numbered file, code-linked, stamped) but fix the
mechanics:

1. **Stamps are date-only for generic docs** — `> **Last refreshed:** YYYY-MM-DD`. Which
   commit a doc reflects is git history's job (`git log` on the file), not the stamp's.
2. **Branch-scoped docs are named `NN-branch-<branch>.md`** and carry a second line,
   `> **Branch/commit:** <branch> @ <commit>`. The branch is read from the **filename**, and
   its tip is resolved with git (`branch_tip`: local first, then `origin/`) — so a branch doc
   is stamped against the branch it *describes*, no matter which branch the file is checked
   out on. A doc whose branch no longer exists is reported `[dead]` (refresh it into place or
   drop it — the transience rule now has teeth).
3. **Content-based staleness** — a doc is stale when its content changed after its stamp:
   mtime for untracked/uncommitted files, last commit date for committed ones. Refresh stamps
   **only** stale docs and writes the header *on the same day it runs*, so
   `refresh` followed by `--check` passes. Untouched docs are never rewritten.
4. **`--check` mode** — report-only; exits 1 if anything is stale, dead, tip-moved, or the
   index table is out of sync. CI-friendly (no CI wiring added yet, but the gate exists).
5. **Index table check** — every numbered overview must have a row in the overviews `README`
   index, and no row may point at a deleted file; gaps are reported.
6. **Tip drift detection** — a current branch doc whose stamped commit is behind the branch
   tip is reported `[tip]` so the author re-verifies or the doc is archived.

The dependency-list diff (documented names in `03-dependencies.md` vs
`malstrom-core/Cargo.toml`) is unchanged, and stays advisory-only — it never affects the
exit code, because `03` deliberately documents where post-split deps moved and is marked
approximate.

All five overviews were migrated to the canonical layout in the same change: `01`, `03`, `04`,
`05` carry date-only stamps; `02` carries date + `new-scheduler @ a4c8fce` (its branch tip at
migration time).

## Alternatives considered

- **Keep stamping every file, but drop the branch@commit suffix** — still churns untouched
  files; the freshness date becomes meaningless because it moves on every run.
- **Stamp branch docs only when run *on* that branch** (a checkout-branch check) — creates a
  deadlock: the doc file may live on any branch while describing another, and CI would need
  to know which checkout to use. Resolving the branch from the filename + git is strictly
  more honest and needs no special checkout.
- **Fully automate freshness from git history (no manual stamps at all)** — a stamp date
  would be derivable from `git log`, but a *verification* date is not the same as a
  *content-change* date: "committed" ≠ "checked against current code". The manual date line
  remains the human's record that the doc was read against the tree; the script only enforces
  that it is at least as new as the last content change.
- **Make the dependency diff exit-failing in `--check`** — rejected: `03` documents moved
  deps by design (post-split note), so a hard failure would be permanent noise, not signal.

## Consequences

- Running the script twice in a row is a no-op; `--check` exits 0 on a fresh tree.
- Stamps now mean "last verified against the code on this date", and branch docs say exactly
  which branch/commit they describe — no more `(branch @ commit)` suffix lying about generic
  docs.
- The `[dead]` and `[tip]` reports push the "branch docs are transient" rule: a branch doc
  whose branch vanished or moved now surfaces on every run (and fails `--check`) until it is
  updated or dropped.
- Cost: an extra `--check` distinction in the script and slightly more code; the doc authors
  must keep `NN-branch-<branch>.md` naming if they want branch docs auto-stamped against the
  right branch.
- Verification: ran `refresh` (stamped `01`–`04`, all stale) then `refresh` again (no-op) and
  `--check` (exit 0). Scratch-dir tests confirmed: new untracked doc gets a header inserted;
  backdated stamps get bumped; a branch doc stamps against its filename branch's tip; a
  current doc with a moved tip reports `[tip]` and fails `--check`; missing index rows are
  reported.
