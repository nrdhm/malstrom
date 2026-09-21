# Agent Note: First-class source discovery message

Status: proposed

## Problem

Part discovery still rides the data plane. After the source-trait collapse, `SourceCoordinator`
(worker 0, raw `Logic`) emits each discovered partition as
`DataMessage::new(part.clone(), NoData, SrcImpl::Timestamp::MIN)` through a
`.distribute(rendezvous_select)` step (`malstrom-core/src/sources/stateful.rs`); the redesign's
first-class control variant — a `Message::SourcePartitions(Vec<PartitionKey>)` emitted by a
framework-owned discovery step (or fed to reader ops as initial state), instead of the
fake-data tuple — was explicitly deferred: see
[collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md),
Decision 3.

The deferral framed the cost as "marginal clarity"; the real issue is **control-plane /
data-plane separation**:

- **Type dishonesty in the runtime vocabulary.** The coordinator's output is typed
  `(PartitionKey, NoData, SrcImpl::Timestamp)`, where `NoData` and `Timestamp::MIN` are lies —
  the payload is a partition announcement, not a record. Because it is a `Message::Data`, it
  flows through the *data* router's versioning/ordering machinery (`VersionedMessage`,
  `TargetedMessage`, the router task), which exists for records and has no meaning for
  discovery.
- **`discover()` can't be re-run on rescale.** `SourceImpl::discover` is documented
  "…and again on rescale" (`malstrom-core/src/sources/stateful.rs`) but is actually called
  **once**, at build (`SourceCoordinator::build`). Rescale moves *state* via
  `Acquire`/`Collect`/`Interrogate` but cannot *discover new partitions*. A control message is
  the natural vehicle for rescale-time rediscovery.
- **Discovery is invisible.** The coordinator already holds the full set (`parts: IndexSet`)
  in memory, but there is no event or surface that reports "source X discovered [a, b, c]"
  without decoding the data-plane sentinel.

What it is **not**: a live exactly-once bug. The half-discovered-partition-set-at-a-barrier
situation is currently safe only because `discover()` re-runs on restart and re-announces every
partition — the snapshot holds per-partition *reading state*, not the partition set. That
invariant is implicit, though; a control message makes the discovery/barrier ordering
explicit rather than accidental.

## Proposal

Make discovery a first-class control message, keeping the current graph shape and assignment
logic. The message shape is **per-partition** (not one full-set message), to keep the
`rendezvous_select` assignment exactly where it lives today:

1. Add a control variant to `Message` in `malstrom-core/src/types/message.rs`:
   `SourcePartition(PartitionKey)` — one per discovered partition, alongside the existing
   control variants (`Rescale`, `ReconfigComplete`, `Interrogate`, `Collect`, `Acquire`).
2. `SourceCoordinator` emits one `Message::SourcePartition(part)` per discovered part
   (keeping the `sent` flag and worker-0 `debug_assert!`), replacing the fake
   `DataMessage(part, NoData, MIN)` loop. The type of the coordinator→distribute stream
   changes from a data tuple to the control variant.
3. The distribute step forwards the control variant along its **control** path (broadcast to
   readers, like `Rescale`/`ReconfigComplete`) rather than routing it as data; each
   `SourcePartitionOp` claims its `rendezvous_select`-assigned subset on receipt.

> The earlier "full set in one message" wording (`SourcePartitions(Vec<PartitionKey>)`)
> conflicted with "keep assignment at the distribute step": one message carrying every
> partition cannot be keyed-routed by `rendezvous_select` without forking the assignment
> logic into the reader. Per-partition control messages resolve that tension. A full-set
> broadcast is a separate, later enhancement if atomic "here is the whole discovery" is ever
> wanted — see Alternatives.

## Codebase impact

- **`Message` enum** (`malstrom-core/src/types/message.rs`) — one new variant.
- **`SafeLogicWrapper::apply`** (`malstrom-core/src/stream/operator_logic.rs`) — the dispatch
  is **exhaustive with no catch-all** (8 arms), so the new variant forces a new
  `SafeLogic::on_source_partition(…)` hook (default no-op) plus a real implementation in
  `SourcePartitionOp` (replacing today's `on_data` → `add_partition` path).
- **`SourceCoordinator`** (raw `Logic`, `malstrom-core/src/sources/stateful.rs`) — emits the
  control message instead of fake data; its own hand-rolled match must forward the new variant.
- **Distribute layer** (`keyed/distributed/distributor.rs`, `remote_receiver.rs`) — classify
  `SourcePartition` as control and forward it on the control path (local), bypassing the data
  router's versioning.
- **Compile-enforced ripple** — every exhaustive `Message` match site the compiler flags
  (the `SafeLogicWrapper` dispatch, `SourceCoordinator`'s raw match, the distribute operators,
  the keyed routers' conversions).

Cross-worker discovery (a `WireMessage` variant + the k8s gRPC transport) is **not** part of
this note — it is a separate dependent note, [source-discovery-wire-message](2026-08-24-source-discovery-wire-message.md).

## Implementation plan

1. **`malstrom-core/src/types/message.rs`** — add `SourcePartition(PartitionKey)` to
   `Message<M>`.
2. **`malstrom-core/src/stream/operator_logic.rs`** — add
   `SafeLogic::on_source_partition(&mut self, key, output, ctx)` (default no-op) and a
   `Message::SourcePartition(..)` arm in `SafeLogicWrapper::apply` that calls the hook then
   forwards the message downstream (the same shape as the `Rescale`/`ReconfigComplete` arms).
3. **`malstrom-core/src/sources/stateful.rs`** — `SourceCoordinator` emits
   `Message::SourcePartition(part.clone())` per part instead of the fake data tuple;
   `SourcePartitionOp` implements `on_source_partition(key)` → `add_partition(key, None)`,
   and its `on_data` no longer handles discovery (only real records). Remove the
   `NoData`/`Timestamp::MIN` sentinel from the source path.
4. **Route as control, not data** — in the distribute layer
   (`keyed/distributed/distributor.rs`, `remote_receiver.rs`), classify `SourcePartition` as
   a control message (forward via the `VersionedMessage::Other`/`TargetedMessage::Other`
   path) so it bypasses the data router's versioning.
5. **Compile-enforced ripple** — update every exhaustive `Message` match site the compiler
   flags.
6. **Verify** — `cargo test -p malstrom` (unit + doc) and single- plus multi-worker smoke
   (`look_ma_im_streaming`, `multithreading`) behave identically.

## Follow-up work

- **Cross-worker discovery** — [source-discovery-wire-message](2026-08-24-source-discovery-wire-message.md),
  dependent on [point-k8s-and-kafka-at-local-malstrom](../../proposed/process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md).
- **Rescale rediscovery** — make `discover()` run on rescale and re-announce via the control
  message, composing with the existing part set; fixes the `discover()` doc lie and completes
  the zero-downtime story. Depends on this note.

## Alternatives considered

- **Status quo (keep the data-plane hack)** — zero churn; the runtime already understands the
  fake message. Lost: type honesty in the runtime vocabulary, a real control path for
  discovery, rescale rediscovery, and observability.
- **Full-set `SourcePartitions(Vec<PartitionKey>)` broadcast** — one message, atomic; but it
  moves `rendezvous_select` assignment into the reader (or a shared helper) and risks
  diverging from the distribute step's routing. Deferred in favor of per-partition control
  messages; revisitable later.
- **Feed the reader ops' initial state directly at build** (the redesign's parenthetical) —
  no discovery message at all, but discovery re-runs on rescale and parts move across workers;
  build-time-only state keeps discovery invisible and complicates the rescale path.
- **`SourcePartition` over the per-source `CommUtility` channel** — reuses the existing
  channel but duplicates the distribute step's assignment logic in the coordinator; rejected
  in favor of keeping one assignment authority.
- **Fold the wire variant into this note** — the `WireMessage` change couples to the frozen
  crates.io k8s crate; keeping it separate lets the core change land on the threaded runtimes
  without the k8s migration as a blocker.

## Acceptance criteria

- Discovery is a control message: the coordinator's fake `DataMessage(part, NoData, MIN)`
  emission is gone; every exhaustive `Message` match/forward site handles the new variant
  (compiler-enforced).
- Discovery is observable without decoding data-plane sentinels (a hook can report the
  discovered part set).
- Behavior is identical: same graph, same assignment, same completion; examples and rescaling
  smoke tests pass on single- and multi-worker runs.

## Risks

- **Barrier ordering** — the control messages must be emitted/forwarded *before* the first
  snapshot barrier so the (currently implicit) "discovery precedes checkpoint" invariant stays
  explicit; this must be designed, not assumed.
- **Routing** — the distribute step *is* the assignment mechanism today; per-partition control
  messages must continue to use `rendezvous_select` without forking or duplicating it, or
  cross-worker assignment will diverge.
- **Rescale** — `discover` runs again on rescale; the control message must compose with the
  existing part set (`listed_parts`/accumulation) so a rescale does not re-trigger or corrupt
  completion.
