# Branch Overview — `new-scheduler`

> **Last refreshed:** 2026-08-23 (new-scheduler @ a4c8fce)

## At a glance

- **Base:** `main` (merge base `be253f5`)
- **Commits:** 19 (2025-09-08 → 2026-07-20), all unmerged work-in-progress
- **Scope:** 163 files changed, **+10,028 / −5,048** lines
- **Compile status:** ❌ **does not currently compile** — `cargo check --workspace` fails with
  20 errors in `malstrom` (core lib), concentrated in the stateful-source rework
  (`sources/stateful.rs`, `sources/stateless.rs`)

Despite the name, this branch is not only a new scheduler — it is a **broad, long-running
refactor** of the core framework: an async conversion, a restructured worker/coordinator
execution model, a rework of distributed keyed-stream routing, and a cleaned-up operator API.

---

## The big themes

### 1. Async conversion of the core
The single most visible change. The core moves from the old sync/threaded model toward
async execution:

- `malstrom-core` bumped to **edition 2024**; `tokio` upgraded to 1.47 (new deps:
  `malstrom-macros`, `console-subscriber`, `pin-project`)
- Operators converted piece-by-piece to async (commit arc: *"2024 edition changes"* →
  *"mostly compiles"* → *"core compiles"* → *"converted some functions to async"* →
  *"convert more operators to async"* → *"some things now work async!"* → *"add missing awaits"*)
- `worker/mod.rs` now builds on `tokio::runtime::LocalRuntime` with async tasks
  (`worker::Worker::new` is async, mpsc channels, `SignalHandle`)
- `.cargo/config.toml` added: `-Awarnings` and `--cfg tokio_unstable` (needed for
  `LocalRuntime` / `console-subscriber`)

### 2. New execution architecture: worker & coordinator restructured
The old monolithic modules were split into focused files:

- **Worker** (`worker/`): `worker/mod.rs` shrank ~330 lines → thin module re-exporting
  `builder.rs` (runtime/stream construction), `worker.rs` (the async worker task),
  `coordination_task.rs` (talks to the coordinator: builds, snapshots, reconfigs),
  `root_logic.rs` (root stream logic), `stream_provider.rs` (the user-facing builder API),
  `sys_message.rs`
- **Coordinator** (`coordinator/`): the 589-line `coordinator.rs` was gutted and split into
  `api.rs` (user-facing `CoordinatorApi`), `cluster.rs` (new `ClusterHandle` tracking worker
  states, snapshot/config versions), `messages.rs` (typed messages), `snapshot.rs`;
  `COORDINATOR_ID = WorkerId::MAX` convention. Old `state.rs` preserved as `state_old.rs`.
- **Communication** (`runtime/communication/`): the old single `communication.rs`
  (226 lines) and `threaded/communication.rs` (255 lines) became directories with focused
  files: `operator_operator.rs`, `worker_coordinator.rs`, `reqres.rs`, `stream.rs`
  (+ `inter_thread.rs` for the threaded flavor).
- `errorhandling.rs` removed; `testing/` module temporarily disabled in `lib.rs`.

### 3. Distributed keyed-stream routing rework (the "ICA" architecture)
The `keyed/distributed.rs` monolith (721 lines) was deleted and re-created as a structured
module:

- New files: `acquire.rs`, `collect.rs`, `distributor.rs`, `interrogate.rs`,
  `remote_receiver.rs` (400 lines), `remote_sender.rs`, plus a `routers/` submodule
  (`normal.rs`, `collect.rs`, `interrogate.rs`, `upgrading.rs`) and wire types
  (`targeted_message.rs`, `versioned_message.rs`, `wire_message.rs`)
- New `keyed/broadcast.rs` for broadcast streams
- The author's own status notes (`Status.md`) describe the intended three-stage routing
  pipeline per operator: **`input_recv`** (local+remote messages in, versioned messages with
  sender out; keeps client set current, aligns barriers) → **`state-handler`** (runs the ICA
  algorithm, buffers collected messages) → **`output_send`** (routes via the correct router:
  WireMessage remote / normal local)
- The old implementation was preserved as **`keyed_old/`** (including `message_router/`,
  `reconfig_task.rs`, `routers.rs`, `remote_receiver.rs`) — a reference while the rewrite is
  in progress

### 4. Operator API cleanup: the `Kvt` trait
A new trait bundles the three message type parameters — Key, Value, Timestamp — into one
bound, cutting generic verbosity (documented in the new `website/internals/KvtTrait.md`):

```rust
pub trait Kvt: Clone + 'static {
    type Key: MaybeKey;
    type Value: MaybeData;
    type Timestamp: MaybeTime;
}
```

Implemented for tuples `(K, V, T)`, with new message types in `types/message.rs`
(`DataMessage`, `Kvt`, reworked `Message`, `Barrier`, `RescaleMessage`, …) plus new
`types/distributable.rs` and `types/sealed.rs`.

### 5. Stream module restructure
- `stream/operator/{builder,context,logic,runnable,standard,traits}.rs` were collapsed and
  reworked into `stream/operator.rs` + new `stream/operator_logic.rs`,
  `stream/operator_context.rs`, `stream/build_context.rs`, `stream/stream_builder.rs`;
  old `stream/builder.rs` deleted
- New `DirectLogic` / `Operator` / `WorkerBuildContext` types (used by the new worker)

### 6. Channels infrastructure
- New: `channels/alignment.rs` (barrier/alignment), `channels/signal.rs` (`SignalHandle`),
  `channels/recv_trait.rs`; heavy rework of `spsc.rs` and `operator_io.rs`
- Old `operator_io.rs` preserved as `operator_io copy.rs` (migration reference)

### 7. Stateful sources & sinks
- `sources/stateful.rs` rewritten (627 lines changed; old version kept as
  `stateful_old.rs`); `sources/stateless.rs` converted to produce `StatefulSource`s
  (`stateless.rs:129` — the exact spot where the current compile errors originate)
- `sinks/stateful.rs` reworked (223 lines changed)

### 8. New proc-macro crate: `malstrom-macros`
A new workspace member providing `#[derive(TTLState)]` with a `#[timestamp_type(T)]`
attribute — wraps every field of a struct as `Option<(FieldType, T)>` with generated
`expire(epoch)` / `is_empty()` logic (used by the reworked TTL map operator).

### 9. Operators & examples
- New `union` operator (`operators/union.rs`); `joining_streams.rs` example renamed →
  `union_streams.rs`
- `ttl_map_example.rs` → `ttl_map.rs`; all ~25 examples updated to the new API
- Operator internals rewritten for async: `stateful_op.rs` (401 lines changed),
  `stateless_op.rs`, `map/filter/filter_map/flatten/split/inspect/cloned`, the time
  operators (`assign_timestamps`, `generate_epochs`, `inspect_frontier`), and a new
  `com_utility.rs`

### 10. Docs & k8s/kafka compatibility
- New docs: `Agents.md` (coding style guide), `Status.md` (author's personal status notes),
  `website/internals/KvtTrait.md`
- `malstrom-k8s` / `malstrom-kafka` received small API-compat fixes to keep compiling
  against the changed core (`coordinator_backend.rs`, `transport.rs`, `worker_backend.rs`,
  `record.rs`, `sink.rs`)

---

## Current state & next steps

- **Broken:** 20 compile errors in the core lib, centered on `StatefulSource` /
  `sources/stateful.rs` and the stateless→stateful source conversion in
  `sources/stateless.rs`. The last commit ("fixed stateful source") is exactly this area,
  so the fix was in progress but incomplete.
- **In-flight migrations:** old code is deliberately kept alongside new code
  (`keyed_old/`, `stateful_old.rs`, `state_old.rs`, `operator_io copy.rs`), so parts of the
  branch are scaffolding/reference rather than finished work.
- **Natural next step:** finish the stateful-source trait wiring so `malstrom` compiles,
  then verify the multi-thread runtime (noted broken in early commits), and eventually
  delete the `*_old` references and merge.
