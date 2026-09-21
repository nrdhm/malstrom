# Agent Note: Replace operator IO's local spsc channel with tokio mpsc

Status: proposed

> This is the **follow-up implementation milestone** of
> [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md):
> the unified edge interface is defined **first**, with the existing `spsc` adapted behind
> it (no behavior change). This note then swaps the local transport to tokio mpsc as a pure
> implementation change — no interface churn. Do not start this work before the edge trait
> lands.

## Problem

`malstrom-core/src/channels/spsc.rs` is a hand-written async SPSC channel
(`Rc<RefCell<SharedInner<T>>>`, waker plumbing, `ReceiverGone` future) used as the local
edge between operators on the same worker. Its module doc calls it unbounded, but it is
actually bounded at `CAPACITY = 1024`: `Send::poll` parks a producer when the queue is
full. Excluding tests it is small, but it reimplements what mature channel crates already
provide, and its three reasons for existing are now partly obsolete:

1. **Receiver-liveness** — `Output::no_receivers()` needs a future that completes when a
   downstream receiver is dropped, so the operator loop can exit once nobody will read the
   output anymore. `tokio::sync::mpsc::Sender::closed()` provides exactly this signal and
   `tokio` is already a dependency.
2. **Single-thread fast path** — the channel uses `Rc` (no atomics) because local operator
   edges live inside the worker's single-thread runtime. That is an optimization, not an
   API requirement.
3. **Backpressure shutdown gap** — when a sender is parked on a full queue and the receiver
   is dropped, `Receiver::drop` wakes only the `receiver_gone` waker, not the parked
   `send_waker`. A bounded queue can therefore leave an upstream operator stuck forever
   exactly when backpressure and downstream termination collide.

The liveness is also **one-sided**, and this has already caused a real hang. There is no
sender-gone signal: `spsc::Sender` has no `Drop` impl, so dropping the last sender of an
edge wakes nobody, and `Receive`'s `Output = T` (never `Option<T>`) cannot express "closed".
An operator whose input's sender was dropped therefore parks in `recv().await` forever and
the dataflow hangs instead of failing. This was hit when a
[stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md) change in
`union()` linked an edge but forgot to register it, dropping the sender; the sink then
blocked on an edge with no sender and the test hung. Fixing *that* wiring mistake and
preventing the whole class are tracked separately in
[fail-loud-on-dangling-operator-edges](2026-09-19-fail-loud-on-dangling-operator-edges.md),
which **depends on this note**: it needs the sender-gone/liveness primitive that the tokio
mpsc swap provides via `Sender::closed()`, and it owns the `Option<T>` vs `pending()`
decision this note raises.

The surrounding `recv_trait::Receiver` trait already carries a
`TODO: do we still need this trait?`, and `AlignmentGroup` (barrier alignment), `Input`
(frontier/epoch merging), and `Output` (partitioning, close signal) are the parts that
actually encode stream-processing semantics. Those stay custom in this milestone. The
channel beneath them does not have to be.

## Proposal

**Prerequisite:** the unified edge trait from
[unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md)
exists, with `spsc` adapted behind it as the baseline local transport. This proposal swaps
that baseline implementation.

Replace `spsc` with a **budget-bounded tokio mpsc** channel for local operator IO — the
synthesis adopted in
[unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md)
(interface from Timely, implementation from Arroyo, no code reuse from either). The target
shape is Arroyo's `batch_bounded`: `tokio::sync::mpsc::unbounded_channel()` guarded by an
atomic slot budget and a watch-style `Notify`, sized in messages rather than Arrow rows.
A simpler first slice — `tokio::sync::mpsc::channel(capacity)` (bounded) — is acceptable if
the budget wrapper is not ready; both satisfy the same trait contract, and the budget is
layered on later without interface churn. Keep the domain machinery untouched:

- `Output<M>::senders` becomes `Vec<tokio::sync::mpsc::Sender<Message<M>>>`.
  `Sender: Clone`, so the `Rc<spsc::Sender<…>>` wrapper and the hand-rolled
  receiver-gone future disappear:
  `no_receivers()` = `join_all(senders.iter().cloned().map(|s| async move { s.closed().await }))`.
- Each edge gets a slot budget, defaulting to the current 1024 until a per-edge/global
  capacity policy is chosen. `send().await` blocks while the budget is exhausted, which
  propagates pressure through the operator loop exactly as today's `CAPACITY = 1024`
  `spsc` does. Receiver-drop behavior must release a blocked sender in all cases —
  Arroyo's own queue only wakes on `recv()` and papers over the gap with background
  draining; malstrom requires the explicit drop-waker fix, matching the bounded
  channel's built-in behavior.
- `Output::send` must tolerate `Sender::send`'s `Err(SendError)` (receiver gone) by
  dropping the returned message, matching today's drop-when-no-receiver behavior.
- `Input<M>`'s receiver side becomes `tokio::sync::mpsc::Receiver<Message<M>>`.
  Add a thin `recv_trait::Receiver` impl (or adapter struct) so `AlignmentGroup` and
  `Input::recv` are unchanged. tokio mpsc `recv()` returns `Option<T>`; map `None` to
  `pending()` to preserve today's "an edge never yields None" behavior, or fail loudly —
  decide explicitly in implementation.
- Keep `AlignmentGroup`, `Input` frontier merge, `Output` partitioner/`ClosedSignal`, and
  the `Message` protocol exactly as-is.
- Keep `flume` for cross-worker/coordinator communication as-is; this proposal is only about
  the local operator-edge channel.

## Alternatives considered

- **Keep `spsc` (status quo)** — zero churn, preserves the `Rc`/no-atomic local path and
  the existing regression tests (`parked_receiver_wakes_on_send`,
  `sending_without_receiver`, `is_spsc`). Costs: a custom channel to maintain, a custom
  liveness future, the `Rc` wrapper in `Output`, and — before bounded backpressure is
  trustworthy — fixing the missing `send_waker` wakeup on receiver drop.
- **`flume::bounded`** — already used elsewhere in the repo for cross-worker edges and a
  reasonable choice if unifying local + remote channels on one crate; but it has
  `is_disconnected()` without a built-in "wait until receiver dropped" future, so
  `no_receivers()` would need an adapter, and it has no reserve/permit API for
  credit-style backpressure. Slightly weaker fit than tokio mpsc for this use case.
- **`local-channel`** — the crate `spsc.rs` cites as inspiration; non-threadsafe and
  **unbounded only** (`send()` never blocks), so it provides no backpressure at all and
  cannot replace the bounded behavior we need. Not a candidate.
- **`futures::channel::mpsc` or `async-channel`** — async channels, but neither exposes the
  receiver-gone signal as directly as `Sender::closed()`; both would require extra adapter
  machinery.
- **Replace `recv_trait::Receiver` with `futures::Stream`** — cleaner long-term abstraction,
  but it changes `AlignmentGroup` and the `OperatorCommReceiver` impl too, widening the
  blast radius. Defer to the unification note; a thin `recv_trait` adapter keeps this
  milestone minimal.
- **Skip to the unified edge abstraction wholesale** — that is the prerequisite, and it
  lands **before** this note (see
  [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md)):
  the edge trait + adapters are defined there, with `spsc` kept as the baseline local
  implementation. This note is deliberately narrower: it only swaps the local transport
  behind that already-defined interface.

## Acceptance criteria

- `cargo test --workspace` is green, with no behavior change in operator tests:
  fan-out/broadcast ordering, barrier alignment, frontier/epoch emission, downstream
  termination (`no_receivers`), and bounded backpressure all behave as before.
- `spsc.rs` and the `Rc<spsc::Sender<…>>` wrapper in `Output` are removed; the custom
  `ReceiverGone` future is gone.
- `no_receivers()` still returns an owned `'static` future that resolves when every
  downstream receiver is dropped (now via `Sender::closed()`).
- No new dependency: `tokio`'s `sync` feature is already enabled in `malstrom-core`.
- A regression test covers the liveness edge that previously hung: an output whose receiver
  is dropped before the liveness future's first poll must now terminate rather than hang
  (explicitly documenting the behavior change from the old `ReceiverGone` edge).
- Backpressure tests: (a) a full edge parks the producer until the receiver drains an item;
  (b) a producer blocked on a full edge is released when the receiver is dropped — the case
  the current `spsc` gets wrong.
- Capacity is bounded per edge (default 1024 or a chosen policy) and is no longer a hidden
  global constant.

## Risks

- **Semantic widening:** tokio mpsc `Sender` is `Clone` and thread-safe, so the SPSC
  invariant becomes a convention rather than an enforced type property. Current code sends
  only from the worker's single-thread runtime, so behavior is unchanged, but the type
  system no longer prevents a future accidental concurrent producer.
- **`Option<T>` vs `T`:** tokio mpsc `recv()` returns `None` when all senders are dropped;
  today's channel never yields `None`. The adapter must decide the semantics (pending vs
  panic) before this lands.
- **Capacity tuning:** switching from a hidden 1024 up front exposes per-edge buffer sizing
  (memory vs latency vs head-of-line blocking); this proposal only preserves the current
  default, it does not invent a global policy.
- **`SendError` handling:** tokio bounded `send()` returns the message back when the
  receiver is gone; `Output::send` and `no_receivers()` must be audited so this cannot
  deadlock or leak messages at shutdown.
- **Cross-worker backpressure unchanged:** this proposal only covers local edges; pressure
  across workers still flows through `flume::bounded(1024)` in the runtime communication
  layer and would need its own policy if unified later.
- **Performance:** `Arc`/atomics replace `Rc`/`RefCell` per message; the local single-thread
  path pays slightly more. Likely negligible for operator-scale workloads, but a targeted
  micro-benchmark is warranted if hot-loop regressions matter.
- **Blast radius:** `Input`/`Output`/`AlignmentGroup` are used by every operator; the
  adapter must be total and well-tested before deletion of `spsc`.

## Related

- [fail-loud-on-dangling-operator-edges](2026-09-19-fail-loud-on-dangling-operator-edges.md) — **dependent**: turns a sender-dropped edge from a hang into a named panic; needs this note's sender-gone/liveness primitive.
- [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md) — prerequisite interface this note swaps the transport behind.
- [stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md) — the refactor whose dropped `add_operator` call caused the hang cited in `## Problem`.