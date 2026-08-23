# Agent Note: Give each CommUtility pair its own channel id

Status: rejected — superseded by [collapse-source-traits](../../proposed/architecture/2026-08-22-collapse-source-traits.md): the frontier-merge completion removes the PartitionFinished protocol, so there is no shared comm channel to fix

## Problem

All `CommUtility` instances share one channel id (`COMM_CHANNEL_ID = u64::MAX` in
`operators/com_utility.rs`). With one source per job this is fine; with **multiple
concurrent sources** the part-listers' and partition-ops' comm channels collide on the same
`(worker, worker, u64::MAX)` keys, so messages (e.g. `PartitionFinished`) can be delivered
to the wrong receiver and lost.

## Proposal

Derive the comm channel id from something the two sides of a pair share. Both operators
derive their names from the source name (`{name}-list-partitions` and `{name}-partition`),
so hash the **source name** (strip the operator suffix) — or pass an explicit channel id
through the source-building API — and use that instead of `u64::MAX`.

## Alternatives considered

### Why not a fixed id per pair assigned at build time?
The two operators are built independently; a shared identifier must come from the source
name or be threaded through the builder API. Name-derived hashing needs no API change.

### Why not make the comm channel key include the sender operator id?
The receiver side (part-lister) does not know the sender's operator id; the key must be
derivable on both sides.

## Acceptance criteria

- Two sources on one worker each deliver `PartitionFinished` to their own part-lister.
- The collision caveat comment in `com_utility.rs` is removed.

## Risks

- Hashing name prefixes is brittle if operator naming changes; prefer an explicit channel
  id threaded through the API if the name scheme evolves.
