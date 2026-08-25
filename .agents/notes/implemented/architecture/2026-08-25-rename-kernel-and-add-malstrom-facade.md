# Agent Note: Rename the kernel to `malstrom-core` and add a `malstrom` facade

Status: implemented

## Problem

After [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md), two
naming/ergonomic issues remained:

1. **Crate name ≠ folder name.** The kernel lived in `malstrom-core/` but its Cargo package was
   `name = "malstrom"`. The `malstrom` name described only one of five crates, which was
   misleading.
2. **There was no single public entry point — a "crate zoo".** To write a program a user had to
   know which crate owns which module: `malstrom::runtime`/`malstrom::snapshot`/`malstrom::stream`
   (kernel) but `malstrom_operators::operators`/`malstrom_operators::sources` and
   `malstrom_distributed::…`. The split's Decision 5 explicitly did not add a kernel re-export
   of `malstrom::operators` (it would cycle the graph), so the pre-split one-crate surface was
   gone and users faced the internal crate boundaries.

## Decision

Two steps, both preserving the acyclic `malstrom`-is-the-base graph:

1. **Renamed the kernel crate** `malstrom` → `malstrom-core` (`malstrom-core/Cargo.toml`
   `name = "malstrom-core"`, crate `malstrom_core`). The four layer crates' manifests now
   depend on `malstrom-core` (path) and their `use malstrom::…` imports became
   `use malstrom_core::…`. The kernel's own `crate::…` paths were unaffected. Two dead
   leftover files in the kernel (`src/keyed/mod.rs`, `src/testing/persistence.rs`) were
   removed.
2. **Added a `malstrom` facade crate** (`malstrom/`, crate `malstrom`, version `0.2.0`) that
   depends on the zoo and re-exports the historical module tree, restoring the pre-split
   single-crate surface:

```rust
// malstrom/src/lib.rs — the public facade, no logic of its own
pub use malstrom_core::{channels, coordinator, runtime, snapshot, stream, types, worker};
#[cfg(feature = "operators")]
pub use malstrom_operators::{keyed, operators, sinks, sources};
#[cfg(feature = "slatedb")]
pub mod slatedb {
    pub use malstrom_snapshot_slatedb::*;
}
```

```toml
# malstrom/Cargo.toml
[features]
default = ["operators", "distributed"]
operators = ["dep:malstrom-operators"]
distributed = ["dep:malstrom-distributed", "operators"]
slatedb = ["dep:malstrom-snapshot-slatedb"]
```

`malstrom::keyed::distributed` keeps working because `malstrom-operators` already carries the
one-line `pub use malstrom_distributed::*` shim.

### Resulting shape

```
malstrom (facade, leaf) ──▶ malstrom-core, malstrom-operators, malstrom-distributed, (slatedb)
malstrom-operators ──▶ malstrom-core, malstrom-distributed
malstrom-distributed ──▶ malstrom-core
malstrom-testkit ──▶ malstrom-core
malstrom-snapshot-slatedb ──▶ malstrom-core
malstrom-examples ──▶ malstrom (facade)
```

The facade is the **default door**; individual crates stay first-class for users who want only
the kernel or only one layer (`futures`/`futures-core` model, not the `rayon` "hidden core"
model).

### Consumers migrated back to `malstrom::…`

- `malstrom-examples` depends on the `malstrom` facade and its examples import
  `malstrom::operators::…`/`malstrom::sources::…`/`malstrom::sinks::…`/`malstrom::keyed::…`
  again (the pre-split surface). `malstrom-operators` remains a dev-dependency only because
  the `TTLState` derive macro expands to `malstrom_operators::operators::TTLState` by name.
- README and the website guides' code blocks import `malstrom::…` again.
- The `malstrom-snapshot-slatedb` examples use the layer paths (`malstrom_core::runtime`,
  `malstrom_operators::sources`) — the connector cannot depend on the facade without a cycle.

## Alternatives considered

- **Status quo (kernel named `malstrom`, no facade)** — zero churn, but the name lied about
  its scope and users had to learn the crate zoo. Rejected.
- **Rename only (kernel → `malstrom-core`, no facade)** — fixed the naming but left the
  multi-import ergonomics; rejected as a half-measure once the facade is cheap.
- **Re-export the operators through the kernel** (pre-split `malstrom::operators` path) — the
  split note's Decision 5 already rejected this: `malstrom → malstrom-operators →
  malstrom-distributed → malstrom` is a cycle cargo forbids. The facade exists precisely to
  hold that re-export without a cycle.
- **One big feature-gated `malstrom` crate instead of the split** — already rejected in the
  split note; the split bought real dependency isolation. The facade is an umbrella *over* the
  split, not a return to it.
- **`rayon`-style hidden core** (`malstrom` public, `malstrom-core` semi-internal) — hides a
  legitimately useful kernel; keep all sub-crates first-class and let the facade be the
  convenience.

## Consequences

- **`malstrom` 0.2.0 is the facade; the kernel is `malstrom-core` 0.1.0.** The crates.io
  name `malstrom` is taken over by the facade at the next version; the published `0.1.0`
  kernel is left as a frozen superseded artifact. Pre-1.0 this is cheap.
- **`malstrom-kafka`/`malstrom-k8s` still pin crates.io `malstrom 0.1.0`** — with the local
  `malstrom` now at 0.2.0, cargo necessarily resolves their dependency from crates.io; they
  still compile in the workspace. Migrating them to the local `malstrom-core`/facade is
  coupled to [point-k8s-and-kafka-at-local-malstrom](../../process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md).
- **`malstrom::snapshot::slatedb` is gone** — `snapshot` is a kernel module, so the facade
  cannot inject `SlateDbBackend` back into it. The connector is re-exported at the new path
  `malstrom::slatedb` (feature `slatedb`). Documented intentional path change.
- **The facade must stay a facade** — it must not grow logic or be depended on by other
  crates (it depends on everything; a dependent would create a cycle). `malstrom-testkit`
  stays out of the facade (dev/test dependency, not a runtime surface).
- **Feature matrix is minimal** — `default` = operators + distributed; `slatedb` gates the
  connector; no per-layer granularity until a real consumer needs it.
- **Verification** — `cargo check --workspace` clean (0 warnings); tests green: `malstrom-core`
  19 unit, `malstrom-operators` 31 unit + 9 doc, `malstrom-testkit` 1, `malstrom-snapshot-slatedb`
  5; all 21 examples build and smoke-run via the facade (`look_ma_im_streaming`,
  `stateful_programs`, `multithreading`, `rescaling`, `ttl_map`); `malstrom::keyed::distributed`
  resolves through the facade.
