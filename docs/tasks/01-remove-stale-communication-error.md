# T1 — Remove stale `CommunicationError` references

> **Status:** [ ] open · **Errors:** 2 · **Files:** `worker/worker.rs`, `runtime/threaded/single.rs`

## Context

During the async refactor, the old `runtime/communication.rs` (which defined
`CommunicationError`) was replaced by `runtime/communication/` (a directory). The new
`RuntimeFlavor::communication()` returns `Result<Self::Communication, Box<dyn std::error::Error + Send + Sync>>`
(`runtime/runtime_flavor.rs:11-13`) — there is no `CommunicationError` type anymore.

## Errors

1. `worker/worker.rs:12` — E0432: `use crate::runtime::{ CommunicationError, OperatorOperatorComm, RuntimeFlavor, ... }` — no `CommunicationError` in `runtime`.
2. `runtime/threaded/single.rs:78` — E0425: return type references `crate::runtime::runtime_flavor::CommunicationError`, which doesn't exist.

## Fix

**worker/worker.rs** — remove the stale import (line 12):

```rust
use crate::{
    channels::signal::SignalHandle,
    coordinator::messages::*,
    runtime::{
        OperatorOperatorComm, RuntimeFlavor,          // ← drop CommunicationError
        communication::{WorkerClient, WorkerCoordinatorComm},
    },
    ...
};
```

**runtime/threaded/single.rs** — align the impl with the trait signature (line 76-81):

```rust
fn communication(
    &mut self,
) -> Result<Self::Communication, Box<dyn std::error::Error + Send + Sync>> {
    todo!()
}
```

(Keep the `todo!()` — body implementation is T5.)

## Verify

```bash
cargo check -p malstrom   # errors 1–2 gone
```

## Risks

None — purely removing a stale name. T5 rewrites this function's body anyway.
