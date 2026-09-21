# Agent Note: Fail loud on dangling operator edges

Status: proposed

## Problem

A stream can be wired so that an operator's input has **no live sender**: the edge's
receiver is kept, but the sender is dropped at the end of the builder's scope. Nothing in
the runtime detects this. The dataflow then neither completes nor fails — it hangs.

This was hit directly. The
[stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md) rewrote
`union()` on the new `OperatorBuilder` helpers and, in doing so, dropped the
`add_operator(edge)` call that the previous implementation had. Each fan-in edge was built,
its tail swapped and linked, and then dropped without being registered:

```rust
let mut edge = OperatorBuilder::new(format!("{}-0", name).into())
    .with_direct_logic(Forward::new().into_logic())
    .build();
self.swap_tail(&mut edge.input);
edge.link_to_input(&mut united_input); // creates the spsc pair: tx in edge.output, rx in united_input
// `edge` is dropped here → its Output (the tx) is dropped with it
```

The observable consequences, in the affected `union_unites` test:

- No `fan-in`/`sink` operator spans exist; only `source-*` run.
- All six source operators exit via the `output_closed.wait_for` branch (`no_receivers`
  never fires), because their outputs now have no receivers.
- The `sink` parks forever in `Input::recv()` on `united_input`, whose only sender was
  dropped, and never sees the terminating MAX epoch.
- `operator_rt.block_on(join_all(tasks))` in `worker::execute` never resolves, so
  `Finished execution` is never logged. The test hangs until the harness timeout instead of
  failing with the wiring error.

Four properties of the current edge layer combine to convert a static wiring mistake into
an unbounded hang:

1. **A receiver is never told its senders vanished.** `channels::spsc::Receiver` has a
   `Drop` that wakes a `receiver_gone` waker, but `spsc::Sender` has no `Drop` at all.
   Dropping the last sender wakes nobody. `Receive::poll` on an empty queue simply
   re-registers `recv_waker` and returns `Pending` — permanently.
2. **`Receive` cannot express "closed".** Its `Output` is `T`, never `Option<T>`. The
   sender-gone condition is already knowable inside `Receive::poll` (the
   `debug_assert!(Rc::strong_count(&self.0.shared) <= 2)` is exactly "no sender left"), but
   it can only fire as a `debug_assert` on a poll that may never happen, and it is compiled
   out in release. It cannot terminate a loop.
3. **`no_receivers()` is deliberately blind to this.** `Output::no_receivers` returns
   `pending()` when `senders.is_empty()` (documented: "an unlinked output, like a sink's,
   never resolves"), and `spsc::ReceiverGone::poll` returns `Pending` forever if the
   receiver was already gone on its first poll. Both point the same way: **a dead edge is
   indistinguishable from a live idle one.**
4. **No build-time assertion.** `InnerRuntimeBuilder` keeps only `JoinHandle`s, so nothing
   validates that every registered operator's input actually has a live sender before the
   worker starts.

The combination is the real defect: the graph can be structurally incomplete, and the
runtime's liveness machinery is one-sided (receiver-gone only) and non-failing.

## Proposal

Make an incomplete or non-progressing operator graph **fail loudly**, at three layers, so
a wiring mistake is a named panic instead of an unbounded hang.

1. **Build-time operator-graph validation.** Before a worker starts executing, validate the
   assembled graph and panic naming the offending operator and edge when an operator's
   input has no live sender or a started operator's output has receivers but no sender. The
   prerequisite is for `InnerRuntimeBuilder::add_operator` to retain enough information to
   inspect the graph at `execute()` — either the operators themselves (until start), or a
   small per-edge liveness registry (live sender/receiver counts) updated on `link` and on
   endpoint drop. The panic message must name the operator (`union_unites`'s missing
   `fan-in-0`, not "a task hung"), because the whole cost of this incident was the gap
   between the symptom (a hang) and the cause (an unregistered builder).

2. **Sender-gone liveness in the edge channel.** Mirror the existing receiver-gone signal:
   add a `sender_gone` wait and wake it from `Drop for Sender`, and expose it on the receive
   side so `Input::recv` can distinguish "idle" from "no senders will ever send again" and
   terminate or panic on the latter. This is the general fix for the class, not just for
   build-time wiring: an edge can also become dead at runtime (a sender dropped
   early). It requires deciding how the receiver observes the condition — an `Option<T>`
   return from `recv_trait::Receiver`, or a separate `closed()`/`is_sender_gone()` signal
   the operator loop selects on. That decision, and the channel that carries it, are owned
   by [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md);
   this note depends on it and does not re-open the transport choice.

3. **No-progress watchdog.** Wrap completion in a bounded no-progress timeout — for tests,
   `malstrom-testkit`'s `rt.execute()` should not be able to eat a CI run; for the runtime,
   the coordinator already polls every 5s and can reuse that tick. On expiry, panic with a
   dump of live operators and edges that have receivers but no senders. A bare timeout is
   strictly worse than a timeout that names the stalled operators.

Layers 1 and 3 catch the incident directly; layer 2 closes the general liveness gap and is
what makes a dangling edge detectable at all rather than merely time-out-able.

## Distinguishing a dangling edge from a legitimate open end

Not every edge without a sender is a bug, so the validation must not reject the graph's
legal shapes:

- **A tail receiver with no sender may be legal.** The terminal sink's output has a
  receiver that is dropped at build time; `Send::poll` deliberately drops messages when
  `!has_receiver` rather than blocking. That is "sender present, receiver gone" — the
  receiver-gone side, already handled.
- **The incident is the mirror:** "receiver present, sender gone". The rule to enforce is
  directional: an operator that is *started* and has an input must have at least one live
  sender for that input; an operator whose *output* has live receivers must retain its
  sender. A registered operator with a truly empty, never-linked input (e.g. a root
  operator driven purely by system messages) is the explicit exception and must stay
  allowed.

## Alternatives considered

- **Do nothing / rely on timeouts.** Cheapest, and the hang already surfaces eventually via
  the test harness. Costs: the failure is indistinguishable from a genuine slow test, gives
  no operator name, and a released binary would hang silently. Rejected — the diagnosis cost
  is exactly what this note exists to remove.
- **Only the build-time graph check (layer 1).** Catches this incident with no channel
  change, but misses a sender dropped at runtime and cannot be expressed without an
  ownership policy for retained operators. Kept as a component, not the whole.
- **Only the watchdog (layer 3).** General and cheap, catches every future deadlock, but
  reports a *time* not a *cause*, and only after the timeout. Kept as the backstop, not the
  primary.
- **Rely on the existing `debug_assert` in `Receive::poll`.** It already computes the
  sender-gone condition, but it fires only on a poll that may never come and is disabled in
  release. Promoting it to a real signal is essentially layer 2; a debug-only assertion is
  not a guarantee.
- **Panic inside `Send::poll` / `Receive::poll` on a dead edge.** Tempting and local, but
  well-behaved shutdown legitimately races sends against closed edges (see the drop branch
  in `Send::poll` and the closed-signal early return in `Output::send`); panicking there
  would turn benign shutdown races into crashes. The check belongs at build time and in a
  dedicated liveness signal, not in the hot send/recv path.
- **Make the whole edge stream-oriented (`futures::Stream`) now.** Would give end-of-stream
  semantics for free, but it is the wider change owned by
  [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md);
  this note stays within the current edge contract and its pending channel swap.

## Acceptance criteria

- Building an operator graph with a started input that has no live sender panics at
  `execute()` with a message naming the operator and the offending edge — a regression test
  reproduces the `union()` mistake (edge linked but never registered) and asserts the panic
  rather than a hang.
- The graph validation does not reject the legal shapes above: the terminal sink's
  build-time-dropped tail receiver, and a root operator with no linked input.
- The edge channel exposes a sender-gone signal to the receiver side, and `Input::recv` (or
  the operator loop) terminates or panics when every sender of a live edge is dropped; the
  SPSC/liveness tests for it live with the channel note
  ([replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)).
- A deadlocked or non-progressing execution fails within a bounded time, and the failure
  names the live operators and any receivers-without-senders edges.
- The existing passing suites (`union_unites`, the `rescale` and completion tests) are green
  after the fix, with no change to normal completion timing or message semantics beyond the
  added checks.

## Risks

- **False positives at shutdown.** Shutdown legitimately drops senders/edges. Validation
  must run at start (or otherwise exclude shutting-down operators), or it will panic on
  correct programs.
- **Ownership cost of retaining operators.** Layer 1 needs the builder to hold operators
  (or a liveness registry) until `execute()`; the registry adds bookkeeping on every `link`
  and endpoint drop. Keep it minimal and local to `InnerRuntimeBuilder`/`spsc`.
- **Dependency coupling.** Layer 2 cannot land before
  [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)
  fixes the transport-level semantics (`Option<T>` vs `pending`, `SendError` on receiver
  gone); if that note is re-scoped, this one's liveness guarantee moves with it.
- **Watchdog tuning.** Too short a no-progress timeout fails legitimately slow work; too
  long reintroduces the CI cost. The threshold must be configurable and driven by observed
  completion times.
- **Scope creep into the IO unification.** Layers 1 and 3 are independent of the transport
  swap and can land first; keep them from absorbing the
  [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md)
  work.

## Related

- [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md) — **dependency**: owns the edge-channel semantics layer 2 builds on.
- [stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md) — the refactor that introduced the dangling edge; its "builder misuse" risk is this incident realized.
- [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md) — the wider edge-layer direction.