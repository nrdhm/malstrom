# Agent Note: Split malstrom-core into layered crates

Status: implemented

## Problem

`malstrom-core` (the published `malstrom` crate) bundled **three layers** in one crate: the
*kernel* (execution engine), the *stdlib* (user-facing operators/sources/sinks), and the
*distributed protocol* (keyed routing). The signs:

- **File counts skew toward library code** — `operators` (21) + `sources` (3) + `sinks` (5) +
  the local keyed operators outnumbered the engine itself; `keyed` was 20 files, ~16 of which
  were the `keyed/distributed` routing protocol.
- **`sources` and `keyed` each mixed library and engine** — `sources/stateful.rs`
  (`SourceCoordinator`/`SourcePartitionOp`) is engine code yet depended on
  `keyed::distributed`; `keyed/key_local` is a library operator while `keyed/distributed` is
  the protocol layer.
- **Leaf dependencies were library-only** — `expiremap` (only `ttl_map`), `rand` (only
  `operators/time/util.rs`), `slatedb`/`object_store`/`tokio-stream` (only
  `snapshot/slatedb.rs`) sat in the kernel manifest; `eyre` was declared and unused.
- **The operator seam was missing** — `channels::operator_io::{Input, Output}`,
  `stream::{Logic, SafeLogic, SafeLogicWrapper}`, and `stream::BuildContext` were
  `pub(crate)`, so nothing outside the crate could implement or compose operators (unlike the
  `StreamSource`/`StreamSink` and `RuntimeFlavor`/`OperatorOperatorComm` seams that already
  made `malstrom-kafka`/`malstrom-k8s` external).

## Decision

Split `malstrom-core` along the kernel / stdlib / protocol boundary, with the kernel owning a
**public operator extension API** that the other crates build on. Shipped crate graph (acyclic;
`malstrom` is depended on, never depends on its own layers):

| Crate | Contents | Depends on |
|---|---|---|
| `malstrom` (kernel, `malstrom-core/`) | `types` (incl. `types::distributed` protocol messages), `channels`, `stream`, `worker`, `coordinator`, `runtime`, `snapshot` | — |
| `malstrom-distributed` | `keyed/distributed` routing (routers, distributor, `remote_receiver`/`remote_sender`, `wire_message`/`versioned_message`/`targeted_message`) plus `worker_partitioners` | `malstrom` |
| `malstrom-operators` | `operators`, `sinks` (incl. `VecSink`), `sources` (incl. `fn_source` and the source engine), local keyed ops (`key_local`, `key_distribute`, `broadcast`) | `malstrom`, `malstrom-distributed` |
| `malstrom-testkit` | `testing` (operator tester, in-memory comm backends, capture persistence) | `malstrom` |
| `malstrom-snapshot-slatedb` | the SlateDB/object-store `PersistenceBackend` | `malstrom` |

1. **The extension API is public.** `stream::{Logic, SafeLogic, SafeLogicWrapper,
   BuildContext, OperatorContext, StreamBuilder}`, `channels::{operator_io, alignment,
   recv_trait, spsc}`, `runtime::communication::{OperatorCommSender, OperatorCommReceiver,
   broadcast}`, `worker::{InnerRuntimeBuilder, add_operator}`, the snapshot state helpers,
   `RescaleMessage::new`, and the protocol-message constructors are all `pub` with docs.
   `types::distributable` and `keyed::distributed` are public modules.
2. **Protocol message types stay in the kernel** as `types::distributed::{Acquire, Collect,
   Interrogate}` — the `Message` enum and the `SafeLogic` handlers embed them, so they are
   kernel runtime vocabulary. The `WireAcquire` conversion moves with the distributed crate.
3. **`worker_partitioners` moved with the distributor** (deviation from the proposal's
   "local keyed ops" list): `WorkerPartitioner` is the `DistributorBuilder`'s partitioner
   protocol and must be visible to both crates; `malstrom-operators`' `keyed` module
   re-exports it.
4. **`malstrom-operators` keeps a `keyed::distributed` shim** re-exporting
   `malstrom-distributed`, so the historical `crate::keyed::distributed::…` paths in the
   operator layer keep resolving without churn.
5. **The `malstrom::operators` public surface moved to `malstrom_operators`.** A kernel
   re-export would create a cycle (`malstrom → malstrom-operators → malstrom-distributed →
   malstrom`); cargo forbids crate cycles. Examples, README, the website guide, and the
   overviews were migrated (`malstrom::operators` → `malstrom_operators::operators`,
   `malstrom::sources` → `malstrom_operators::sources`, etc.); kernel paths
   (`malstrom::runtime`, `malstrom::snapshot`, …) are unchanged.
6. **The source engine lives in `malstrom-operators`** (deviation from the proposal's kernel
   row): `SourceImpl`/`SourcePartition`, the `Source` struct, the coordinator→distribute→reader
   graph, and `fn_source` move together as one unit (see
   [collapse-source-traits](2026-08-22-collapse-source-traits.md) for the trait unification
   this builds on), so the engine's use of `malstrom_distributed` (`rendezvous_select`,
   `Acquire`/`Collect`/`Interrogate`) is an operator→distributed edge, not a kernel seam —
   the proposal's "seam 1" never had to be cut.
7. **`VecSink` stayed with `malstrom-operators`' sinks** (deviation from the proposal's
   testkit row): a testkit → operators dependency would create a dev-dependency cycle with
   the operators' own tests.
8. **Phased order changed** (deviation): the proposal put `malstrom-testkit` first, but a
   kernel dev-dependency on testkit (which depends on the kernel) makes cargo build a
   **second kernel instance**, breaking type identity (`malstrom_testkit::VecSink` is a
   different type from the kernel's `VecSink`). Testkit can only be consumed by non-kernel
   crates, so the extraction moved `malstrom-distributed` + `malstrom-operators` first, then
   testkit, then the slatedb connector.
9. **The kernel manifest is lean.** `rand`, `expiremap`, `eyre` (dead), and the
   `slatedb`/`object_store`/`tokio-stream` stack left the kernel; `console-subscriber` became
   a dev-dependency (multithreading example); the `slatedb` feature and its `[[example]]`
   feature gates were removed.
10. **The slatedb examples live in `malstrom-snapshot-slatedb`** — moved out of the kernel
    package right after the split (`malstrom-snapshot-slatedb/examples/`), so the connector
    crate owns its examples. They had been feature-gated and never compiled against the async
    operator API; their closures are `async` now, and they import `SlateDbBackend` from the
    crate itself. (They still panic at runtime with a pre-existing tokio runtime-drop error,
    unchanged from before the split.)

## Alternatives considered

- **Status quo (keep the monolith)** — zero churn, no versioned extension-API contract.
  Lost: kernel builds without library/protocol deps, WIP-protocol isolation, and the forcing
  function to stabilize the operator API. Rejected.
- **Extract without stabilizing the API** (`#[path]` hacks or a `__private` grab-bag) —
  rework bait; the API is the deliverable, the crate boundary just hosts it. Rejected.
- **Extract `malstrom-distributed` before `malstrom-operators`** — the proposal's
  reservation held in reverse: the two extractions had to land together because the kernel
  hosted operator code that imported `keyed::distributed`; splitting either alone broke the
  kernel. The proposal's original "cheap extraction first" (testkit) was impossible for the
  dev-dep-cycle reason in Decision 8.
- **One `malstrom-stdlib` crate (operators + distributed together)** — fewer crates but
  keeps library and protocol coupled, defeating WIP isolation. Rejected.
- **`malstrom::operators` re-export for compatibility** — impossible: it would cycle the
  crate graph (Decision 5). The public surface moves; examples/docs migrate.
- **Feature-gated modules instead of crates** — a lighter-weight middle ground, but no real
  dependency isolation. Not taken.
- **Keeping the whole source module in the kernel with an inverted seam** — the proposal's
  "seam 1" (engine takes the distributor as a parameter) was unnecessary once the engine
  moved to `malstrom-operators` wholesale (Decision 6).

## Consequences

- **Acyclic, layered graph** — `malstrom` is a dependency leaf; each layer compiles and tests
  without the layers above it. `cargo check --workspace` is clean (0 warnings).
- **Versioned extension API** — every operator-facing type is now semver surface; breaking
  it is a breaking change. This is the maintenance cost the proposal predicted.
- **Kernel manifest is lean** — the kernel no longer pulls `expiremap`/`rand`/`slatedb`
  stack; connectors (`malstrom-snapshot-slatedb`, `malstrom-kafka`) are their own crates.
- **WIP isolation** — the distributed protocol (the least-stable layer) is its own crate;
  the kernel builds/tests without the router machinery.
- **API churn** — every example, the README, `website/guide/TtlMapOperator.md`, and the
  overviews migrated from `malstrom::operators` to `malstrom_operators::…`; kernel-path
  imports are unchanged. `malstrom-kafka`/`malstrom-k8s` pin the published `malstrom 0.1.0`
  and are unaffected.
- **Verification** — tests green: `malstrom` 18 unit + 1 doc, `malstrom-operators`
  31 unit + 9 doc (the operator/source tests and doctests moved with the code), 
  `malstrom-testkit` 1, `malstrom-snapshot-slatedb` 5; examples `look_ma_im_streaming`,
  `basic_stdout`, `ttl_map`, `rescaling` smoke-run identically to before the split. The
  pre-existing silent-exit quirk of keyed chains without a terminal sink is unchanged.
- **Dev-dep cycle lesson** — cargo duplicates a crate across a dev-dependency cycle, breaking
  type identity; test utilities must live where they are consumed or be consumed only by
  non-cyclic crates.
