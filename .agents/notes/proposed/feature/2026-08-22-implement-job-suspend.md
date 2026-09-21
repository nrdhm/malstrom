# Agent Note: Implement job suspend

Status: proposed

## Problem

`ApiRequestOperation::Suspend` is marked UNIMPLEMENTED, and `ClusterHandle::suspend`
(added to unblock compilation) is a stub that only logs a warning. A user asking the
coordinator to suspend a job gets a silent no-op.

## Proposal

1. Add a `Suspend` variant to `RuntimeMessage` in `coordinator/messages.rs`.
2. Implement `ClusterHandle::suspend` like `take_snapshot`: send `RuntimeMessage::Suspend`
   to all workers via `CoordinatorClient::send`, set each `WorkerState.phase` to
   `Suspended`, and await the responses.
3. Handle `RuntimeMessage::Suspend` in the worker's `CoordinationTask`, flowing a
   `SysMessage` to the root operator so operators see the `SuspendMarker` (the operator
   loop already exits on it via `output_closed`).
4. Resumption is a separate decision (job restart / resume from snapshot); keep the
   suspended state persisted via the coordinator's snapshot machinery.

## Alternatives considered

### Why not port `perform_suspend_all` from `main` as-is?
The old implementation targeted the pre-async message set and worker protocol; the new
`RuntimeMessage`/`SysMessage` split and the `no_receivers` termination path change what
"suspended" must mean. Port the mechanism, not the code.

### Why not drop the API entirely?
The coordinator API and the `WorkerPhase::Suspended` state already exist; removing them is
more churn than implementing the message flow.

## Acceptance criteria

- A `Suspend` request through `CoordinatorApi` reaches all workers, sets their phase to
  `Suspended`, and completes.
- Workers stop processing cleanly (operators observe `SuspendMarker`); no records are lost
  mid-flight beyond what a later resume from snapshot tolerates.
- The stub's warning is gone.

## Risks

- Suspend/resume semantics interact with exactly-once snapshots; a suspended job that never
  resumes must not wedge the coordinator loop (`check_execution_complete` must treat
  suspended workers specially).
- Scope creep: this touches the coordinator protocol, worker coordination, and operator
  plumbing — worth a dedicated effort, not a drive-by.
