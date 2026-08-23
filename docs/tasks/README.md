# Fix Plan — `new-scheduler` build errors

> **Last refreshed:** 2026-08-22 (new-scheduler @ a4c8fce)
> **Status:** ✅ **ALL FIXED** — `cargo check -p malstrom` green, `cargo test` green
> (**50/50 unit tests + 11/11 doctests**, stable across repeated runs), all examples run
> and terminate cleanly (incl. `MultiThreadRuntime` at parallelism 4), `cargo check
> --workspace` green.

## Task list

| # | Task | Errors | Status |
|---|------|--------|--------|
| [T1](01-remove-stale-communication-error.md) | Remove stale `CommunicationError` references | 2 | ✅ |
| [T2](02-worker-coordinator-error-type.md) | Make `WorkerCoordinatorComm` errors `Send + Sync` | 3 | ✅ |
| [T3](03-cluster-handle-suspend.md) | Implement `ClusterHandle::suspend` | 1 | ✅ (stub) |
| [T4](04-stateless-source-adapter.md) | Finish stateless→stateful source adapter | 12 | ✅ |
| [T5](05-single-thread-runtime-comm.md) | Implement `SingleThreadRuntimeFlavor` communication | 2 | ✅ |

**Bonus:** `MultiThreadRuntime` (`// mod multi;` was commented out) was re-enabled and
fixed; the stale `k8s` crate is unaffected because it pins the published `malstrom 0.1.0`
from crates.io (not the local path).

## Runtime-correctness fixes made beyond compilation

While verifying, the framework turned out to compile but **not run** — every pipeline
produced no output and hung. The data path and completion protocol had multiple bugs:

1. **`ClosedSignal::wait_for` was a future-of-a-future** (`async fn … -> impl Future`);
   the inner wait was never awaited, so every operator exited before applying any logic.
   → plain fn returning the async block.
2. **Sink swallowed the MAX epoch** — the sink's `Logic::apply` only handled
   `Message::Data`; added epoch handling + `Output::close()` so the sink terminates.
3. **`CommUtility` couldn't reach the local worker** — it excluded `ctx.worker_id`
   (`swap_remove`); the partition-op's `PartitionFinished` to worker 0 was dropped
   (single-worker case). Now connects to every worker incl. self, via a dedicated
   `COMM_CHANNEL_ID` (documented collision caveat).
4. **`merge_frontiers` dropped the epoch** — an epoch merged against remotes that
   hadn't reported yet (`None`) was lost; remotes without a source partition no longer
   block it.
5. **Completion protocol was circular** — the root operator never exits (nothing closes
   its output), so `join_all` and the coordinator's `check_execution_complete` hung.
   Fixes: root task excluded from the join, root closes its output when the sys channel
   closes, a completion watch lets the coordination task answer `true`, and the worker
   waits for the coordination task before dropping the comm runtime.
6. **Terminal operators never terminate** — the part-lister on non-0 workers blocks on
   `comm.recv()` forever; added a `no_receivers` signal (output senders wrapped in
   `Rc`, spsc `Receiver` drop notification) so an operator exits once all downstream
   receivers are gone.
7. **spsc waker bug** — `Receive::poll` registered its waker in `send_waker`, so blocked
   receivers were never woken (the `debug_assert`/busy-polling masked it).
8. **1-based channel keys vs 0-based frontiers** — `Input::recv` indexed `frontiers`
   with the receiver key; `link()` now assigns 0-based keys.
9. **`ConnectionKey` normalization merged send/receive channels** — the fix for the
   coordinator channel made the comm utility's outgoing/incoming channels collide
   (MPMC), losing `PartitionFinished`; reverted the normalization and aligned the
   coordinator channel key orientation instead.
10. **`ExecutionComplete` struct vs enum** — the coordinator sent the unit struct, the
    worker decoded the `RuntimeMessage` enum; now sends the enum variant.
11. **Send-after-close panic** — `Output::send` now drops messages once the output is
    closed instead of hitting a `debug_assert!`.

## Test-suite fix (cargo test)

`cargo test` did not compile because the `testing` module was disabled in `lib.rs`
(`// #[cfg(test)] pub(crate) mod testing;`) — every in-file `#[cfg(test)]` block imports
it. Re-enabling it surfaced ~74 errors, all from the test harness being written against the
pre-async-refactor API. Fixes:

1. Re-enabled the `testing` module (74 → 10 errors).
2. Rewrote `testing/communication.rs` (`NoCommunication` → new `new_sender`/`new_receiver`).
3. Rewrote `testing/operator_tester.rs` — `FakeCommunication` implements the new
   `OperatorOperatorComm` + `StreamSender`/`StreamReceiver`; the tester uses the new
   `BuildContext::new`/`OperatorContext::new` signatures, a non-blocking `Input::try_recv`,
   and `futures::executor::block_on` (a nested `LocalRuntime` panics inside `tokio::test`;
   the runtime needed for `BuildContext` is leaked, which is safe — `LocalRuntime` spawns
   no threads of its own).
4. Widened `Interrogate::new`/`Collect::new` to `pub(crate)` and added `Acquire::new`
   (their constructors were `pub(super)`, unreachable from tests).
5. Rewrote the `stateful_op` tests for the channel-based control messages (Interrogate/
   Collect report keys/state via their receivers instead of `try_unwrap`).
6. Modernized stale in-file tests: `operator_io` (async + timeout-based empty checks),
   `spsc` (recv-trait import, dropped `peek_apply` test), `single_iterator`, `map`/
   `inspect`/`filter_map` doc values (`&str` → `String`, needed for `Distributable`).
7. Fixed a cluster of **dropped send futures** (the async-refactor's classic bug: a
   `output.send(...)` without `.await` silently discards the message): `split::Forward`,
   `assign_timestamps`' forward op, `ttl_map::TtlOp`, and — the big one —
   `time::util::handle_maybe_late_msg` (which broke every `generate_epochs` pipeline).

## Remaining follow-ups

- `ClusterHandle::suspend` is a stub (feature was already UNIMPLEMENTED).
- `CommUtility`'s single `COMM_CHANNEL_ID` collides across sources — needs a
  per-source channel id for multiple concurrent sources.
- Dead reference files (`keyed_old/`, `*_old.rs`, `operator_io copy.rs`) still present.
- `malstrom-k8s`/`malstrom-kafka` are pinned to crates.io `malstrom 0.1.0` — they will
  need a migration if switched back to the local path dependency.
- The `testing/` module is still disabled in `lib.rs`.
