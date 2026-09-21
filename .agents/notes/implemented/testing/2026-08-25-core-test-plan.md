# Agent Note: Test the `malstrom` core (kernel) for behavior and API stability

Status: implemented

## Problem

The kernel is the foundation every other crate builds on, and its public surface was just
widened by [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
— but almost none of it is pinned by tests. Coverage was 19 unit tests concentrated in
`channels` (14) and `coordinator/watchmap` (5):

| Module | Tests | Gap |
|---|---|---|
| `channels/operator_io`, `spsc`, `signal` | 7+4+3 | mostly covered |
| `coordinator/watchmap` | 5 | only `watchmap`; `coordinator`/`cluster`/`api`/`messages` untested |
| `stream/operator` | 1 | `hash_is_stable` only; `StreamBuilder`/`LogicBuilder`/`BuildContext` untested |
| `snapshot` | 1 | `NoPersistence` only; barrier protocol untested |
| `runtime/threaded/multi` | 1 | the new multi-worker test |
| `runtime/threaded/single`, `runtime/communication/*`, `runtime/runtime_flavor` | 0 | untested |
| `types/*` (`message`, `distributable`, `time`, `key`, `data`) | 0 | no serialization/ordering tests |
| `worker/*`, `coordinator/*` (except watchmap) | 0 | untested |
| `channels/alignment` | 0 | placeholder `todo!()` only |

There was no `malstrom-core/tests/` directory, so no test exercised the public API the way an
external consumer does. The eleven runtime bugs fixed earlier this cycle (see the
[collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md)
and [restore-new-scheduler-compilation](../../implemented/bug-fix/2026-08-22-restore-new-scheduler-compilation.md)
notes) each fixed behavior that had no test — every one of them could regress silently.

## Decision

The kernel is covered in five layers, ordered by value-per-effort. The through-line is:
tests that (a) pin the public extension API from the *outside*, (b) lock in the tricky
invariants future refactors will touch, and (c) act as regression guards for the bugs already
fixed. What shipped per layer is recorded in `## Testing` below.

### Layer 1 — public-API contract tests, split by which crate owns the surface

A contract test lives in the crate that *owns* the surface it exercises — the kernel can
only pin what the kernel exposes (a kernel dev-dep on `malstrom-operators` would cycle the
graph, per [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
Decision 8).

**1a. Kernel extension contract (`malstrom-core/tests/`)** — pins the seam the layer crates
build on, importing only the kernel crate (`malstrom_core::`) and never
`malstrom_operators`/`malstrom_distributed`:

- `tests/safe_logic_contract.rs` — a custom `Logic`/`SafeLogic` operator: `on_schedule` →
  `on_data`/`on_epoch`/`on_barrier` in the documented order, automatic system-message
  forwarding.
- `tests/completion.rs` — a kernel-only source emitting `Epoch(MAX)` terminates the runtime
  (pins the root/`no_receivers`/completion protocol).
- `tests/rescale.rs` — `MultiThreadRuntime::api_handle().rescale(n)` scales without deadlock.
- `tests/runtime_flavor_contract.rs` — a mock `RuntimeFlavor`/`OperatorOperatorComm`/
  `WorkerCoordinatorComm` implementation works (the seam `malstrom-k8s`/`malstrom-distributed`
  rely on).

**1b. Full public-API contract (`malstrom/tests/`)** — pins the end-user `malstrom::`
surface (kernel + operators + distributed re-exported under one namespace). This is the real
"public API predictability" anchor for users. Builds on the facade, which landed in
[rename-kernel-and-add-malstrom-facade](../../implemented/architecture/2026-08-25-rename-kernel-and-add-malstrom-facade.md):

- `tests/hello_pipeline.rs` — `malstrom::sources` → `malstrom::operators` → `malstrom::sinks`
  pipeline run on `SingleThreadRuntime` and `MultiThreadRuntime`, asserting deterministic
  output.
- `tests/namespace.rs` — every historical top-level path resolves (`malstrom::operators`,
  `malstrom::sources`, `malstrom::sinks`, `malstrom::keyed`, `malstrom::keyed::distributed`,
  `malstrom::runtime`, …), so a missing facade re-export fails at compile time.

### Layer 2 — unit tests for untested invariants

- `types/` — `Distributable` encode/decode round-trips for the primitive and container types;
  `Timestamp` merge + `CHECK_FINISHED` semantics; `Message`/`DataMessage` (de)serialization;
  `Kvt` tuple impls; `NoKey`/`NoData`/`NoTime` unit semantics.
- `stream/` — `StreamBuilder`/`Malstrom::then` links operators in order (output↔input wiring);
  `BuildContext`/`OperatorContext` expose the documented fields.
- `channels/` — `alignment.rs` (barrier held until all channels report, emitted once aligned),
  and the `Input::recv` epoch/frontier merge rules.
- `runtime/` — `RuntimeFlavor`/`OperatorOperatorComm`/`WorkerCoordinatorComm` contract via a
  local mock; `inter_thread` channel keying.
- `coordinator/` — `ClusterHandle` build/execute/reconfigure using a mock `WorkerCoordinatorComm`
  (the `CoordinatorClient`/`WorkerClient` req/res round-trip).
- `snapshot/` — `NoPersistence` + `serialize_state`/`deserialize_state` round-trip.

### Layer 3 — property-based tests (`proptest` re-added to the kernel dev-deps)

- `Distributable` round-trip: `proptest!(|v in …| assert_eq!(decode(&encode(v)), v))` over the
  types users will send (`u64`, `usize`, `String`, `Vec<u8>`, nested tuples).
- `Timestamp` merge: commutative, associative, idempotent, monotone (for the numeric impls).
- Partitioner/hash: `hash_op_name` stability (the existing `hash_is_stable` test is the model;
  `rendezvous_select`/`index_select` live in `malstrom-distributed`, a layer crate, out of
  scope for the kernel).

### Layer 4 — regression tests for the bugs already fixed

One named test per bug, so "fixed" stays fixed:

| Bug | Regression test |
|---|---|
| `ClosedSignal::wait_for` future-of-a-future | operator does not exit before applying; exits only after output closes |
| spsc waker inversion | a receiver parked on an empty channel wakes when a message is sent (no busy-poll) |
| `merge_frontiers` dropped the MAX epoch | single-worker source's `Epoch(MAX)` reaches the sink and closes it |
| `ConnectionKey` direction merge | coordinator↔worker and operator↔operator channels connect on the same key |
| 1-based vs 0-based frontier index | multi-input `Input::recv` epoch alignment doesn't panic |
| `ExecutionComplete` struct vs enum | coordinator `check_execution_complete` round-trips |
| `on_schedule` pump-until-idle | a source with no input emits until exhausted |
| root/`no_receivers` termination | job terminates (join_all returns) with no sink |
| sink swallowing MAX | sink closes on `Epoch(MAX)` |
| send-after-close | sending on a closed output is a no-op, not a panic |

### Layer 5 — doc tests

The public extension surface (`StreamBuilder`, `Operator`, `Logic`, `SafeLogic`,
`BuildContext`, `RuntimeFlavor`, `OperatorOperatorComm`, `WorkerCoordinatorComm`, `Message`,
`DataMessage`, `Distributable`, `Timestamp`) gets `///` examples. Doc tests are the cheapest
"public API is usable" check and they compile+run under `cargo test --doc`.

## Testing

Everything below ships and runs green as of 2026-08-25.

### Layer 1a

`malstrom-core/tests/` contains four contract-test files (importing only `malstrom_core::`,
no layer crates):

- `tests/common/mod.rs` — in-process mock of `OperatorOperatorComm` +
  `WorkerCoordinatorComm` + a `RuntimeFlavor` (`MemoryComm`/`MemoryFlavor`), used by
  `runtime_flavor_contract.rs`.
- `tests/safe_logic_contract.rs` — `SafeLogic` operator ordering (`on_schedule` →
  `on_data`/`on_barrier`/`on_epoch`) + automatic system-message forwarding.
- `tests/completion.rs` — kernel-only source emitting `Epoch(MAX)` terminates both runtimes;
  multi-worker parallelism doubles the values.
- `tests/runtime_flavor_contract.rs` — the comm/flavor seam round-trips (operator stream,
  coordinator req/res, flavor communication). The req/res test needs a concurrent sender —
  `receiver.recv()` before any send can never return.
- `tests/rescale.rs` — rescale 1→2 without deadlock, job still completes.

**Writing `rescale.rs` found three real kernel bugs, all fixed in this change:**

1. **Rescale sent the startup protocol to existing workers**
   `ClusterHandle::reconfigure` called `start_build`/`start_execution` for *all* workers; an
   existing worker's `CoordinationTask` only decodes the `RuntimeMessage` enum, so the
   tuple-struct `StartBuild` was mis-decoded as the enum and panicked, killing the coordinator
   loop (`Err(Stopped)` on the API handle). Fixed: bootstrap only the newly-added workers;
   existing workers learn the new scale via `RuntimeMessage::Reconfigure`. (The rescaling
   example previously "worked" only because its 2→2 rescale is a no-op and the fatal 2→1
   happened late.)
2. **Rescale never spawned the new worker**
   `MultiThreadRuntime::execute` used `threads.len()` (which includes the coordinator thread)
   as the current scale, so a rescale from P to P+1 spawned nothing and the coordinator's
   `start_build` blocked forever on the missing worker. Fixed: track `workers_spawned`
   separately.
3. **Terminal operator output blocked forever after 1024 messages**
   The spsc `Send::poll` queued into the bounded channel even after the receiver was dropped
   (its "sends without a receiver drop the message" doc was a lie; the
   `sending_without_receiver` unit test never polled the future, so it false-passed). The
   last operator's output tail receiver is dropped at build time, so after 1024 records the
   terminal's `send` blocked forever, stalling the whole pipeline (the source never polled
   its input, so the rescale handshake could not complete). Fixed: `Send::poll` drops the
   message when `has_receiver == false`, matching the documented intent. This is why the
   earlier tests (5 records, 2 records) never hit it.

### Layer 1b

`malstrom/tests/` (the facade, exercising only `malstrom::`):

- `tests/hello_pipeline.rs` — a `sources → operators → sinks` pipeline
  (`Source::from_iterator` → `map` → `VecSink`) on both `SingleThreadRuntime` and
  `MultiThreadRuntime` (parallelism 1), asserting the exact doubled sequence. The `build`
  closure must annotate its parameter (`|p: &mut dyn StreamProvider| …`) — an un-annotated
  closure does not satisfy the `FnOnce(&mut dyn StreamProvider)` HRTB bound, while a plain
  fn item does.
- `tests/namespace.rs` — every historical top-level path resolves through the facade (kernel
  modules + `keyed` incl. `keyed::distributed`, `operators`, `sinks`, `sources`), plus a
  smoke test touching `serialize_state`/`deserialize_state`. `malstrom::coordinator::api` is
  private — the facade re-exports `CoordinatorApi`/`ApiRequestError` at
  `malstrom::coordinator::` directly.

### Layer 2

21 new unit tests (unit count 19 → 40):

- `types/distributable` — encode/decode round-trips (primitives, `String`, `Vec<u8>`, nested
  tuples, `Kvt` tuples).
- `types/time` — `Timestamp::merge` laws (min for numerics, AND for `OnceTime`),
  `CHECK_FINISHED` only for MAX, `NoTime` semantics.
- `types/message` — `DataMessage` serde round-trip, `Kvt` impls, payload constructors.
- `channels/alignment` — replaced the `todo!()` placeholder: non-barrier passes through,
  barrier held until *all* channels report then emitted together, group recovers after
  alignment.
- `coordinator/messages` — `StartBuild`/`StartExecution`/`RuntimeMessage` wire round-trips
  (pins the format the coordination task decodes).
- `coordinator/cluster` — regression: `reconfigure_bootstraps_only_new_workers` with a mock
  `WorkerCoordinatorComm` + fake workers — worker 0 sees only `StartBuild, StartExecution,
  Reconfigure`; worker 1 sees the same; pre-fix, worker 0 was sent `StartBuild` again and
  this test fails with a decode panic.
- `snapshot` — `serialize_state`/`deserialize_state` round-trip.
- `stream/stream_builder` — `link` wires output→input (single and multi-input).

Also fixed a flake in `tests/safe_logic_contract.rs`: the operator loop polls the apply
branch before the output-closed branch, so one trailing `schedule` pump (yielding no
message) races shutdown; the test trims trailing schedules before asserting the exact
dispatch order.

### Layer 3

`proptest` re-added to the kernel dev-deps (it had been dropped with all dev-deps) and
property tests added:

- `types/distributable` — encode/decode is an identity over generated `u64`/`i64`/`String`/
  `Vec<u8>`/`(u64, i32, String)` values.
- `types/time` — `Timestamp::merge` laws over generated values: min for numerics,
  commutative, associative, idempotent, monotone (never advances the frontier).

### Layer 4

The 10 regression rows, each pinned by a named test:

| Bug | Regression test |
|---|---|
| `ClosedSignal::wait_for` future-of-a-future | `closed_signal_resolves_only_after_close` (operator_io) |
| spsc waker inversion | `parked_receiver_wakes_on_send` (spsc) |
| `merge_frontiers` dropped the MAX epoch | `completion.rs` (Epoch(MAX) reaches the sink, job terminates) |
| `ConnectionKey` direction merge | `operator_channels_connect_on_same_connection_key` + `coordinator_worker_connect_on_same_connection_key` (inter_thread) |
| 1-based vs 0-based frontier index | `multi_input_epoch_merges_with_zero_based_frontiers` (operator_io) |
| `ExecutionComplete` struct vs enum | `runtime_messages_round_trip` (coordinator/messages) |
| `on_schedule` pump-until-idle | `on_schedule_pumps_until_idle` (operator_logic) |
| root/`no_receivers` termination | `completion.rs` |
| sink swallowing MAX | `completion.rs` + `safe_logic_contract.rs` |
| send-after-close | `send_after_close_is_noop` (operator_io) |

**`send_after_close_is_noop` found a real kernel bug, fixed here:** `Output::close()` (and
the internal Suspend/`CHECK_FINISHED` auto-close) used `watch::Sender::send`, which — per
tokio semantics — is a *no-op that does not update the value* when there are zero receivers.
With no subscriber, `close()` silently left the output open and sends kept flowing. Switched
to `send_replace(true)` (updates unconditionally). This also forced two test-side fixes:
`buffer_on_barriers` used a `NoTime` output, where `CHECK_FINISHED` is always true so the
(now-correct) auto-close after the first send breaks its expectations — it now uses `i32`
timestamps; likewise the pump test. Runtime behavior is unchanged (operators always
subscribe to their own closed signal).

### Layer 5

`///` examples added on the extension surface, all running under `cargo test --doc`
(8 doc tests): `Distributable` (encode/decode round-trip), `Timestamp` (merge), `DataMessage`
(construction + fields), `Message` (variant construction), `OperatorContext` (fields),
`Logic` (minimal impl), `SafeLogic` (minimal impl), and `Operator::built_by` (a kernel-only
single-thread job end-to-end: raw `Logic` source + `then` + `Epoch(MAX)` termination — the
"public API is usable" anchor).

The verbose trait impls (`RuntimeFlavor`, `OperatorOperatorComm`, `WorkerCoordinatorComm`,
`BuildContext`, `StreamBuilder`) get no doc examples — the in-process mock in
`tests/common/mod.rs` is the running usage example, and a doc example would duplicate it with
~30 lines of boilerplate.

## Alternatives considered

- **Coverage for coverage's sake (measure lines/branches)** — the ask is behavioral and API
  stability, not a percentage. Line coverage would reward tests of `Default` impls and miss the
  cross-operator invariants. Rejected as the metric; the layers above are the goal.
- **Move the tests into `malstrom-testkit`** — impossible for the kernel: a kernel dev-dep on
  testkit (which depends on the kernel) duplicates the kernel and breaks type identity
  ([split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
  Decision 8). Kernel integration tests must be self-contained, using the kernel's own
  in-process runtimes and local mock comm backends.
- **Snapshot the public API with `cargo-semver-checks`/`cargo-public-api` in CI** —
  complementary, but it only guards *signatures*, not *behavior*; the Layer-1 tests do both.
  Worth adding later, not instead.

## Consequences

**Accepted.** Final state: 57 kernel unit tests (incl. proptest), 4 integration binaries
(7 tests), 8 doc tests, 3 facade tests — 75 tests total, up from 19. `cargo test -p malstrom`
and `cargo test -p malstrom-core` (unit + doc + integration) are green; tests are
deterministic (no sleeps in tests — completion is observed via channels/barriers).

**Costs and trade-offs.**

- Mocking the comm traits was the hard part, as anticipated: `CoordinatorClient`/`WorkerClient`
  are `pub(crate)`, so coordinator/worker tests implement the *public*
  `WorkerCoordinatorComm`/`OperatorOperatorComm` locally. That mock is itself the contract
  test for the seam `malstrom-k8s`/`malstrom-distributed` rely on, so the effort paid twice.
- A handful of behavioral quirks surfaced and are now pinned as documented behavior: the
  `build` closure must annotate `&mut dyn StreamProvider` (HRTB), `coordinator::api` is
  private (facade exposes `CoordinatorApi` at `malstrom::coordinator::`), the operator loop
  may run one trailing schedule pump during shutdown, and a `NoTime` output auto-closes
  after its first send.
- Writing the tests found and fixed four real kernel bugs: the rescale startup-protocol
  mis-decode, the rescale worker-spawn off-by-one (coordinator thread counted), the terminal
  spsc send blocking forever after 1024 messages, and `Output::close()` being a silent no-op
  without subscribers (`watch::Sender::send`). Each is now a named regression test.
- The `rendezvous_select`/`index_select` partitioner determinism from the original plan
  lives in `malstrom-distributed` (a layer crate), so it is out of scope for the kernel;
  `hash_op_name` stability was already pinned.
- The verbose trait impls (`RuntimeFlavor`, `OperatorOperatorComm`, `WorkerCoordinatorComm`,
  `BuildContext`, `StreamBuilder`) get no doc examples — the in-process mock in
  `tests/common/mod.rs` is the running usage example and a doc example would duplicate it.

**Deferred (pre-existing issues observed, not fixed).**

- Stateful downscale (the rescaling example's 2→1) stalls in the keyed state-movement
  machinery (Interrogate/Collect/Acquire handshake), and removed workers are never told to
  shut down — their sources keep running until `Epoch(MAX)`. The kernel stateless scale-up
  path is proven by `rescale.rs`.
- A `NoTime` output (e.g. the root operator's `Output<()>`) auto-closes after its *first*
  send and the operator exits — the root dies after the first system message, so a second
  coordination message (e.g. a snapshot after a rescale) would never reach the dataflow.

**Related.** Complements the earlier test-suite restoration
([restore-test-suite](../../implemented/testing/2026-08-22-restore-test-suite.md)); this
note covers the new kernel extension suite, that one the pre-refactor suite revival.
