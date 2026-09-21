# Agent Note: Unify operator IO behind a symmetric edge abstraction

Status: proposed

## Problem

Malstrom currently has two parallel edge implementations, chosen by locality:

- **Local inter-op edges (same worker)** are hardwired to the concrete
  `channels::spsc` type in `Output`/`Input`
  (`Vec<Rc<spsc::Sender<Message<M>>>>`, `IndexMap<…, spsc::Receiver<…>>`). The channel is
  documented as unbounded but is actually bounded at a hidden `CAPACITY = 1024`, and it has
  a receiver-liveness/backpressure gap: `Receiver::drop` does not wake a sender parked on a
  full queue.
- **Cross-worker edges** are `flume::bounded(1024)` behind `StreamSender`/`StreamReceiver`
  traits, entering operators through `OperatorCommReceiver`, which implements
  `recv_trait::Receiver`.

The input side has a common trait (`recv_trait::Receiver`); the output side does not.
Local output is concrete `spsc`, remote output is trait-based. Consequences:

- Moving an operator to another worker changes the edge type, and therefore the IO code
  path, even though operator semantics are identical.
- A new transport (TCP, k8s pods, IPC, shared memory) needs its own fork of the IO layer or
  another adapter seam per side.
- The `recv_trait::Receiver` TODO (`do we still need this trait?`) is unresolved because the
  trait is only half the picture: there is no symmetric sender side.

Two existing systems show the target shape. Timely Dataflow has one logical edge
abstraction — `Push`/`Pull` endpoints created by an `Allocate` trait — so thread,
in-process multi-thread, and TCP multi-process transports are pluggable and transparent to
operator code. Arroyo (Rust/tokio) reaches the same goal differently: every edge is the
same concrete `BatchSender`/`BatchReceiver`, and remote edges are relays between two local
channels. Malstrom's goal of "same operator code, different placement" is the same goal,
expressed in its own runtime (epochs, barriers, `Message` protocol). This note borrows the
*shapes* of both designs; reusing either codebase is explicitly out of question
(see [Proposal §4](#4-no-code-reuse-from-timely-or-arroyo)).

## Proposal

Introduce one edge abstraction for all operator IO: an `Allocate`-style factory returning
symmetric async `Push`/`Pull` endpoints, with concrete transports chosen by placement.
Interface from Timely, implementation from Arroyo, no code reuse from either:

1. **One edge identity: symmetric async `Push`/`Pull`.** Widen `recv_trait::Receiver` into
   a symmetric sender/receiver pair (i.e. adopt `Push`/`Pull`-style traits in malstrom's
   async-futures model — not Timely's reference-borrowing contract). `Output::send` and
   `Input::recv` become transport-agnostic; the trait TODO resolves by *widening* the
   trait, not deleting it. The `spsc` channel itself remains, initially, as one
   implementation behind that interface.

2. **Interfaces first, implementations later.** The trait must be designed against the
   second implementation's semantics (bounded async send, receiver-liveness via a
   `closed()`-style signal) so later implementation swaps are pure churn, not interface
   churn. Initial transports:
   - *Local (baseline)*: the **existing `spsc`, adapted** behind a thin wrapper exposing
     the new contract, so behavior is unchanged while the interface lands.
   - *Local (later)*: a **budget-bounded tokio mpsc** channel — Arroyo's pattern
     (`unbounded_channel` + atomic row/weight budget + watch-style `Notify`), plus the
     drop-waker fix malstrom requires. Tracked as a follow-up implementation milestone in
     [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md).
   - *Cross-worker (baseline)*: keep `flume::bounded(1024)` (async send/recv, disconnection
     detection) behind the same trait.
   - *Cross-worker (later)*: an **Arroyo-style relay between two local channels** — an
     out-link drains the source side's local `rx`, encodes via `Message::encode`, writes
     TCP; an in-link decodes and feeds the destination's local `tx`. If malstrom writes its
     own TCP transport, multiplex all channels of a worker pair over one connection
     (Timely-style `MessageHeader`), not per-edge sockets like Arroyo.

3. **Allocator-like factory.** A small `Allocate`-style factory constructs the transport
   for each edge from worker topology (same worker vs different worker), mirroring Timely's
   `Thread`/`Process`/`Cluster` allocators. The factory returns typed endpoints
   (e.g. `allocate(id) -> (Vec<Push<T>>, Pull<T>)`) and a `broadcast` adapter as a generic
   one-to-many push, so single-worker, multi-thread multi-worker, and distributed modes
   differ only in channel construction — not in operator logic.

4. **No code reuse from Timely or Arroyo.** We adopt the *shapes* only; neither
   `timely_communication` nor `arroyo-*` becomes a dependency, and wholesale adoption is
   out of question:
   - `timely_communication` couples to a synchronous push/pull-borrow model, capability/
     progress machinery, and zero-copy `Bytes` slabs that conflict with malstrom's async
     futures and epoch/barrier runtime.
   - Arroyo's channel code lives in unpublished, Arrow/DataFusion/metrics-coupled crates;
     it lacks a waitable receiver-gone future and papers over the blocked-sender-on-drop
     gap with background draining.
   - The borrow is cheap in dependencies: `tokio::sync::mpsc` and watch-style `Notify` are
     already in malstrom's dependency set.

5. **Chaining later.** Only physical edges that must exist become channels. Merge
   chainable operator runs to drop queues entirely (Arroyo 0.13+), as an independent
   follow-up after the trait and transports land.

## Field survey: Timely vs Arroyo

Both systems were read from source (`timely_communication`; `arroyo-operator`/
`arroyo-worker`). Neither will be reused as code; they are surveyed for shapes.

### Timely (`timely_communication`)

- **Interface:** `Push<T>` (`push(&mut Option<T>)`, `send(T)`, `done()`) and
  `Pull<T>` (`pull() -> &mut Option<T>`, `recv()`) endpoints created by an `Allocate`
  trait: `allocate<T>(id) -> (Vec<Push<T>>, Pull<T>)`, plus `broadcast`, `events`,
  `await_events`, `receive`, `release`. Serialization boundary is `Bytesable`.
- **Local:** `Thread` allocator = `VecDeque` behind `Rc<RefCell>` (sync, zero-copy borrow);
  `Process` allocator = `std::sync::mpsc` channels.
- **Remote:** `Cluster` = one TCP connection per worker pair with a 48-byte `MessageHeader`
  (channel/source/target_lower/target_upper/length/seqno), dedicated send/recv threads,
  zero-copy `Bytes` pipes.
- **Backpressure** is not in the channel: the scheduler drives `await_events`/`receive`/
  `release` and drains communication events.

### Arroyo (`arroyo-operator`)

- **One channel type for every edge.** All physical edges — forward, shuffle, joins — are
  `BatchSender`/`BatchReceiver` pairs from `batch_bounded(queue_size)`; locality does not
  change the queue type. The pair wraps `tokio::sync::mpsc::unbounded_channel()` with a
  shared row budget (`Arc<AtomicU32>`), a byte counter, and a `tokio::sync::watch`-style
  `Notify`: `send().await` blocks while the rows-in-flight budget is exhausted, `recv()`
  decrements and notifies waiters. Backpressure bounds **rows** (`queue_size` is an
  explicit worker config), not messages.
- **Remote edges are relays between two local channels.** The graph on every worker still
  contains the full local `(tx, rx)` pair per edge. For a remote target, an outbound
  `OutNetworkLink` drains the source worker's local `BatchReceiver` and writes framed Arrow
  IPC over TCP; an inbound `InNetworkLink` on the destination decodes and pushes into the
  local `BatchSender`; the downstream operator reads its own `BatchReceiver`. Backpressure
  composes end-to-end: slow destination fills the dest `tx` → TCP stalls → the out link
  stops draining the source `rx` → the source `tx` fills → the source operator's
  `send().await` blocks.
- **Signals flow in-band.** Checkpoint barriers/watermarks are `ArrowMessage::Signal` on
  the same queues and the same TCP with a message-type flag — the analogue of malstrom's
  `Message::AbsBarrier` riding the data channel.
- **Gaps vs malstrom's requirements.** Sender-side shutdown is `tx.is_closed()` →
  `SendError`, not a waitable receiver-gone future; Arroyo drains leftover queues in
  background tasks after stop to avoid deadlocks — malstrom's `closed()`/`no_receivers()`
  contract is a superset. Outbound TCP is effectively one connection per `Quad` (logical
  remote edge), not one per worker-pair like Timely's `Cluster` allocator.
- **Chaining removes queues.** Arroyo 0.13+ merges chainable operators to drop queues
  entirely (memory and task-count win): only physical edges that must exist become
  channels.

### What to adopt / not adopt

Adopt:

1. Timely's `Allocate`-factory shape + `broadcast` adapter (mapped to async futures).
2. The symmetric `Push`/`Pull` pair, as async traits — resolving the `recv_trait::Receiver`
   half-trait by widening it.
3. Arroyo's budget backpressure (`unbounded_channel` + atomic budget + `Notify`), with the
   receiver-drop wakeup malstrom requires.
4. Arroyo's network-as-a-relay design as the future uniform remote transport (flume stays
   as the baseline adapter until then).
5. In-band control signals on the data channel — validates malstrom's `Message::AbsBarrier`
   design; no code change implied.
6. Chaining as a later "fewer edges" optimization, not a channel change.

Do not adopt:

1. `timely_communication` or `arroyo-*` code/crates (explicitly out of question).
2. Timely's `await_events`/`receive`/`release` scheduler loop and zero-copy `Bytes` slab
   allocators — built for a synchronous event loop, not async futures.
3. Timely's reference-borrowing `push(&mut Option<T>)` / `pull() -> &mut Option<T>`
   contract.
4. Arroyo's per-`Quad` TCP connections — prefer worker-pair multiplexing if malstrom writes
   its own TCP transport.

## Alternatives considered

- **Status quo (two parallel edge types)** — least churn; costs: placement changes the edge
  type, transports are not swappable, and the `spsc` maintenance + liveness bug remain.
- **Narrow swap first: `spsc` → tokio mpsc before the interface exists** — rejected as the
  ordering. It would touch `Output`/`Input` once for the swap and again for the trait, and
  would still leave the output side concrete and the remote side trait-based. Interface
  first makes the swap a localized implementation change; the swap is tracked as a
  follow-up in
  [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md).
- **Adopt `flume` for everything** — one crate for local and remote; but local edges lose
  the `Rc` fast path, flume has no `closed()` wait future, and it still does not give a
  single abstraction with pluggable transports (it *is* the transport).
- **Wrap `timely_communication` now** — rejected: reuse is out of question. It couples to
  Timely's allocator/lifetime/progress model, which conflicts with malstrom's
  epoch/barrier runtime and async futures.
- **Adopt Timely Dataflow wholesale now** — rejected: reuse is out of question; it is an
  architecture replacement (runtime, progress model, scheduling, scopes, operators), not an
  IO unification.
- **Vendor/fork Arroyo's channel code (`batch_bounded`)** — rejected: unpublished and
  coupled to Arrow/DataFusion/metrics; it lacks a waitable `closed()` and the
  blocked-sender-on-drop wakeup malstrom requires. The useful part is a ~60–100-line
  pattern already implementable with malstrom's existing `tokio::sync::mpsc` + `Notify`
  dependencies.
- **Network-as-a-relay from day one instead of a flume adapter** — attractive end-state,
  but it means writing/replacing a cross-worker transport while the trait still lands;
  flume already exists today and exercises the trait's remote semantics first. Relay is the
  later transport, not the first.

## Acceptance criteria

- All operator edges — local and cross-worker — are created through the same trait/factory;
  no operator code branches on transport.
- `recv_trait::Receiver` is widened into a symmetric sender/receiver pair; the
  `TODO: do we still need this trait?` is resolved by the widening.
- The local channel's *interface* is defined; the initial implementation may remain the
  existing `spsc` behind an adapter (behavior unchanged). The tokio mpsc swap is a
  follow-up, evaluated independently in
  [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md).
- **No `timely_communication` or `arroyo-*` dependency is added; adoption is shape-only.**
- Existing tests pass unchanged (`cargo test --workspace`): fan-out, broadcast ordering,
  barrier alignment, frontier/epoch emission, bounded backpressure, and downstream
  termination.
- A placement-transparency test runs the same operator code twice — wired once with local
  channels and once with cross-worker channels (in-process multi-worker) — and observes
  identical semantics. The same test is expected to pass unchanged against a later relay
  transport.
- The unified abstraction exposes receiver-liveness (a `closed()` signal on the sender
  side) so `no_receivers()` continues to return an owned `'static` future that resolves on
  every transport; the `ReceiverGone`/blocked-sender-on-drop edge tests are owned by the
  local-channel swap note.
- `broadcast` is expressed as a generic adapter over ordinary pushes (Timely-style) unless
  explicitly deferred in the implementation milestone.

## Risks

- **Abstraction overhead.** A generic edge trait with dynamic dispatch can cost against the
  current concrete `spsc`; keep the local transport typed where possible (generic
  allocator interface, concrete channel implementations).
- **Trait design churn.** Widening `recv_trait::Receiver` touches `AlignmentGroup`,
  `Input`, `Output`, `OperatorCommReceiver`, and every operator builder. High blast radius
  — requires staged migration.
- **Backpressure semantics.** The unified abstraction must preserve bounded async blocking
  on both local and remote edges and define receiver-drop behavior once (tokio `SendError`,
  flume disconnection) rather than twice.
- **Shape-vs-semantics drift.** Borrowing `Push`/`Pull`/`Allocate`/relay *shapes* without
  their machinery risks copying synchronous or zero-copy semantics that don't fit malstrom.
  Each borrowed shape must be mapped to async futures and malstrom's epoch/barrier/`Message`
  model, not copied blindly.
- **Scope creep.** "Unification" can balloon into a full IO-layer rewrite; the milestones
  (edge trait + adapters → allocator factory → local-channel swap → optional relay/chaining)
  must each land independently and reversibly. The local-channel swap is postponed until
  after the interface exists and is tracked in
  [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md).