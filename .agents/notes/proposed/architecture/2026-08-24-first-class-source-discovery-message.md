# Agent Note: First-class source discovery message

Status: proposed

## Problem

Part discovery still rides the data plane. After the source-trait collapse, `SourceCoordinator`
(worker 0, raw `Logic`) emits each discovered partition as
`DataMessage::new(part.clone(), NoData, SrcImpl::Timestamp::MIN)` through a
`.distribute(rendezvous_select)` step (`malstrom-core/src/sources/stateful.rs`); the redesign's
first-class `Message::SourcePartitions(Vec<PartitionKey>)` control variant was explicitly
deferred — see [collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md),
Decision 3, and the [redesign](../../../../docs/reviews/sources-module-redesign.md), §3.

The cost is observability: a *control* fact — "these partitions exist" — is indistinguishable
from *data* on the wire. Tracing, metrics, and any job-inspection surface must know the
`NoData`/`Timestamp::MIN` sentinel convention to tell a discovery record from a user record,
and data-plane record counters must filter it out. There is no way to ask "what partitions did
this source discover?" other than sniffing the data stream — even though the coordinator
already holds the full set (`parts: IndexSet`) in memory.

## Proposal

Make discovery a first-class control message, keeping the current graph shape and assignment
logic:

1. Add a control variant to `Message` in `malstrom-core/src/types/message.rs` — e.g.
   `SourcePartitions(Vec<PartitionKey>)` — alongside the existing control variants (`Rescale`,
   `ReconfigComplete`, `Interrogate`, `Collect`, `Acquire`). It carries the full discovered set
   in one message instead of one fake `DataMessage` per part.
2. `SourceCoordinator` emits it exactly once (keeping the `sent` flag and the worker-0
   `debug_assert!`), replacing the per-part emit loop.
3. Keep assignment where it lives: the distribute step still owns the keyed
   `rendezvous_select` routing that lands each partition on its host worker. The change is
   confined to the *payload shape* of discovery; the reader ops (`SourcePartitionOp`) and the
   completion protocol are untouched.

The observability payoff is the point of the change:

- A trace or log line can say "source `X` discovered partitions `[a, b, c]`" without decoding
  data-plane sentinels; record counters stop counting `NoData` discovery records as data.
- A future inspection/status endpoint can surface the coordinator's `parts` set directly (it
  is already held in memory).
- Downstream code stops depending on the `NoData`/`Timestamp::MIN` convention to understand
  the partition stream.

## Alternatives considered

- **Status quo (keep the data-plane hack)** — zero churn; the runtime already understands the
  fake message, and the sentinel never reaches user operators. Lost: every observability
  benefit above, and the runtime vocabulary keeps lying about what is data.
- **Feed the reader ops' initial state directly at build** (the redesign's parenthetical) —
  no discovery message at all, but discovery re-runs on rescale and parts move across workers;
  build-time-only state makes the dynamic/rescale path harder to reason about and keeps
  discovery invisible.
- **`SourcePartitions` over the per-source `CommUtility` channel** — reuses the existing
  channel but duplicates the distribute step's assignment logic in the coordinator; rejected
  in favor of keeping one assignment authority.

## Acceptance criteria

- Discovery is observable: an instrumentation hook can report the discovered part set without
  inspecting data-plane messages, and record counters do not count discovery as data.
- Every exhaustive `Message` match/forward site handles the new variant (compiler-enforced);
  the coordinator's fake `DataMessage(part, NoData, MIN)` emission is gone.
- Behavior is identical: same graph, same assignment, same completion; examples and rescaling
  smoke tests pass on single- and multi-worker runs.

## Risks

- **Churn:** the `Message` enum is the runtime vocabulary; a new variant ripples through every
  exhaustive match site (`key_local.rs`, `broadcast.rs`, `sinks/stateful.rs`, the `SafeLogic`
  wrapper dispatch, router conversions). This is exactly why the collapse deferred it "for
  marginal clarity" — the clarity is no longer marginal once observability is on the table.
- **Routing:** the distribute step *is* the assignment mechanism today; the control payload
  must not fork or duplicate that logic, or cross-worker assignment will diverge.
- **Rescale:** `discover` runs again on rescale; the control message must compose with the
  existing part set (`listed_parts`/accumulation) so a rescale does not re-trigger or corrupt
  completion.
