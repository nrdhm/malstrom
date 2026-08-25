# Agent Note: Restore `new-scheduler` compilation

Status: implemented

## Problem

The `new-scheduler` branch — the async refactor of the core — did not compile: 20 errors in
the `malstrom` lib, the `testing` module was disabled in `lib.rs`, and `MultiThreadRuntime`
was commented out of the build (`// mod multi;` in `runtime/threaded/mod.rs`). The errors
fell into five clusters: a stale `CommunicationError` reference, a `WorkerCoordinatorComm`
whose error type couldn't be `?`-converted, a missing `ClusterHandle::suspend` method, a
half-migrated stateless→stateful source adapter (12 of the 20 errors), and a hollow
`SingleThreadRuntimeFlavor` communication stub (`todo!()`s).

## Decision

- **Removed the stale `CommunicationError`** (`worker/worker.rs` import; `single.rs`
  return type) — the type died with the old `runtime/communication.rs`.
- **Hardened `WorkerCoordinatorComm`** to `Box<dyn Error + Send + Sync>` and converted the
  trait to `#[async_trait]`, returning boxed `ReqResSender`/`ReqResReceiver` trait objects.
  All generic call sites (`CoordinatorClient::new`, `WorkerClient::new`, cluster helpers,
  coordinator loop, `RuntimeFlavor::Communication`) gained `Send`/`Sync` bounds as needed.
- **Implemented `ClusterHandle::suspend`** as a logging stub — the suspend feature was
  already marked UNIMPLEMENTED (`ApiRequestOperation::Suspend`).
- **Finished the stateless→stateful source adapter** (`sources/stateless.rs`): the new
  `StatefulSourceImpl`/`StatefulSourcePartition` traits are 0-generic with associated
  `Value`/`Timestamp`, all methods `async`; `SourceWrapper`/`PartitionWrapper` gained
  `PhantomData<(V, T)>` and `Distributable` bounds; `StatefulSource::new(self.0)` with a
  single generic.
- **Rebuilt `SingleThreadRuntimeFlavor` communication** on the real `InterThreadCommunication`
  from `runtime/threaded/communication/` via delegation, replacing the empty stub.
- **Re-enabled `MultiThreadRuntime`** with the same delegation pattern; fixed its stale
  references (`CommunicationError`, the dead `Shared` type, `CoordinatorRequestError` →
  `ApiRequestError`, `keyed::partitioners` → `keyed::rendezvous_select`).

## Alternatives considered

- **Native async-fn-in-trait vs `#[async_trait]` for `WorkerCoordinatorComm`:** the native
  AFIT future could not be proven `Send` through two generic layers even with
  `where Self: Sync` and `+ Send` on the opaque returns (verified against the real call
  chain, which compiled in isolation but not in situ). `#[async_trait]` boxes the future
  with an explicit `Send` bound, which is unconditional — chosen.
- **`impl Trait` returns vs boxed trait objects in the comm trait:** boxed objects make
  `Send`/`Sync` explicit from the trait's supertraits and avoid RPIT opacity — chosen.
- **`PhantomData` on the adapter wrappers vs restructuring the traits:** the 0-generic
  traits with associated types require the unconstrained `V`/`T` to appear in the self
  type; `PhantomData<(V, T)>` is the minimal fix.

## Consequences

- `cargo check -p malstrom` and `--examples` green; `cargo check --workspace` green —
  but only because `malstrom-k8s`/`malstrom-kafka` still pin the **published**
  `malstrom 0.1.0` from crates.io rather than the local branch (see the proposed
  [migration note](../../proposed/process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md)).
- `RuntimeFlavor::Communication` now requires `Sync`; comm trait methods return boxed
  trait objects — a public API change external backends must follow.
- `ClusterHandle::suspend` silently no-ops until the real feature lands (proposed note
  [implement-job-suspend](../../proposed/feature/2026-08-22-implement-job-suspend.md)).
- `MultiThreadRuntime` is back in the build; examples using it now compile.
