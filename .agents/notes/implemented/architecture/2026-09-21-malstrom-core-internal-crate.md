# Agent Note: `malstrom-core-internal` — a lower layer for edge primitives

Status: implemented

## Problem

`malstrom-core` had two audiences and one `pub` surface that served both — see
[`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
for the full inventory:

- The **facade** (`malstrom/src/lib.rs`) re-exports every core module verbatim, so the
  sibling-facing surface was also the user-facing surface: a user could name
  `malstrom::channels::spsc::Receiver`, `malstrom::channels::alignment::AlignmentGroup`,
  `malstrom::channels::recv_trait::Receiver`, `malstrom::stream::Forward`, and
  `malstrom::worker::InnerRuntimeBuilder`.
- The **sibling crates** (`malstrom-distributed`, `malstrom-operators`, `malstrom-testkit`)
  reached into those same deep paths for edge plumbing.

Rust has only `pub` and `pub(crate)`, so "visible to sibling crates but not to users" cannot be
expressed inside one crate. `#[doc(hidden)]` is unenforced — it hides the docs but the item
stays nameable.

## Decision

A lower-layer crate, **`malstrom-core-internal`**, holds the edge primitives that the kernel
and siblings need but users must not see. Dependency direction is strictly downward: core and
the siblings depend on the internal crate; it depends on none of them; the facade never
re-exports it.

```
malstrom-core-internal        (no facade exposure)
        ▲          ▲
        │          │
malstrom-core + malstrom-{distributed,operators,testkit}
        ▲
        └── malstrom (facade) ──► users
```

**Shipped (2026-09-21):**

- New crate `malstrom-core-internal` with `spsc` (the SPSC edge channel), `recv_trait` (the
  receiver abstraction) and `alignment` (the barrier-alignment combinator) — moved out of
  `malstrom_core::channels`. These three are a self-contained cluster: they reference only
  `std`/`futures`/`indexmap`/`log`, not any core type, which is what makes them movable
  without a dependency cycle.
- `malstrom-core` depends on it and re-exports the three modules `pub(crate)` for its own
  `operator_io`/`operator_operator`; `malstrom-distributed` and `malstrom-operators` import
  them from `malstrom_core_internal` directly.
- The published `malstrom` facade no longer exposes them: `malstrom::channels::{spsc,
  recv_trait, alignment}` are gone, and the facade namespace test
  (`malstrom/tests/namespace.rs`) was updated to assert the curated surface instead.
- Earlier safe-subset items from the audit are also in place: `worker::InnerRuntimeBuilder`
  and `Output`/`Input::add_another_one` are `pub(crate)`, and `stream::{Forward,
  OperatorBuilder}` are `#[doc(hidden)]`.

**Deferred (coupled to the public extension API):** `stream::Forward`, `stream::OperatorBuilder`,
`runtime::communication` and `types::distributed` stay in `malstrom-core` (doc-hidden where
they are implementation detail). They reference types that must remain user-facing —
`Forward`/`OperatorBuilder` are `SafeLogic`/`Output`-shaped, `communication` implements traits
over `Message`/`Kvt`, and `Message` (public) embeds `Acquire`/`Collect`/`Interrogate`. Moving
them needs either `internal → core` (a cycle) or first decoupling the edge layer, which is what
[unify-operator-io-edge-abstractions](../../proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md)
and [replace-operator-io-spsc-with-tokio-mpsc](../../proposed/architecture/2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)
do.

## Ordering / prerequisites

The plan was to move *final* types, not types about to be rewritten. What actually gated the
move and what remains:

| Before | Why | State (2026-09-21) |
|---|---|---|
| [replace-operator-io-spsc-with-tokio-mpsc](../../proposed/architecture/2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md) / [unify-operator-io-edge-abstractions](../../proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md) | reshape `spsc`/`recv_trait`/`alignment` | **not landed** — so `spsc`/`recv_trait`/`alignment` were moved in their current, self-contained form; they may move/merge again when the edge layer is unified |
| [fail-loud-on-dangling-operator-edges](../../proposed/architecture/2026-09-19-fail-loud-on-dangling-operator-edges.md) | adds a liveness signal to `spsc` | not landed |
| [collapse-kernel-test-support-into-testkit](../../proposed/testing/2026-09-16-collapse-kernel-test-support-into-testkit.md) | changes kernel↔testkit deps | not landed |
| [point-k8s-and-kafka-at-local-malstrom](../../proposed/process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md) | makes the whole workspace build in CI | not landed |
| [fix-warning-backlog](../../proposed/process/2026-08-25-fix-warning-backlog.md) | the new crate inherits the final `[lints]` | Steps 0–3 landed; Step 4 open |

The move proceeded anyway because the chosen cluster is self-contained, so it does not churn
when the edge work lands — at most those modules move again. The coupled items stayed put
rather than force a cycle.

## Alternatives considered

- **`#[doc(hidden)]` only, no new crate** — cheapest, no dependency changes, but not enforced:
  a user can still name the items and the facade still re-exports the modules. Kept only for
  the coupled items (`Forward`/`OperatorBuilder`/`types::distributed`).
- **A feature-gated `internal-api` module in `malstrom-core`** — `#[cfg(feature = ...)] pub mod
  internal { … }`, enabled by siblings. Real enforcement, one crate, no new edge — but features
  unify across the graph, so a user can enable it too, and it adds cfg noise. Rejected in
  favour of the crate.
- **Move `types::distributed` / `Forward` / `communication` too** — impossible without inverting
  the dependency (they reference the public API). Rejected; documented as deferred.
- **Curate the facade only, leave core's `pub` tree** — fixes the user view but core's own
  surface stays flat and the sibling/user distinction stays implicit. Strictly weaker; the
  crate move is the enforced version.
- **Do nothing** — the audit's status quo: users inherit the sibling surface.

## Consequences

- Users can no longer name `spsc`, `recv_trait` or `alignment` through `malstrom::…` — the
  facade's public surface is smaller and the internal crate is the enforced boundary for the
  moved items.
- Sibling crates gained a `malstrom-core-internal` dependency; the kernel's `channels` module
  no longer owns the edge primitives.
- `find target/doc -name '*spsc*'` etc. no longer find them under `malstrom_core`; rustdoc links
  that pointed at `channels::spsc` were updated.
- `cargo check`, `cargo test`, `cargo clippy --all-targets -- -D warnings` and
  `RUSTDOCFLAGS="-D warnings" cargo doc` are green for every locally-buildable crate.
- **Not a complete hiding of the audit's list:** `Forward`, `OperatorBuilder`,
  `runtime::communication` and `types::distributed` remain nameable from core (doc-hidden),
  because moving them requires the edge-unification work. The `cargo-public-api` gate the audit
  suggested was not added; the `malstrom` namespace test is the current regression anchor.

## Related

- [`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
  — the audit this acts on (module inventory, audience matrix, hide list).
- [stream-builder-union-refactor](../architecture/2026-09-14-stream-builder-union-refactor.md)
  — created `OperatorBuilder` and the public `Forward`, two of the coupled items above.
- [unify-operator-io-edge-abstractions](../../proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md)
  and [replace-operator-io-spsc-with-tokio-mpsc](../../proposed/architecture/2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)
  — own the edge layer; the deferred items move with them.
- [split-malstrom-core](../architecture/2026-08-24-split-malstrom-core.md) — the crate split whose
  first-class-kernel decision this extends with an internal boundary.