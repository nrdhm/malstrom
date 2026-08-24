# Agent Note: Make the async runtime execute and terminate

Status: implemented

## Problem

After compilation was restored, every pipeline **ran nothing and hung**: the hello-world
example printed no records, all doctests failed with empty sinks, and `SingleThreadRuntime`
and `MultiThreadRuntime` alike never terminated. The async refactor had landed the
structure but broken the data path and the completion protocol at eleven distinct points.

## Decision

Eleven fixes, in roughly dependency order:

1. **`ClosedSignal::wait_for` was a future-of-a-future** — declared `async fn … -> impl Future`,
   the inner wait was never awaited, so `tokio::select!` saw the branch ready instantly and
   every operator exited before applying any logic. Now a plain fn returning the async block.
2. **The sink swallowed the MAX epoch** — `StatelessSink`'s `Logic::apply` only handled
   `Message::Data`; added epoch handling plus `Output::close()` so the terminal operator
   terminates.
3. **`CommUtility` could not reach the local worker** — it excluded `ctx.worker_id`
   (`swap_remove`), so the partition-op's `PartitionFinished` to worker 0 was dropped in the
   single-worker case. Now connects to every worker incl. self via a dedicated
   `COMM_CHANNEL_ID`.
4. **`merge_frontiers` dropped epochs** — an epoch merged against remotes that had not
   reported one yet (`None`) was lost forever; remotes without a source partition no longer
   block the epoch.
5. **The completion protocol was circular** — the root operator never exits (nothing closes
   its output), so the worker's `join_all` and the coordinator's `check_execution_complete`
   hung. Fixes: the root task is excluded from the join; the root closes its output when the
   system-message channel closes; a completion watch lets the coordination task answer
   `true`; the worker waits for the coordination task before dropping the comm runtime.
6. **Terminal operators never terminated** — the part-lister on non-0 workers blocks on
   `comm.recv()` forever. Added a `no_receivers` signal: spsc receivers notify on drop, the
   output's senders are `Rc`-wrapped so an owned future can watch them, and the operator loop
   exits once all downstream receivers are gone.
7. **spsc waker bug** — `Receive::poll` registered its waker in `send_waker`, so blocked
   receivers were never woken; progress only happened via coincidental re-polling.
8. **Receiver keys vs frontiers mismatch** — `Input::recv` indexed `frontiers` with the
   receiver key, which `link()` assigned 1-based; `link()` now assigns 0-based keys.
9. **`ConnectionKey` normalization merged send/receive channels** — the normalization added
   for the coordinator channel made the comm utility's outgoing and incoming channels share
   one MPMC channel, so `PartitionFinished` could be consumed by an unused receiver and
   lost. Reverted the normalization; the coordinator's `coordinator_to_worker` now builds
   the same key orientation as the worker's `worker_to_coordinator`.
10. **`ExecutionComplete` struct vs enum** — the coordinator sent the unit struct, the
    worker decoded the `RuntimeMessage` enum; the coordinator now sends the enum variant.
11. **Send-after-close panic** — `Output::send` hit a `debug_assert!` when an operator
    applied once more during shutdown; it now drops the message once the output is closed.

## Alternatives considered

- **`no_receivers` borrow conflict:** the operator loop needs `&mut output` for `apply` and
  a receiver-liveness future for the exit branch. A future borrowing `&output` conflicts;
  Rc-wrapping the spsc senders so the future is owned (`+ 'static`) resolved it. A spawned
  watcher task that closes the output was considered but needs the same ownership machinery.
- **Coordinator channel pairing:** normalize `ConnectionKey` (merged unrelated channels —
  rejected) vs orient both sides of the coordinator connection identically (chosen).
- **Root termination:** root closes its output when the sys channel closes (chosen) vs a
  generic "downstream receiver gone" exit for all operators (kept only for the terminal
  part-lister case via `no_receivers`).

## Consequences

- Single-worker pipelines (doctests, `look_ma_im_streaming`) and multi-worker pipelines
  (`multithreading` at parallelism 4, values distributed across workers) now execute and
  terminate cleanly; the coordinator's completion poll adds ~5s of shutdown latency.
- The shared `COMM_CHANNEL_ID` (u64::MAX) can collide when multiple sources run concurrently
  on one worker — fixed by per-source comm channel ids in the
  [source-trait collapse](../architecture/2026-08-22-collapse-source-traits.md).
- `Output::send` on a closed output silently drops — a deliberate shutdown-race tolerance.
- `Input::try_recv` (noop-waker poll) exists for single-threaded test harnesses; the
  noop waker may clobber a real one, so it must not be used where other tasks wait.
