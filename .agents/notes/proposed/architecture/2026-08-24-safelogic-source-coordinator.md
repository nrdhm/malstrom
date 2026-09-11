# Agent Note: SafeLogic for the source coordinator

Status: proposed

## Problem

`SourceCoordinator` is the last raw-`Logic` operator in the source path
(`malstrom-core/src/sources/stateful.rs`): it hand-dispatches an eight-arm `Message` match
inside a `tokio::select!` on the input and comm channels. This is exactly the pattern the
review flagged as smell #7 — two operator-authoring models in the same file: `PartLister`
implemented raw `Logic` and hand-dispatched eight `Message::*` arms, while
`StatefulSourcePartitionOp` implemented `SafeLogic` ("`SafeLogic` exists precisely so operators
don't get the internal messaging invariants wrong; the part-lister opted out") — and the
redesign's "one operator model" (both the discovery operator and the reader operator implement
`SafeLogic`, no hand-rolled raw `Logic` match) promised to remove it. The collapse refactor
shipped the reader as `SafeLogicWrapper<SourcePartitionOp>` but kept the coordinator raw as "a
narrow, deliberate exception" — see
[collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md),
Alternatives. The exception is narrow but permanent unless revisited: the coordinator is the
one place a source author must understand raw `Logic` and hand-maintain the messaging
invariants the `SafeLogic` wrapper enforces everywhere else.

## Proposal

Convert `SourceCoordinator` to `SafeLogic` so the entire source path shares one authoring
model. Its three behaviors map onto `SafeLogic` (`malstrom-core/src/stream/operator_logic.rs`)
as follows:

1. **Emit-once discovery** → `on_schedule` on first call (or an explicit first-`on_epoch`
   hook): emit the discovered parts and return `true` so the scheduler loops; return `false`
   once emitted.
2. **`comm.recv()` for `PartitionFinished`** → drain in `on_schedule` via a non-blocking
   `try_recv` on `CommUtility`; the current `select!` becomes "the wrapper drives the input
   dispatch, `on_schedule` drains the comm channel". Emit `Epoch(MAX)` from the drain path
   when `listed_parts && parts.is_empty()`.
3. **Forwarding** → override `on_barrier` / `on_rescale` / `on_reconfig_complete` to forward;
   the drop decisions (`Data`, `Epoch`, `Interrogate`, `Collect`, `Acquire`) become typed
   no-ops instead of match arms.

Recommended ordering: land
[frontier-merge-source-completion](2026-08-24-frontier-merge-source-completion.md) (or the
in-band completion signal) **first** — the coordinator then loses its comm channel entirely
and becomes a pure emit-once control op, making this conversion mechanical and removing the
only part that strains the `SafeLogic` model (concurrent side-channel listening). If the comm
channel must stay, extend `SafeLogic` with an optional side-input contract (e.g. a default
`on_schedule` comm-poll) rather than keeping a raw-`Logic` island.

## Alternatives considered

- **Keep the exception (status quo)** — the coordinator is small and pure-control; the cost is
  the second authoring model and hand-maintained invariants (the eight-arm match can drift
  from the wrapper's dispatch).
- **Implement completion first, then convert (recommended)** — removing the comm channel makes
  the conversion mechanical and avoids extending `SafeLogic` for a channel that may not exist
  next quarter.
- **Extend `SafeLogic` with a side-input hook** — benefits any operator that needs auxiliary
  channels, at the cost of a wider trait and more surface for all `SafeLogic` users.

## Acceptance criteria

- No raw-`Logic` implementation remains in `malstrom-core/src/sources/` (the goal is the
  whole crate if a sweep finds no other legitimate raw operators).
- `SourceCoordinator` behaves identically: emit-once discovery, `Epoch(MAX)` exactly when the
  global part set empties, barriers/rescale/reconfig forwarded.
- Multi-worker and rescale examples pass; completion behavior is unchanged.

## Risks

- **Scheduling semantics:** `on_schedule` must be invoked often enough that `PartitionFinished`
  is not delayed indefinitely, and it must return `false` when drained or the scheduler could
  spin. Verify the wrapper's scheduler-loop contract before relying on it.
- **Invariant drift:** the whole point is to hand the invariants to the wrapper; a botched
  conversion could silently change forwarding/drop behavior for a control operator that rarely
  receives messages.
- Scope creep if `SafeLogic` is extended: every existing `SafeLogic` user is affected.
