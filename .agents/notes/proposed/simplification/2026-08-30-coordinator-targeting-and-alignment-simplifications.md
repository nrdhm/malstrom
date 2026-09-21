# Agent Note: Simplify coordinator targeting and the alignment/scale test paths

Status: proposed

## Problem

The `new-scheduler-tests` review verified the branch's test coverage against
[core-test-plan](../../implemented/testing/2026-08-25-core-test-plan.md) and left four
simplifications unaddressed. Each is small, but together they remove a busy-wait, an
O(workers × targets) lookup, a non-total branch in a channel, and a state-drift hazard:

- **Coordinator target lookup is O(workers × targets).**
  `Coordinator::start_build` and `start_execution` take `targets: &[WorkerId]` and filter with
  `targets.contains(wid)` (`malstrom-core/src/coordinator/cluster.rs:51,67`). The rest of the
  coordinator already uses `IndexSet<WorkerId>` for worker sets, so the slice type is the
  outlier and the linear scan is avoidable.
- **`AlignmentGroup::recv` busy-hangs when every receiver is paused.** When
  `recv_futures.is_empty()` it awaits `std::future::pending::<()>()`
  (`malstrom-core/src/channels/alignment.rs:115-117`). Today no caller enters with all
  receivers already paused, so the empty set only means "source, no receivers"; but if that
  assumption changes the group hangs instead of emitting the already-collected aligned value.
  The branch is not total.
- **`workers_spawned` drifts from the live worker count.** `MultiThreadRuntime::execute`
  tracks `workers_spawned` separately and `threads.retain(|x| !x.is_finished())` removes
  finished handles (`malstrom-core/src/runtime/threaded/multi.rs:86,104`), but
  `workers_spawned` never decreases. A worker that exits frees its handle without freeing its
  slot, so the two diverge on a later rescale.
- **The cluster test uses a `yield_now()` spin-loop.** `MockComm::take_receiver` busy-polls on
  a `std::thread::yield_now()` inside an async test
  (`malstrom-core/src/coordinator/cluster.rs:276-281`). It works on the current-thread Tokio
  runtime but is a scheduling-sensitive wait; a deterministic handshake would not be.

## Proposal

Four independent, individually-landable changes.

### 1. `IndexSet` coordinator targets

Change the signatures to take `targets: &IndexSet<WorkerId>` so `contains` is O(1); keep
`WorkerCoordinatorComm` callers building one `IndexSet` at the call site. `reconfigure` already
has `new_set: IndexSet<WorkerId>`, so it can pass it through directly, and initial startup in
`coordinator.rs` can collect the worker ids into one `IndexSet` reused by both
`start_build` and `start_execution` instead of rebuilding the same `Vec` twice. Alternatively,
add `start_build_all()` / `start_execution_all()` for the common all-workers case.

### 2. Make the alignment empty-set branch total

Restructure `AlignmentGroup::recv` so the "no receivers" case (source) is distinguished from
"all receivers paused, `recv_futures` empty". Only the latter awaits `pending()`; collect the
paused values and return an `AlignedValue::Aligned` (or the appropriate aligned result)
instead of hanging.

### 3. Derive the live count instead of tracking it

Hold coordinator and worker handles in separate `Vec`s (`Vec<JoinHandle<Coordinator…>>`,
`Vec<JoinHandle<…>>`), so `workers_spawned == worker_threads.len()` and a rescale can re-spawn
a worker id whose thread has exited. This is the more invasive of the four.

### 4. Deterministic test handshake

Have `MockComm::coordinator_to_worker` deliver the fresh `flume::Receiver` over a per-worker
`flume::Sender<flume::Receiver<ReqRes>>`, and have the test `await handshake_rx.recv_async()`
once, removing the `yield_now()` loop.

## Alternatives considered

- **Keep `&[WorkerId]` and accept the linear scan.** Fine for the current worker count, but the
  codebase already standardized on `IndexSet` for worker sets; matching it also lets
  `reconfigure` stop rebuilding collections.
- **Leave the alignment `pending()` branch as-is.** Correct under the current invariant
  (callers never arrive with all receivers paused). Lost: the invariant stays implicit and the
  empty-set case stays a hang rather than a total function if a caller changes.
- **Keep `workers_spawned` as a counter.** Cheapest fix for the original off-by-one bug. Lost:
  the counter must be decremented for every worker-exit path forever, and it already drifts;
  deriving the count from the handle vector makes drift impossible.
- **Keep the `yield_now()` spin.** No test change. Lost: a scheduling-sensitive wait that can
  misbehave under a different Tokio flavor; the handshake removes the wait entirely.

## Acceptance criteria

- `start_build` / `start_execution` take `&IndexSet<WorkerId>` (or `_all` helpers exist), and
  `cargo test -p malstrom-core` stays green.
- An `AlignmentGroup` with all receivers paused returns an aligned value rather than hanging,
  pinned by a unit test.
- `MultiThreadRuntime` derives the worker count from its handle vector; a rescale after a
  worker exits re-spawns the slot, pinned by a test.
- `cluster.rs` has no `yield_now()`; the mock uses a channel handshake.

## Risks

- Deriving the worker count is the only invasive change and touches the rescale path that the
  `rescale.rs` / `cluster.rs` tests exercise; land it separately and watch those tests.
- Changing the coordinator target type ripples to every `WorkerCoordinatorComm` caller across
  the coordinator, worker and test mocks; mechanical but broad.

## Related

These items were first recorded in the (now removed) `new-scheduler-tests-review.md` working
review; this note is their sole current record.

- [core-test-plan](../../implemented/testing/2026-08-25-core-test-plan.md) owns the test suite
  and the `workers_spawned` bug fix these items build on.