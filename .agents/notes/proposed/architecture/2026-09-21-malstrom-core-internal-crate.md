# Agent Note: `malstrom-core-internal` — a lower layer for edge primitives

Status: proposed

## Problem

`malstrom-core` has two audiences and one `pub` surface that serves both — see
[`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
for the full inventory:

- The **facade** (`malstrom/src/lib.rs`) re-exports every core module verbatim, so the
  sibling-facing surface is also the user-facing surface: a user can name
  `malstrom::channels::spsc::Receiver`, `malstrom::channels::alignment::AlignmentGroup` and
  `malstrom::channels::recv_trait::Receiver`.
- The **sibling crates** (`malstrom-distributed`, `malstrom-combinators`, `malstrom-testkit`)
  reach into those same deep paths for edge plumbing.

Rust has only `pub` and `pub(crate)`, so "visible to sibling crates but not to users" cannot be
expressed inside one crate. `#[doc(hidden)]` is unenforced — it hides the docs but the item
stays nameable.

## Proposal

A lower-layer crate, **`malstrom-core-internal`**, holds the edge primitives that reference no
core type and that users must not see. Dependency direction is strictly downward: core and the
siblings depend on the internal crate; it depends on none of them; the facade never re-exports
it.

```
malstrom-core-internal        (no facade exposure)
        ▲          ▲
        │          │
malstrom-core + malstrom-{distributed,operators,testkit}
        ▲
        └── malstrom (facade) ──► users
```

**Move to `malstrom-core-internal`:** `spsc` (the SPSC edge channel), `recv_trait` (the
receiver abstraction) and `alignment` (the barrier-alignment combinator). These three are a
self-contained cluster — they reference only `std`/`futures`/`indexmap`/`log`, not any core
type — which is what makes them movable without a dependency cycle. `malstrom-core` depends on
the crate and re-exports the modules `pub(crate)` for its own use; the siblings import from it
directly.

**Kernel-owned, stays in `malstrom-core`:** `stream::OperatorBuilder` (with the shared no-op
`stream::Forward` it uses). Its whole purpose is to hide `Operator`'s internals from combinator
code; it must be expressible against the kernel's `SafeLogic`/`Operator`/`Input`, so a lower
crate would invert the dependency. Being used by the operator crates does not make it
operator-layer.

**Public by classification, not hidden:** `runtime::communication` and `types::distributed`
are legitimate extension points — the `OperatorOperatorComm`/`WorkerCoordinatorComm` traits are
what `malstrom-k8s/runtime` implements, and `types::distributed` is the keyed state-movement
vocabulary embedded in the public `Message` enum.

## Current state (2026-09-21)

Landed in the tree already, but **this note stays `proposed` until the owner declares it done**:

- `malstrom-core-internal` exists with `spsc`, `recv_trait`, `alignment`; core depends on it and
  re-exports `pub(crate)`; siblings import from it directly; the facade no longer exposes them
  (`malstrom::channels::{spsc, recv_trait, alignment}` are gone, and
  `malstrom/tests/namespace.rs` asserts the new surface).
- The earlier safe-subset items are done: `worker::InnerRuntimeBuilder` and
  `Output`/`Input::add_another_one` are `pub(crate)`; `Operator`'s `input`/`output` fields are
  `pub(crate)` so `OperatorBuilder` is the construction path.
- `OperatorBuilder`/`Forward` are back in `malstrom-core` (doc-hidden) after a brief, reverted
  attempt to move them to `malstrom-combinators`.

## Alternatives considered

- **`#[doc(hidden)]` only, no new crate** — cheapest, no dependency changes, but not enforced:
  a user can still name the items and the facade still re-exports the modules. Kept only for
  the kernel-owned `Forward`/`OperatorBuilder`.
- **A feature-gated `internal-api` module in `malstrom-core`** — `#[cfg(feature = …)] pub mod
  internal { … }`, enabled by siblings. Real enforcement, one crate, no new edge — but features
  unify across the graph, so a user can enable it too, and it adds cfg noise. Rejected in
  favour of the crate.
- **Move `OperatorBuilder`/`Forward` to `malstrom-combinators`** — tried and reverted: it inverts
  the abstraction's ownership (a kernel mechanism that hides `Operator` internals) and reads
  ownership off the caller rather than the concept.
- **Curate the facade only, leave core's `pub` tree** — fixes the user view but core's own
  surface stays flat and the sibling/user distinction stays implicit. Strictly weaker; the
  crate move is the enforced version.
- **Do nothing** — the audit's status quo: users inherit the sibling surface.

## Acceptance criteria

- `malstrom-core-internal` holds the agreed sibling-only items; `malstrom-core` and the
  siblings build against it.
- `malstrom` (facade) no longer re-exports the internal modules: `spsc`, `recv_trait` and
  `alignment` are not reachable under `malstrom::…`.
- `cargo-public-api` (or an equivalent `cargo doc` JSON diff) pins the intended user surface of
  `malstrom-core` and the facade, and fails when an internal item leaks into it.
- `cargo test --workspace` and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` stay
  green; no intra-doc link points at a moved item.
- The kernel-owned vs public classification (`OperatorBuilder`/`Forward`; `communication` /
  `types::distributed`) is recorded and stable.

## Risks

- **Scope creep into a rename.** The adjacent [combinators-vs-operators](../../implemented/architecture/2026-09-22-combinators-vs-operators.md)
  proposal may split/rename the operator crate; keep that separate from this boundary.
- **Re-export spillover.** A transitional `pub use` from `malstrom-core` re-opens the same leak;
  keep them `pub(crate)` / `#[doc(hidden)]` and removed where possible.
- **The edge notes reshape the moved modules.** `spsc`/`recv_trait`/`alignment` may move or
  merge again when [unify-operator-io-edge-abstractions](2026-09-13-unify-operator-io-edge-abstractions.md)
  and [replace-operator-io-spsc-with-tokio-mpsc](2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)
  land; the boundary direction is what matters, not the file location.
- **`cargo-public-api` gate not yet added** — without it, a new leak is not caught
  automatically; `malstrom/tests/namespace.rs` is the current anchor.

## Related

- [`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
  — the audit this acts on (module inventory, audience matrix, hide list).
- [combinators-vs-operators](../../implemented/architecture/2026-09-22-combinators-vs-operators.md) — the adjacent
  distinction between graph combinators and message operators.
- [stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md)
  — created `OperatorBuilder` and the shared `Forward`.
- [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md) — the crate split whose
  first-class-kernel decision this extends with an internal boundary.