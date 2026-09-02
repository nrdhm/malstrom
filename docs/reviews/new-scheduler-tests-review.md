# Review: `new-scheduler-tests` vs the implemented core-test-plan note

> **Last refreshed:** 2026-08-30

Reviewed `fork/new-scheduler-split..HEAD` (7 commits) against
[`.agents/notes/implemented/testing/2026-08-25-core-test-plan.md`](../../.agents/notes/implemented/testing/2026-08-25-core-test-plan.md)
and ran the suite locally.

Verified results:

- `cargo test -p malstrom-core --lib --tests`: 57 kernel unit tests + 7 integration tests
  (4 binaries: 2 + 1 + 3 + 1)
- `cargo test -p malstrom-core --doc`: 8 doc tests
- `cargo test -p malstrom --tests`: 3 facade tests
- `cargo clippy -p malstrom-core --all-targets -- -D clippy::correctness`: passes
- `cargo fmt --all -- --check`: **fails** — 31 format hunks; the baseline branch is
  format-clean

## 1. Doc vs actual code: mostly accurate

### Test counts

The note says: baseline 19 unit tests → 40 after Layer 2 → 57 final kernel unit tests,
75 total. Verified exactly:

- Baseline has 19 *runnable* unit tests. The apparent 22-count comes from three
  commented-out `#[tokio::test]` blocks in `channels/signal.rs`, which are not compiled.
- Layer 2 commit `2e9ea3f` adds exactly 21 runnable test attributes → 40.
- Layer 3 adds 10 proptest attributes; Layer 4 adds 7 regression attributes → 57.
- Integration: 7; doc: 8; facade: 3 → 75 total.

### The four “real bugs” are real and fixed where the doc says

| Claim | Verified in code |
|---|---|
| Rescale sent startup protocol to existing workers | `reconfigure` now bootstraps only `new_workers` in `cluster.rs` |
| Rescale counted the coordinator thread in `threads.len()` | `multi.rs` now tracks `workers_spawned` separately |
| Terminal spsc send blocked forever after 1024 messages | `Send::poll` drops `value` when `!has_receiver` |
| `Output::close()` was a silent no-op | `send_replace(true)` used in `close()`, Suspend auto-close, and `CHECK_FINISHED` auto-close |

All four are pinned by named regression tests as the doc claims.

### Layer claims

- Layer 1a: 4 integration binaries using only `malstrom_core` — matches.
- Layer 1b: `hello_pipeline.rs` + `namespace.rs` — matches.
- Layer 2/3/4/5: the named tests, proptest modules, and 8 doc examples all exist and run
  green.

### Minor caveats

- “No sleeps in tests” is literally true — there is no `sleep()`. But several unit tests
  rely on real-time `tokio::time::timeout(...)` to assert “this does **not** happen”, and:
  - `tests/rescale.rs` uses blocking `rx_seen.recv().unwrap()`
  - `cluster.rs` unit test uses a `std::thread::yield_now()` spin-loop

  This is not “fully deterministic async” in a strict sense. The doc’s wording is
  defensible but slightly optimistic.

## 2. Excessive / useless code lines

### Strong candidates (should be deleted)

1. **`malstrom-core/src/channels/operator_io.rs:39-41`** — dead local `finalized_signal`

   ```rust
   /// Allow NoTime type to indicate a final output
   /// even if send is never called on this output
   let finalized_signal = Signal::new(M::Timestamp::CHECK_FINISHED(&None));
   ```

   The variable is never read. This also makes the `Signal` import appear used, hiding the
   dead code.

2. **`malstrom-core/src/channels/operator_io.rs:170-188`** — `UpstreamState` struct +
   `new()`. It is never constructed or referenced anywhere. Entirely dead.

3. **`malstrom-core/src/coordinator/messages.rs:32-33`** — dead `struct ExecutionComplete`.
   This is directly relevant to the “struct vs enum” regression row. The enum variant
   `RuntimeMessage::ExecutionComplete` is the real wire type; the standalone unit struct is
   never used. Leaving it around is a trap for the exact bug the tests claim to prevent.
   Delete it.

4. **`malstrom-core/src/coordinator/cluster.rs:258`** — unused `ReqResResponder` import in
   the new test module.

### Unused imports in files this branch touched

- `channels/operator_io.rs:9,16,19` — `SignalHandle`, `SuspendMarker`,
  `snapshot::SnapshotBarrier`, `FuturesUnordered`, `oneshot`
- `channels/alignment.rs:4,6,8` — `IndexSet`, `crate::channels::spsc`,
  `super::spsc::Receiver`
- `types/distributable.rs:3` — `use crate::types::Kvt;`
- `runtime/threaded/communication/inter_thread.rs:18` — `use tracing::debug;`

These are mostly pre-existing warnings, but the branch touched each file and could have
cleaned them.

### Duplicated / redundant code

- `operator_io.rs:118-120` — `close()` has two overlapping doc lines:

  ```
  Mark this output as closed, causing downstream operators watching [get_closed_signal] to stop.
  Mark this output as closed; further sends are dropped.
  ```

- `coordinator.rs:109-114` — `start_build(...)` and `start_execution(...)` each build the
  same `state.workers.keys().copied().collect::<Vec<_>>()` allocation. Collect once and
  reuse.
- `tests/rescale.rs:56-66` and `71-80` — the `match msg { ... }` block is duplicated
  between the `try_recv()` branch and the `select!` branch. Extract one
  `async fn forward(output, msg)` helper.

## 3. Algorithm simplifications worth proposing

### a. `start_build` / `start_execution` target type

`cluster.rs` currently filters with `targets.contains(wid)` over a `&[WorkerId]`, which is
O(workers × targets). Since the rest of the code already uses `IndexSet<WorkerId>` for
worker sets, change the signature to:

```rust
pub async fn start_build(&self, targets: &IndexSet<WorkerId>)
```

- `contains` becomes O(1)
- `reconfigure` can keep `new_workers` as an `IndexSet`
- initial startup in `coordinator.rs` can reuse one collected `IndexSet`

Alternatively, add `start_build_all()` / `start_execution_all()` helpers and avoid the
allocation in the common all-workers case.

### b. `reconfigure` bootstrap set

Currently `new_workers` is a `Vec`; use an `IndexSet` instead. It is semantically the same
type as `new_set` and `BuildInformation.worker_set`, and it makes the two
`start_build`/`start_execution` calls use the O(1) lookup from (a).

### c. Replace the spin-loop in `cluster.rs` test

```rust
fn take_receiver(&self, worker: WorkerId) -> flume::Receiver<ReqRes> {
    loop {
        if let Some((_tx, rx)) = self.channels.lock().unwrap().remove(&worker) {
            return rx;
        }
        std::thread::yield_now();
    }
}
```

This busy-polls inside an async test on a current-thread Tokio runtime. A cleaner
deterministic handshake:

- have `MockComm::coordinator_to_worker` send the fresh `flume::Receiver` over a
  per-worker `flume::Sender<flume::Receiver<ReqRes>>`
- have the test await `handshake_rx.recv_async().await` once

That removes the `yield_now()` loop entirely.

### d. `AlignmentGroup::recv` pending hack

```rust
if recv_futures.is_empty() {
    std::future::pending::<()>().await;
}
```

This is a subtle hack: it is correct for the empty-receivers/source case, but if all
receivers were already paused it would hang instead of emitting the aligned group. Better:

```rust
if self.receivers.is_empty() {
    std::future::pending::<()>().await;
}
// then when recv_futures.is_empty(): collect paused values and return Aligned
```

That makes the “all barriers already collected” state total instead of relying on the fact
that current callers never enter with all paused.

### e. `MultiThreadRuntime::execute` worker tracking

`workers_spawned` is fine for the bug fix, but it drifts if a worker thread exits:
`threads.retain(...)` removes finished handles while `workers_spawned` never decreases. A
slightly more robust design is to keep the coordinator handle in one `Vec` and worker
handles in a separate `Vec<JoinHandle<...>>`; then `workers_spawned = worker_threads.len()`
and a future rescale can re-spawn a replaced worker id. This is more invasive, so it is a
suggestion rather than a small change.

## 4. Things that should be addressed before the branch is “shipped”

1. **`cargo fmt --all -- --check` fails.** The baseline is format-clean; HEAD has 31
   rustfmt hunks across 13 files, all in files this branch added or touched (e.g.
   `hello_pipeline.rs`, `namespace.rs`, `alignment.rs`, `safe_logic_contract.rs`). The CI
   workflow’s first step would fail. Run `cargo fmt` and amend.

2. **Delete the dead `ExecutionComplete` struct.** It undermines the regression-test story
   for the struct-vs-enum bug.

3. **Delete the dead `finalized_signal` / `UpstreamState` in `operator_io.rs`.** They are
   exactly the kind of “looks meaningful but does nothing” code that hides intent.

4. **Consider replacing the `yield_now()` spin in `cluster.rs`** before it becomes flaky
   under a different Tokio flavor or scheduling change.

## Conclusion

The doc is an accurate description of the branch’s **test coverage and bug-fix claims** —
the counts are exact and all four bug fixes are real. The main quality problems are not in
the test strategy but in the details:

- the branch is not rustfmt-clean, which contradicts “ships green” and would fail CI
  immediately,
- several dead-code remnants should have been removed while touching those files
  (`finalized_signal`, `UpstreamState`, standalone `ExecutionComplete`),
- and a few test/API simplifications (`IndexSet` targets, handshake instead of spin-wait,
  shared `forward` helper) would make the code smaller and more deterministic.