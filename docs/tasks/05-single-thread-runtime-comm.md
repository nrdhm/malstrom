# T5 — Implement `SingleThreadRuntimeFlavor` communication

> **Status:** [ ] open · **Errors:** 2 · **Files:** `runtime/threaded/single.rs` (+ `runtime/threaded/communication/*`)

## Context

`SingleThreadRuntimeFlavor`'s `InterThreadCommunication` is a hollow shell: every method is
`todo!()` and the struct is empty (`runtime/threaded/single.rs:88`). The commented-out line
at 80 (`Ok(InterThreadCommunication::new(self.comm_shared.clone(), 0))`) shows the intent:
there used to be a shared in-process comm backend. The new `runtime/threaded/communication/`
module (split out during the refactor) holds the primitives to rebuild it:
`inter_thread.rs`, `reqres.rs`, `stream.rs`.

## Errors

- E0283 `type annotations needed` at `single.rs:112` — `todo!()` can't name the hidden type
  behind `Result<impl ReqResReceiver, …>`.
- E0283 `type annotations needed` at `single.rs:119` — same for `impl ReqResSender`.

## Fix (minimal — compile only)

Make the impl return **concrete** types so `todo!()` bodies type-check. The trait permits
impls to be more concrete than the trait signature. If `runtime/threaded/communication/reqres.rs`
exposes e.g. `ThreadedReqResReceiver/Sender`, use those:

```rust
impl WorkerCoordinatorComm for InterThreadCommunication {
    async fn worker_to_coordinator(
        &self,
    ) -> Result<ThreadedReqResReceiver, Box<dyn std::error::Error + Send + Sync>> {   // match T2's trait
        todo!()
    }
    // same for coordinator_to_worker → ThreadedReqResSender
}
```

`new_sender`/`new_receiver` (lines 92–106) return `Box<dyn StreamSender>`/`Box<dyn StreamReceiver>`
— those already compile with `todo!()`; only the two `impl Trait` returns need concrete types.

## Fix (proper — wire it up)

Reconstruct the shared in-process comm from `runtime/threaded/communication/`:

1. Give `InterThreadCommunication` a `Shared` handle (the old `comm_shared`), as the
   commented-out `new(...)` implies.
2. Implement `new_sender`/`new_receiver` via `threaded/communication/stream.rs` (bounded
   channels keyed by `(worker, operator)`), `worker_to_coordinator`/`coordinator_to_worker`
   via `threaded/communication/reqres.rs`, and fill in `communication()` (line 76).
3. Reference: `git show main:malstrom-core/src/runtime/threaded/communication.rs` — the
   255-line pre-split implementation is the blueprint.

## Verify

```bash
cargo check -p malstrom            # errors 7–8 gone
cargo run --example look_ma_im_streaming   # SingleThreadRuntime actually executes
```

## Risks

The minimal fix leaves the runtime non-functional (panics on `todo!()` at runtime). The
proper fix is the real work — this is the "single-thread runtime" behavior task. Do not
merge the minimal version without the follow-up.
