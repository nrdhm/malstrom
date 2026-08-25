# Agent Note: Point the k8s and kafka crates at the local `malstrom`

Status: proposed

## Problem

`malstrom-k8s/runtime` and `malstrom-kafka` declare `malstrom = "0.1.0"` — the **published**
crate from crates.io (commits `e85f6a7` and `be253f5` switched them away from local paths).
They therefore compile against the *old* pre-refactor API (`BiStreamTransport`,
`CommunicationBackendError`, sync trait methods) and are unaffected by — and unable to use —
everything on `new-scheduler`. This masks real incompatibilities: the workspace is green
only because these crates never build against the branch.

## Proposal

1. Switch both crates' `malstrom` dependency to a path dependency on `../malstrom-core`.
2. Migrate `malstrom-k8s/runtime` to the new API:
   - `WorkerCoordinatorComm` is now `#[async_trait]` with `worker_to_coordinator` /
     `coordinator_to_worker` returning boxed `ReqResReceiver`/`ReqResSender`
     (`Box<dyn Error + Send + Sync>`); replace `BiStreamTransport` usage.
   - `OperatorOperatorComm` now has `new_sender`/`new_receiver`; `TransportError` and
     `CommunicationBackendError` are gone (use `Box<dyn std::error::Error>`).
   - `types::distributable::Distributable` replaces `BiCommunicationClient`.
3. Fix `malstrom-kafka` against whatever changed in its small surface (`record.rs`, `sink.rs`).
4. **Keep `malstrom-k8s` and `malstrom-k8s-operator` buildable in CI** — **both** crates'
   `build.rs` run `tonic_build::compile_protos` (`exchange.proto`, `k8s_operator_api.proto`),
   which needs a `protoc` binary that CI runners do not provide; `ci.yaml` now installs
   `protobuf-compiler` (a short-lived exclusion of the two crates was tried and reverted —
   they are first-class parts of the repo, so the gate keeps them in). The protoc requirement
   is **independent** of the re-pointing — the gRPC runtime and operator keep their protobufs
   after migrating to the new comm traits.

## Alternatives considered

### Why not keep the crates.io pin until the branch is released?
It makes the workspace's green status misleading and lets the k8s runtime rot further
against the async API; the migration cost only grows.

### Why not revert the pin commits instead?
They exist precisely because the local path pointed at the broken branch; the right fix is
to finish the migration, not to un-pin while broken.

## Acceptance criteria

- `cargo check --workspace` compiles both crates against the local `malstrom`.
- The k8s runtime's gRPC backends exercise the new comm traits (compile-level; running it
  needs a cluster).

## Risks

- The k8s runtime migration is substantial — its `transport.rs`/`worker_backend.rs` are
  written against the old bidirectional transport and need a real rewrite, not a signature
  tweak. Budget it as a dedicated effort; until then the crates.io pin is a deliberate,
  documented divergence.
