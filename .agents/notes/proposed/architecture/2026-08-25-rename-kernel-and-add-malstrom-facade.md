# Agent Note: Rename the kernel to `malstrom-core` and add a `malstrom` facade

Status: proposed

## Problem

After [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md), two
naming/ergonomic issues remain:

1. **Crate name ≠ folder name.** The kernel lives in `malstrom-core/` but its Cargo package is
   `name = "malstrom"`. The `malstrom` name now describes only one of five crates, which is
   misleading.
2. **There is no single public entry point — a "crate zoo".** To write a program a user must
   know which crate owns which module: `malstrom::runtime`/`malstrom::snapshot`/`malstrom::stream`
   (kernel) but `malstrom_operators::operators`/`malstrom_operators::sources` and
   `malstrom_distributed::…`. The split's Decision 5 explicitly did not add a kernel re-export
   of `malstrom::operators` (it would cycle the graph), so the pre-split one-crate surface is
   gone and users face the internal crate boundaries.

## Proposal

Two steps, both preserving the acyclic `malstrom`-is-the-base graph:

1. **Rename the kernel crate** `malstrom` → `malstrom-core` (`malstrom-core/Cargo.toml`
   `name = "malstrom-core"`, crate `malstrom_core`). Update the intra-workspace dependencies
   and `use malstrom::…` → `use malstrom_core::…` in the four layer crates. The kernel's own
   `crate::…` paths are unaffected.
2. **Add a `malstrom` facade crate** (`malstrom/`, crate `malstrom`) that depends on the zoo
   and re-exports the historical module tree, restoring the pre-split single-crate surface:

```rust
// malstrom/src/lib.rs — the public facade, no logic of its own
pub use malstrom_core::{channels, coordinator, runtime, snapshot, stream, types, worker};
pub use malstrom_operators::{keyed, operators, sinks, sources};

#[cfg(feature = "slatedb")]
pub use malstrom_snapshot_slatedb::{SlateDbBackend, SlateDbClient};
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
```

The facade is the **default door**; individual crates stay first-class for users who want only
the kernel or only one layer (`futures`/`futures-core` model, not the `rayon` "hidden core"
model).

## Alternatives considered

- **Status quo (kernel named `malstrom`, no facade)** — zero churn, but the name lies about
  its scope and users must learn the crate zoo. Rejected.
- **Rename only (kernel → `malstrom-core`, no facade)** — fixes the naming but leaves the
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

## Acceptance criteria

- `malstrom-core/Cargo.toml` says `name = "malstrom-core"`; the four layer crates depend on
  `malstrom-core` (path) and compile.
- A new `malstrom` facade crate is a workspace member; `use malstrom::operators::*`,
  `use malstrom::sources::…`, `use malstrom::runtime::…`, and `use malstrom::keyed::distributed::…`
  all resolve.
- `default` features pull operators + distributed; `slatedb` gates the connector; `cargo check
  --workspace` and `cargo test --workspace` are green.
- README/website guide/examples import `malstrom::…` (the facade) rather than the layer crates.
- The graph stays acyclic (`malstrom` facade depends on the layers, nothing depends on it).

## Risks

- **crates.io name/version.** `malstrom 0.1.0` is already published as the old kernel. The
  facade takes over the name at the next version (`malstrom 0.2.0`); the published `0.1.0`
  kernel is left as a frozen superseded artifact. Pre-1.0 this is cheap — do it now, not after
  a stable release.
- **`malstrom-kafka`/`malstrom-k8s` pin the published `malstrom 0.1.0`** — they must migrate
  to `malstrom-core`/the facade, coupling this to
  [point-k8s-and-kafka-at-local-malstrom](../../process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md).
- **The old `malstrom::snapshot::slatedb` path breaks.** `snapshot` is a kernel module, so the
  facade cannot inject `SlateDbBackend` back into it; the connector re-exports at a new path
  (`malstrom::slatedb` or a facade-owned shim). Document it as an intentional path change.
- **Facade must stay a facade.** The facade must not grow logic or a `lib` that other crates
  depend on (it depends on everything; a dependent would create a cycle). `malstrom-testkit`
  stays out of the facade (it is a dev/test dependency, not a runtime surface).
- **Feature matrix.** Start with the three features above; do not add per-layer granularity
  until a real consumer needs it.
