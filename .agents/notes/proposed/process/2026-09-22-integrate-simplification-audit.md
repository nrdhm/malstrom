# Agent Note: Integrate a simplification-audit process

Status: proposed

## Problem

Simplification findings currently depend on an ad-hoc survey. The recent whole-tree survey
([complexity-simplification-survey](../../../../docs/reviews/2026-09-22-complexity-simplification-survey.md))
found dead code (`coordinator/watchmap.rs`), half-built surface that compiles only because it is
unreachable (`malstrom-distributed` routers), and a lint allow-list that hides the classes the
gates are meant to catch. Nothing in the repository schedules that work, and nothing owns the
judgment half — deciding dead versus deliberately seamed. Left implicit, the residue returns with
the next refactor. The [fix-warning-backlog](2026-08-25-fix-warning-backlog.md) note covers the
lint gate itself; the judgment pass, its cadence, and its output do not have an owner.

## Proposal

Adopt a three-layer simplification process, each layer with one trigger:

1. **Mechanical (per PR).** Finish [fix-warning-backlog](2026-08-25-fix-warning-backlog.md)
   Step 4 so the existing `-D warnings` CI gate enforces the lint groups that expose dead or
   duplicated surface. Add one script, `scripts/find-simplifications.sh`, running deterministic
   scans (unused dependencies, duplicate crate versions, stub markers in `src/`, facade/API
   leaks) as a non-blocking-by-default CI job.
2. **Judgment (cadence).** Add a project skill at
   `.agents/skills/malstrom-find-simplifications/SKILL.md`: a survey method with explicit
   evidence rules (production vs test/docs consumers) and malstrom's protected seams (the crate
   splits and facade, the twin comm trait/impl trees, `malstrom-distributed`, k8s/kafka,
   `malstrom-testkit`). Run it after crate splits or renames and periodically — not per PR.
3. **Durable (per finding).** A proven candidate becomes a `proposed/simplification/` Agent
   Note with acceptance criteria; a batch survey produces a `docs/reviews/` note whose strong
   items graduate to notes. Notes retire through the existing
   `implemented/` → archive/delete lifecycle.

The exact tools, thresholds, and file layout are deliberately left open: the repository is
expected to change materially before this is implemented. The process *shape* is the decision;
the mechanics are settled at implementation time.

## Alternatives considered

- **Keep running one-off surveys.** What produced the current review. Lost: the residue returns
  after the next refactor, and findings stay in chat rather than notes.
- **Put the whole process in CI, including the judgment pass.** One gate for everything. Lost:
  judgment is noisy and slow, blocks PRs, and cannot distinguish a real duplicate from a
  protected seam; only the deterministic scans belong in the gate.
- **Add a dedicated workflow/tier for audits.** Rejected: the existing notes lifecycle and
  `docs/reviews/` already provide the output and retirement path; a new tier would duplicate it.
- **Adopt the upstream skill verbatim.** Rejected: its protected seams and package names are
  another repository's; malstrom's must be recorded on its own terms.

## Acceptance criteria

- `fix-warning-backlog` Step 4 is complete: no simplification-relevant lint remains `allow`ed.
- `scripts/find-simplifications.sh` runs the deterministic scans locally and in CI.
- The simplification skill exists, is discoverable by the agent harness, and is referenced from
  the root `AGENTS.md`.
- A new simplification finding is recorded as a `proposed/simplification/` Agent Note, not only
  in a review or a conversation.

## Risks

- **Process weight.** The cadence must stay a trigger, not a standing obligation; guard against
  quotas and against running the judgment pass on every PR.
- **Boundary erosion.** Without an evidence bar before a note, deliberate crate splits and
  seams get "simplified" away.
- **Tool churn.** Tool choice (`cargo-machete`, `cargo-deny`, a public-API check) and CI cost
  should be decided when implemented, against whatever the repository looks like then.