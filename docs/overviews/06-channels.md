# Channels

> **Last refreshed:** 2026-09-06

The transport types used to move data and control signals through `malstrom-core`. Channels
are split into two groups: the stream-data edges in [`channels/`](../../malstrom-core/src/channels/mod.rs)
(same-worker operator edges) and the `tokio::sync` + `flume` channel families used for
control plumbing and cross-thread communication.

## Same-worker data plane (`channels/`)

These are the edges between stream operators **on the same worker** — the stream graph
itself.

| Type | File | Role |
|---|---|---|
| `Sender<T>` / `Receiver<T>` | [`channels/spsc.rs`](../../malstrom-core/src/channels/spsc.rs) | The low-level bounded SPSC queue (`CAPACITY = 1024`). Async future-based `send()`/`recv()` with wakers; backpressure via `send_waker`/`recv_waker`; receiver-gone detection (`has_receiver`, `ReceiverGone`). `Send::poll` drops the message when no receiver exists (terminal-sink case). Created with `spsc::unbounded()`. |
| `Output<M>` / `Input<M>` | [`channels/operator_io.rs`](../../malstrom-core/src/channels/operator_io.rs) | The operator-facing channel types, built on `spsc`. `Output` fans out to N spsc senders via an `OperatorPartitioner` (data) or broadcast (system messages), tracks its frontier, and carries a `closed_signal` (`watch::Sender<bool>`). `Input` fans in from N spsc receivers, aligns barriers, and merges epochs into a frontier. `link()` creates one spsc edge and wires `Output` ↔ `Input`. |
| `AlignmentGroup<K, R, F>` | [`channels/alignment.rs`](../../malstrom-core/src/channels/alignment.rs) | A receiver combinator, not a channel. Wraps N receivers plus a condition (e.g. "is this a barrier?"); pauses channels whose message matches, emits `AlignedValue::Aligned(...)` only once **all** channels have paused. This is how operator `Input`s synchronize barriers across multiple upstream edges. |
| `Receiver` trait | [`channels/recv_trait.rs`](../../malstrom-core/src/channels/recv_trait.rs) | The small async `recv()` abstraction implemented by both spsc `Receiver` and `AlignmentGroup`. Carries a `TODO: do we still need this?` and a commented-out `IndexMap` impl. |
| `Signal` / `SignalHandle` | [`channels/signal.rs`](../../malstrom-core/src/channels/signal.rs)  | Internal watch-based signal built on `watch::channel(bool)`. Largely vestigial — its tests are commented out and `SignalHandle` is imported but unused in `operator_io.rs`. |

## Control and cross-thread channels (outside `channels/`)

| Type | Used for | Where |
|---|---|---|
| `tokio::sync::watch` | Last-value / state channels — `closed_signal`, `CoordinatorApi` handle, worker `completion` | `operator_io.rs`, `runtime/threaded/multi.rs`, `worker/{worker,coordination_task}.rs` |
| `tokio::sync::oneshot` | Request/response rendezvous — coordinator API calls, worker↔coordinator req/res responders, watchmap notifications, snapshot completion callbacks | `coordinator/{api,watchmap,cluster}.rs`, `runtime/threaded/communication/*`, `snapshot/mod.rs`, `types/distributed.rs` |
| `tokio::sync::mpsc` | One-shot callback channels in tests and snapshot barrier callbacks | tests, `operator_io.rs` tests |
| `tokio::sync::broadcast` | Fan-out of build context from worker to its threads | `worker/{builder,worker}.rs` |
| `flume::bounded(1024)` | Cross-thread (inter-worker) communication in the threaded runtime — operator↔operator and worker↔coordinator, keyed by `ConnectionKey`. Operator channels carry `Vec<u8>` (serialized); coordinator channels carry `(Vec<u8>, oneshot::Sender<Vec<u8>>)` for the response half | `runtime/threaded/communication/inter_thread.rs` |

## Layering summary

- **Same-worker data plane:** `spsc` (queue) wrapped by `Output`/`Input` → `link()` edges,
  with `AlignmentGroup` providing multi-edge barrier alignment.
- **Cross-worker data plane:** `flume::bounded` channels carrying serialized bytes between
  worker threads.
- **Control/synchronization:** `watch` (state), `oneshot` (rendezvous), `broadcast`
  (fan-out), `mpsc` (callbacks).

## Notes

- `spsc.rs`'s module doc still says *"A dead simple **non-threaded** unbounded channel"*,
  but the implementation is now bounded (`CAPACITY = 1024`) with backpressure — the comment
  is stale.
- `link()` keys receivers 0-based so they line up with `Input::frontiers`; this assumption
  breaks if a receiver is ever removed after linking (see the `multi_input_epoch_merges_with_zero_based_frontiers`
  regression test).