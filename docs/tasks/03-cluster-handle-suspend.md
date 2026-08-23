# T3 — Implement `ClusterHandle::suspend`

> **Status:** [ ] open · **Errors:** 1 · **Files:** `coordinator/cluster.rs` (+ optionally `coordinator/messages.rs`)

## Context

The coordinator API exposes `ApiRequestOperation::Suspend`, explicitly marked
`#[allow(unused)] // TODO` and documented "UNIMPLEMENTED: Request Coordinator to suspend the
execution" (`coordinator/api.rs:63`). The coordinator loop dispatches it at
`coordinator/coordinator.rs:160`:

```rust
ApiRequestOperation::Suspend => state.suspend().await,
```

…but the new `ClusterHandle` (`coordinator/cluster.rs:21`) never got a `suspend` method
(the old `main` coordinator had `perform_suspend_all`, see `git show main:.../coordinator.rs:440`).

## Error

- E0599 `no method named 'suspend' found for struct 'ClusterHandle'` at `coordinator/coordinator.rs:160`

## Fix (minimal — unblock compilation)

Add to `impl ClusterHandle` in `coordinator/cluster.rs`:

```rust
/// Suspend execution on all workers.
/// NOTE: currently a stub — the suspend feature is unimplemented (see ApiRequestOperation::Suspend).
pub async fn suspend(&self) {
    tracing::warn!("Coordinator suspend is not yet implemented");
}
```

## Fix (proper — port the real behavior)

1. Add a `Suspend` variant to `RuntimeMessage` in `coordinator/messages.rs`.
2. Implement `ClusterHandle::suspend` like `take_snapshot`: send `RuntimeMessage::Suspend`
   to all workers via `CoordinatorClient::send`, set each `WorkerState.phase` to `Suspended`,
   and await responses.
3. Handle `RuntimeMessage::Suspend` in the worker's `CoordinationTask`
   (`worker/coordination_task.rs`).

Reference: `git show main:malstrom-core/src/coordinator/coordinator.rs:440`
(`perform_suspend_all`).

## Verify

```bash
cargo check -p malstrom   # error 6 gone
```

## Risks

Minimal stub is a no-op at runtime (suspend silently does nothing). Only acceptable while
the feature is explicitly UNIMPLEMENTED; track the proper port separately.
