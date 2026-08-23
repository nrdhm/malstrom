# T2 — Make `WorkerCoordinatorComm` errors `Send + Sync`

> **Status:** [ ] open · **Errors:** 3 · **Files:** `runtime/communication/worker_coordinator.rs`,
> `malstrom-k8s/runtime/src/communication/worker_backend.rs`

## Context

`WorkerClient::new` (used by `Worker::new` at `worker/worker.rs:45`) returns
`Result<Self, Box<dyn std::error::Error + Send + Sync>>` because `WorkerExecutionError::Communication`
requires `Send + Sync` (`worker/worker.rs:122`). But the trait method it calls —
`WorkerCoordinatorComm::worker_to_coordinator` — returns `Box<dyn std::error::Error>` (no
`Send + Sync`). The `?` at `worker_coordinator.rs:92` cannot convert
`Box<dyn Error>` → `Box<dyn Error + Send + Sync>` (that `From` impl does not exist).

## Errors (all `runtime/communication/worker_coordinator.rs:92`)

- E0277 `dyn StdError: Send` not satisfied
- E0277 `dyn StdError: Sync` not satisfied
- E0277 `dyn StdError: Sized` not satisfied

## Fix

Harden the **trait** (both methods) in `runtime/communication/worker_coordinator.rs`:

```rust
async fn worker_to_coordinator(
    &self,
) -> Result<impl super::ReqResReceiver, Box<dyn std::error::Error + Send + Sync>>;

async fn coordinator_to_worker(
    &self,
    to_worker: WorkerId,
) -> Result<impl super::ReqResSender, Box<dyn std::error::Error + Send + Sync>>;
```

Then update every impl of the trait:

1. `runtime/threaded/single.rs:109` — `impl WorkerCoordinatorComm for InterThreadCommunication`
   (bodies are `todo!()` from T5; just match the new return types).
2. `malstrom-k8s/runtime/src/communication/worker_backend.rs:135` — `impl WorkerCoordinatorComm
   for WorkerGrpcBackend`: change the error type to `Box<dyn std::error::Error + Send + Sync>`
   (the underlying tonic/transport errors are already `Send + Sync`, so this is usually a
   signature-only change).

`CoordinatorClient::new` needs no change: std provides
`From<Box<dyn Error + Send + Sync>> for Box<dyn Error>`, so its `?` keeps working.

## Verify

```bash
cargo check -p malstrom          # errors 3–5 gone
cargo check -p malstrom-k8s      # worker_backend.rs still compiles
```

## Risks

Public trait change — any external `WorkerCoordinatorComm` impl (k8s runtime) must be
updated in the same commit. This is a small, contained break.
