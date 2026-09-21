# Agent Note: Remove dead reference files

Status: implemented

## Problem

The migration preserved the old implementations as dead reference files that are **not
declared as modules** anywhere, so they are not compiled but still clutter the tree and
inflate greps, dependency maps, and directory listings:

- `keyed_old/` (the pre-routing-rework distributed keyed machinery)
- `sources/stateful_old.rs`
- `coordinator/state_old.rs`
- `channels/operator_io copy.rs` (a filename with a space, not even a valid module)

## Decision

All four files were deleted in the `collapse-source-traits` change (commit `0f0cc19`,
2026-08-23), once the new implementations were verified against the restored test suite.
A fifth undeclared module — `testing/iterator_source.rs`, which referenced the removed
`SingleIteratorSource`/`IntoSource` API — was deleted in the same change; its `emits_values`
test logic lives on in `sources/fn_source.rs`.

Before deleting, `git grep` confirmed no `mod` declaration or `include!` referenced any of
them. The overview docs that described them as current tree state were updated in the same
pass; the historical review/redesign docs still mention them by design (that folder is a
frozen source of truth and is not edited).

## Alternatives considered

### Why not keep them until the distributed rework is complete?
The rework (`keyed/distributed/`) is the live code; the old files already diverged from it
and their API no longer compiles against the current types. Git history preserves them.

## Consequences

- The acceptance criteria hold: `git grep -l 'keyed_old\|stateful_old\|state_old\|operator_io copy'`
  matches only prose in documentation and notes (historical overviews and this note), never code.
- The full test suite still passes after deletion (50 unit + 10 doc tests).
- Directory listings, greps, and the dependency/module maps no longer include files that
  were never compiled.
- Git history retains the deleted implementations; if the distributed routing rework ever
  needs a pre-async reference, it lives in history rather than the tree.
