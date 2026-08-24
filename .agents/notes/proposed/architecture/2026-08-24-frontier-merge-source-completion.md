# Agent Note: Reassess frontier-merge source completion

Status: proposed

## Problem

Source completion still runs on the bespoke protocol the review flagged: each reader sends
`PartitionFinished(key)` via `CommUtility` to worker 0, and `SourceCoordinator` holds the
global part set, emitting `Epoch(Timestamp::MAX)` only when
`listed_parts && parts.is_empty()` (`malstrom-core/src/sources/stateful.rs`). The redesign's
step 4 — reader exhaustion → `distribute` frontier merge → `Epoch(MAX)`, deleting
`PartitionFinished`, the shared-channel/worker-0 machinery — was **not adopted** at
implementation time because of a multi-worker ordering hazard: a reader-local MAX can race
partitions still in flight through the distribute step's async router (a reader can exhaust
before other partitions are even assigned, emitting a premature MAX and losing data). The
collapse note explicitly left the door open: *"the redesign's frontier-merge path remains
available to revisit if the distribute step ever becomes a strict pipeline stage"* — see
[collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md),
Decision 4 and Alternatives.

The current protocol works and is correct, but it is a parallel control plane: a serde wire
type (`PartitionFinished`), per-source comm channels (`seahash` ids), a hardcoded worker-0
completion authority, and an out-of-band signal that is unordered with respect to the data it
completes. Completion is the one place in the runtime that does not fall out of the ordinary
epoch/barrier machinery.

## Proposal

Reassess the frontier-merge design under explicit enabling conditions, not as a blanket
reversal. The rejection hinged on two facts; each needs a contained answer:

1. **The exhaustion signal is out-of-band.** `PartitionFinished` travels a side channel, so it
   is unordered with respect to that partition's data. Make it in-band: the reader op emits a
   completion signal **on its own output stream, after the partition's last record**, so
   per-partition ordering with the data is guaranteed by the channel itself. No race for a
   partition's own records is possible.
2. **A correct MAX needs the full discovered set.** A stream-wide `Epoch(MAX)` from any reader
   is only safe once every partition is known exhausted — worker-local knowledge is
   insufficient. The merge point must know the global part set; with the first-class discovery
   message (see
   [first-class-source-discovery-message](2026-08-24-first-class-source-discovery-message.md))
   the completion authority can hold the full set without the `CommUtility` side channel. This
   preserves the coordinator's `listed_parts` guard ("a rescale later assigns new partitions")
   as an explicit part of the merge condition.

If both hold, the completion authority (the distribute op, or a coordinator on the data plane)
merges per-partition exhaustion into `Epoch(MAX)` through the existing frontier machinery, and
`PartitionFinished`, the per-source comm channels, and the hardcoded worker 0 disappear — the
review's smell #10 resolved as originally intended (see
[the review](../../../../docs/reviews/sources-module-review.md), §10).

The reassessment should first **pin down the exact hazard** with a written ordering argument
(or a test that reproduces it on the current code), then decide whether the in-band signal
removes it or whether the "strict pipeline stage" condition from the collapse note is
genuinely required.

## Alternatives considered

- **Status quo (global coordinator + side channel)** — simple and correct; the cost is the
  parallel control plane and the worker-0 authority.
- **Two-level merge (worker-local, then worker-0 final)** — reduces single-point coupling but
  keeps a side channel and a worker-0 role; judged not worth the machinery unless the full
  redesign lands.
- **Per-key epoch/frontier extension of the `Message` model** — the cleanest semantics
  (per-partition frontiers) but a large runtime change affecting every operator; the in-band
  signal is the minimal version of this.
- **Keep `CommUtility` but move the completion authority onto the data plane** — a middle path
  that removes the hardcoded worker-0 logic without touching the epoch model; only worth it if
  the full redesign does not.

## Acceptance criteria

- Identical end-of-stream behavior on single- and multi-worker jobs, verified against the
  current protocol; no data loss — the ordering hazard is disproven or contained by
  construction, not by luck.
- `PartitionFinished`, per-source `CommUtility` completion, and the hardcoded worker-0 MAX
  emission are removed.
- Rescale-during-exhaustion is covered by a test: a rescale that assigns new partitions must
  not produce a premature MAX (the `listed_parts` guard's purpose today).
- Completion is observable through the existing frontier tooling (`InspectFrontier`).

## Risks

- **The rejection reason must be conclusively addressed.** A subtle ordering bug at
  end-of-stream is silent data loss — the worst failure mode; this is why the reassessment
  must start with the written ordering argument, not the implementation.
- The epoch/frontier machinery is shared by every operator; changes there are
  high-blast-radius.
- Rescale interplay (the `listed_parts && parts.is_empty()` comment explicitly guards "a
  rescale later assigns new partitions").
