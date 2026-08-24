# Agent Note: Source discovery wire message

Status: proposed

## Problem

Cross-worker discovery still rides the wire data plane. Once
[first-class-source-discovery-message](2026-08-24-first-class-source-discovery-message.md)
lands, `SourceCoordinator` emits `Message::SourcePartition(key)` control messages — but on a
multi-worker job the `.distribute(rendezvous_select)` step sends partitions to other workers
through `WireMessage`, which today only carries `Data / Epoch / SnapshotBarrier / Acquire`
(`malstrom-core/src/keyed/distributed/wire_message.rs`). Until a control wire variant exists,
discovery either cannot cross workers or falls back to a serialized `Data` payload — the same
type-dishonesty the core note removes, one level down.

The blocker is `malstrom-k8s`: its gRPC runtime serializes `WireMessage` and is pinned to the
**published** crates.io `malstrom 0.1.0`, not the local path — so any `WireMessage` change
lands on a frozen crate that never builds against `new-scheduler`. The wire change therefore
cannot ship until
[point-k8s-and-kafka-at-local-malstrom](../../process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md)
is done.

## Proposal

Ship the cross-worker half of source discovery as a wire control message, in lockstep with the
k8s re-pointing:

1. Add `WireMessage::SourcePartition(…key…)` to
   `malstrom-core/src/keyed/distributed/wire_message.rs`, carrying the same
   `PartitionKey` as the in-process `Message::SourcePartition`.
2. Handle it in `DistributorReceiver::handle_remote_message`
   (`malstrom-core/src/keyed/distributed/remote_receiver.rs`) — decode and forward it on the
   control path (same as the local case), so a reader op receives it identically whether the
   partition was announced locally or from another worker.
3. Serialize/deserialize it in `malstrom-k8s`'s transport alongside `WireMessage::Data`, once
   that crate is re-pointed at the local `malstrom` and migrated to the new comm API.

## Dependencies

- Depends on [point-k8s-and-kafka-at-local-malstrom](../../process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md)
  (the k8s crate must build against the local `malstrom` before its `WireMessage` codec can
  change).
- Depends on [first-class-source-discovery-message](2026-08-24-first-class-source-discovery-message.md)
  (the in-process variant it extends to the wire).

## Alternatives considered

- **Ship the wire variant before the k8s re-pointing** — the change would land on a frozen
  crates.io crate that never compiles against it; the workspace's green status would mask the
  divergence. Rejected.
- **Skip the wire variant; keep discovery-as-`WireMessage::Data` across workers** — splits
  the model: control on the threaded runtimes, data on the wire. The type dishonesty the core
  note fixes survives exactly where the multi-worker/k8s path lives. Rejected.
- **Broadcast the full discovery set over the existing barrier-sender channels** — reuses a
  control channel but introduces a second assignment authority beside the distribute step's
  `rendezvous_select`. Rejected; keep one routing authority.

## Acceptance criteria

- `WireMessage::SourcePartition` round-trips through the k8s gRPC transport; a reader op opens
  a partition announced from a remote worker exactly as it does a local one.
- `cargo check --workspace` compiles with `malstrom-k8s` re-pointed at the local `malstrom`.
- Multi-worker smoke (≥2 workers) passes: discovery crosses workers, assignment matches the
  threaded runtime, completion is unchanged.

## Risks

- **k8s migration coupling** — the wire change is blocked on a substantial migration
  (`transport.rs`/`worker_backend.rs` rewrite). Budget the two together; do not land the wire
  variant while the crate is still pinned.
- **Wire stability** — `WireMessage` is a durable contract across versions; the variant must
  be additive and not renumber or reinterpret existing variants.
- **Assignment divergence** — the remote decode path must apply the same `rendezvous_select`
  claim as the local path, or cross-worker assignment silently differs from single-worker.
