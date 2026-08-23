# Agent Note: Remove dead reference files

Status: proposed

## Problem

The migration preserved the old implementations as dead reference files that are **not
declared as modules** anywhere, so they are not compiled but still clutter the tree and
inflate greps, dependency maps, and directory listings:

- `keyed_old/` (the pre-routing-rework distributed keyed machinery)
- `sources/stateful_old.rs`
- `coordinator/state_old.rs`
- `channels/operator_io copy.rs` (a filename with a space, not even a valid module)

## Proposal

Delete all four once the new implementations are verified against the restored test suite
(50 unit + 11 doc tests passing, multi-worker examples terminating). Verify no `mod`
declaration or `include!` references them first.

## Alternatives considered

### Why not keep them until the distributed rework is complete?
The rework (`keyed/distributed/`) is the live code; the old files already diverged from it
and their API no longer compiles against the current types. Git history preserves them.

## Acceptance criteria

- `git grep -l 'keyed_old\|stateful_old\|state_old\|operator_io copy'` (excluding
  `.git`) returns nothing.
- The full test suite still passes after deletion.

## Risks

- Deleting `keyed_old/` before the distributed routing is fully proven removes the last
  reference implementation — acceptable because it targets the pre-async API and could not
  be used as a blueprint anyway.
