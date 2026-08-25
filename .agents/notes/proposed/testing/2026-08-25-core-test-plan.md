# Agent Note: Test the `malstrom` core (kernel) for behavior and API stability

Status: proposed (Layer 1a implemented 2026-08-25 — see Progress below)

## Progress

### Layer 1a — DONE (2026-08-25)

`malstrom-core/tests/` now contains four contract-test files (importing only
`malstrom_core::`, no layer crates):

- `tests/common/mod.rs` — in-process mock of `OperatorOperatorComm` +
  `WorkerCoordinatorComm` + a `RuntimeFlavor` (`MemoryComm`/`MemoryFlavor`), used by
  `runtime_flavor_contract.rs`.
- `tests/safe_logic_contract.rs` — `SafeLogic` operator ordering (`on_schedule` →
  `on_data`/`on_barrier`/`on_epoch`) + automatic system-message forwarding. PASSES.
- `tests/completion.rs` — kernel-only source emitting `Epoch(MAX)` terminates both
  runtimes; multi-worker parallelism doubles the values. PASSES.
- `tests/runtime_flavor_contract.rs` — the comm/flavor seam round-trips (operator
  stream, coordinator req/res, flavor communication). PASSES (req/res test needed a
  concurrent sender — `receiver.recv()` before any send can never return).
- `tests/rescale.rs` — rescale 1→2 without deadlock, job still completes. PASSES.

**Writing `rescale.rs` found three real kernel bugs, all fixed in this change:**

1. **Rescale sent the startup protocol to existing workers**
   `ClusterHandle::reconfigure` called `start_build`/`start_execution` for *all* workers;
   an existing worker's `CoordinationTask` only decodes the `RuntimeMessage` enum, so the
   tuple-struct `StartBuild` was mis-decoded as the enum and panicked, killing the
   coordinator loop (`Err(Stopped)` on the API handle). Fixed: bootstrap only the
   newly-added workers; existing workers learn the new scale via
   `RuntimeMessage::Reconfigure`. (The rescaling example previously "worked" only because
   its 2→2 rescale is a no-op and the fatal 2→1 happened late.)
2. **Rescale never spawned the new worker**
   `MultiThreadRuntime::execute` used `threads.len()` (which includes the coordinator
   thread) as the current scale, so a rescale from P to P+1 spawned nothing and the
   coordinator's `start_build` blocked forever on the missing worker. Fixed: track
   `workers_spawned` separately.
3. **Terminal operator output blocked forever after 1024 messages**
   The spsc `Send::poll` queued into the bounded channel even after the receiver was
   dropped (its "sends without a receiver drop the message" doc was a lie; the
   `sending_without_receiver` unit test never polled the future, so it false-passed).
   The last operator's output tail receiver is dropped at build time, so after 1024
   records the terminal's `send` blocked forever, stalling the whole pipeline (the
   source never polled its input, so the rescale handshake could not complete). Fixed:
   `Send::poll` drops the message when `has_receiver == false`, matching the documented
   intent. This is why the earlier tests (5 records, 2 records) never hit it.

**Observed but NOT fixed (pre-existing, outside Layer 1a):** the rescaling example's
stateless 2→2 rescale completes, but its *stateful* 2→1 downscale stalls in the
keyed-state movement machinery (Interrogate/Collect/Acquire handshake). Previously the
coordinator died at that point; it now reaches the state-movement layer and hangs. Also,
downscale never shuts removed worker threads down (their sources keep running until
`Epoch(MAX)`). Both are candidates for a follow-up (Layer 4 regression or a dedicated
state-movement fix) — the kernel stateless scale-up path is proven by `rescale.rs`.

### Remaining layers

- 1b: `malstrom/tests/` (hello_pipeline + namespace) — unblocked, facade landed.
- 2: unit tests (types/stream/channels/runtime/coordinator/worker/snapshot).
- 3: proptest — must re-add `proptest` to the kernel dev-deps (dropped with all dev-deps).
- 4: the 10 regression tests from the table.
- 5: doc examples on the public extension surface.

## Problem

The kernel is the foundation every other crate builds on, and its public surface was just
widened by [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
— but almost none of it is pinned by tests. Current coverage is 19 unit tests concentrated in
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

There is no `malstrom-core/tests/` directory, so no test exercises the public API the way an
external consumer does. The eleven runtime bugs fixed earlier this cycle (see the
[collapse-source-traits](../../implemented/architecture/2026-08-22-collapse-source-traits.md)
and [restore-new-scheduler-compilation](../../implemented/bug-fix/2026-08-22-restore-new-scheduler-compilation.md)
notes) each fixed behavior that had no test — every one of them could regress silently.

## Proposal

Cover the kernel in five layers, ordered by value-per-effort. The through-line is: tests that
(a) pin the public extension API from the *outside*, (b) lock in the tricky invariants future
refactors will touch, and (c) act as regression guards for the bugs already fixed.

### Layer 1 — public-API contract tests, split by which crate owns the surface

A contract test must live in the crate that *owns* the surface it exercises — the kernel can
only pin what the kernel exposes (a kernel dev-dep on `malstrom-operators` would cycle the
graph, per [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
Decision 8).

**1a. Kernel extension contract (`malstrom-core/tests/`)** — pin the seam the layer crates
build on, importing only the kernel crate (`malstrom_core::` after the rename) and never
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

**1b. Full public-API contract (`malstrom/tests/`, once the facade exists)** — pin the
end-user `malstrom::` surface (kernel + operators + distributed re-exported under one
namespace). This is the real "public API predictability" anchor for users:

- `tests/hello_pipeline.rs` — `malstrom::sources` → `malstrom::operators` → `malstrom::sinks`
  pipeline run on `SingleThreadRuntime` and `MultiThreadRuntime`, asserting deterministic
  output.
- `tests/namespace.rs` — every historical top-level path resolves (`malstrom::operators`,
  `malstrom::sources`, `malstrom::sinks`, `malstrom::keyed`, `malstrom::keyed::distributed`,
  `malstrom::runtime`, …), so a missing facade re-export fails at compile time.

1b depends on [rename-kernel-and-add-malstrom-facade](../../proposed/architecture/2026-08-25-rename-kernel-and-add-malstrom-facade.md);
until the facade lands, the end-user surface is covered by the operator crate's own tests plus
1a.

### Layer 2 — unit tests for untested invariants

- `types/` — `Distributable` encode/decode round-trips for the primitive and container types;
  `Timestamp` merge + `CHECK_FINISHED` semantics; `Message`/`DataMessage` (de)serialization;
  `Kvt` tuple impls; `NoKey`/`NoData`/`NoTime` unit semantics.
- `stream/` — `StreamBuilder`/`Malstrom::then` links operators in order (output↔input wiring);
  `LogicBuilder` closure and `build` forms; `BuildContext`/`OperatorContext` expose the
  documented fields.
- `channels/` — `alignment.rs` (barrier held until all channels report, emitted once aligned),
  `recv_trait`, `signal`, and the `Input::recv` epoch/frontier merge rules.
- `runtime/` — `SingleThreadRuntime` end-to-end; `RuntimeFlavor`/`OperatorOperatorComm`/
  `WorkerCoordinatorComm` contract via a local mock; `inter_thread` channel keying.
- `coordinator/` — `ClusterHandle` build/execute/reconfigure using a mock `WorkerCoordinatorComm`
  (the `CoordinatorClient`/`WorkerClient` req/res round-trip).
- `worker/` — `WorkerBuilder` builds a dataflow and `execute` runs it; `CoordinationTask`
  handles snapshot/reconfigure/execution-complete messages.
- `snapshot/` — barrier propagation order through operators; `NoPersistence` +
  `serialize_state`/`deserialize_state` round-trip.

### Layer 3 — property-based tests (`proptest`, already a dev-dep)

- `Distributable` round-trip: `proptest!(|v in …| assert_eq!(decode(&encode(v)), v))` over the
  types users will send (`u64`, `usize`, `String`, `Vec<u8>`, nested tuples).
- `Timestamp` merge: commutative, associative, idempotent, monotone (for the numeric impls).
- Partitioner/hash: `rendezvous_select`/`index_select` determinism and `hash_op_name`
  stability (the existing `hash_is_stable` test is the model).

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

Every public item in the extension surface (`StreamBuilder`, `Operator`, `Logic`,
`SafeLogic`, `BuildContext`, `RuntimeFlavor`, `OperatorOperatorComm`, `WorkerCoordinatorComm`,
`Message`, `DataMessage`, `Distributable`, `Timestamp`) gets a `///` example. Doc tests are the
cheapest "public API is usable" check and they compile+run under `cargo test --doc`.

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

## Acceptance criteria

- `malstrom-core/tests/` exists and its tests import only the kernel crate (no `crate::`,
  no `malstrom_operators`/`malstrom_distributed`); the facade `malstrom/tests/` imports only
  `malstrom::` and reaches the full surface.
- Every public module has at least one test that reaches it through the public API.
- The Layer-4 regression table is implemented (one test per row, named after the bug).
- `cargo test -p malstrom` (unit + doc + integration) is green; the new tests are deterministic
  (no `sleep`, no timing-dependent assertions).

## Risks

- **Mocking the comm traits is the hard part.** `CoordinatorClient`/`WorkerClient` are
  `pub(crate)`; coordinator/worker unit tests must implement the *public*
  `WorkerCoordinatorComm`/`OperatorOperatorComm` traits locally, which is also a useful
  contract test but is real work.
- **Determinism** — coordinator/rescale tests can become flaky if they rely on timing; use
  channels/barriers to observe completion (as the existing `multi.rs` test does) rather than
  sleeps.
- **`proptest` currently sits unused in the kernel manifest** — Layer 3 revives it; if it stays
  unused it should be dropped instead (see the separate dead-dependency cleanup).
