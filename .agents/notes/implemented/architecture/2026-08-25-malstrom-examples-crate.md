# Agent Note: Extract a `malstrom-examples` crate and free the kernel of dev-dependencies

Status: implemented

## Problem

`malstrom-core`'s `[dev-dependencies]` section was entirely example-driven, and two of its
entries recreated the dev-dependency cycle that [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
Decision 8 was written to prevent:

| kernel dev-dep | used by kernel `src`? | actually served |
|---|---|---|
| `malstrom-operators` | ❌ | the 5 framework examples |
| `malstrom-distributed` | ❌ | same |
| `console-subscriber` | ❌ | `multithreading.rs` |
| `tracing-subscriber` | ❌ | example logging (`rescaling.rs`) |
| `proptest` | ❌ | **dead** (the property tests moved to `malstrom-operators`' `fn_source`) |

The kernel's 18 unit tests needed **no** dev-dependencies, so the whole section existed only
for examples. `malstrom` →dev→ `malstrom-operators` → `malstrom` is a dev-dependency cycle —
it resolved to a single `malstrom` instance (verified: one rlib in both example and test
builds), but it is the same shape that duplicated the kernel crate for `malstrom-testkit` in
Decision 8; a future re-resolution could reintroduce the type-identity hazard. The examples
were also scattered across three crates: the kernel (5 framework-level), `malstrom-operators`
(16 operator-level), and `malstrom-snapshot-slatedb` (2).

## Decision

Create a **`malstrom-examples`** leaf crate (workspace member, `publish = false`, no `lib`
target) that hosts the runnable examples, and free the kernel (and shrink
`malstrom-operators`) of example-only dev-dependencies:

1. **`malstrom-examples`** — depends on the full stack (`malstrom`, `malstrom-operators`,
   `malstrom-distributed`) plus `indexmap`/`serde`/`tokio` as `[dependencies]`, and on the
   demo tooling (`console-subscriber`, `tracing-subscriber`, `chrono`, `expiremap` — the
   `ttl_map` example imports `expiremap::ExpireMap` directly) as `[dev-dependencies]`. Being
   a leaf, it depends on everything without a cycle — acyclicity is structural, not
   conventional.
2. **All 21 non-SlateDB examples moved there** — the 5 framework-level (`basic_noop`,
   `multithreading`, `rescaling`, `stateful_programs`, `stateful_program_multiple_keys`) and
   the 16 operator-level (`look_ma_im_streaming`, `basic_operators`, `basic_stdout`,
   `custom_stateless_operator`, `custom_stateful_operator`, `event_time`,
   `event_time_out_of_order`, `keyed_streams`, `split_streams`, `union_streams`,
   `cloned_streams`, `ttl_map`, `file_source_stateful`, `file_source_stateless`,
   `file_sink_stateful`, `file_sink_stateless`). They run via
   `cargo run -p malstrom-examples --example <name>`.
3. **SlateDB stays where it is** — `malstrom-snapshot-slatedb/examples/` and its unit tests
   stay in the connector crate (it owns its persistence examples).
4. **Kernel `[dev-dependencies]` deleted entirely** — `malstrom-core` is dev-dependency-free.
   The kernel's one doctest (`runtime/threaded/multi.rs`) previously imported
   `malstrom_operators`; it was rewritten kernel-only — a plain `Logic` source plus a
   pass-through operator built via `Operator::built_by` — which doubles as a live demo of the
   kernel's public extension API.
5. **`malstrom-operators` dev-dependencies are `malstrom-testkit` only** — `chrono` and
   `expiremap` (dev) left with the examples.
6. **`malstrom-examples/README.md` groups the examples** into *Framework-level* (exercises
   the engine — scheduling, multi-threading, rescaling, stateful programs) and
   *Operator-level* (demonstrates individual operators/sources/sinks), with a per-example
   table and the borderline test. The old `malstrom-core/examples/README.md` was deleted.
7. **Docs updated** — every website guide include and the overviews (`01-project.md` example
   table + run commands, `03-dependencies.md` dev-dep notes) point at `malstrom-examples`;
   the stale `"malstrom-core/examples/*"` workspace glob and the emptied
   `malstrom-core/examples/` directory are gone.

## Alternatives considered

- **Status quo (examples scattered + kernel dev-deps)** — zero churn, but the kernel keeps a
  dev-dependency cycle with its own layers and pulls example tooling into its dev graph.
  Rejected.
- **Move only the 5 framework examples** — frees the kernel and kills the cycle, but
  `malstrom-operators` keeps its `chrono`/`expiremap` example dev-deps and examples stay
  split across two crates. Rejected as a partial cleanup.
- **Directory grouping (`examples/framework/…`, `examples/operators/…`)** — visually groups,
  but Cargo only auto-discovers nested `examples/*/main.rs`, so it fights the tooling; the
  README grouping carries the *why*. Rejected.
- **Two example crates (stdlib-examples + kernel-examples)** — rejected because a leaf crate
  can host both groups and the distinction is a documentation concern, not a dependency
  concern.
- **Kernel-only doctest** — a review finding: deleting the kernel dev-deps breaks the
  `multi.rs` doctest unless it is rewritten kernel-only. Doing so (a `Logic` source +
  pass-through via `Operator::built_by`) turns the blocker into a live demo of the extension
  API.

## Consequences

- **Kernel is dev-dependency-free** — `cargo test -p malstrom` (18 unit + 1 doc) depends on
  nothing external; the dev-dep cycle with the operator layer is gone, so acyclicity is
  structural.
- **One examples home** — all framework/operator examples live in `malstrom-examples`; the
  SlateDB examples live in the connector; `malstrom-kafka` ships its own. Each crate's
  example set tells its own story.
- **`malstrom-testkit` stays in `malstrom-operators`** (test-only, used by its 31 unit
  tests), per split-note Decision 7.
- **Docs churn completed** — website guides and overviews reference the new paths; no
  `malstrom-core/examples` or `malstrom-operators/examples` references remain.
- **Verification** — `cargo check --workspace` clean (0 warnings); tests green: `malstrom`
  18 unit + 1 doc (the rewritten extension-API doctest), `malstrom-operators` 31 unit + 9
  doc, `malstrom-testkit` 1, `malstrom-snapshot-slatedb` 5; all 21 examples build and
  `look_ma_im_streaming`, `stateful_programs`, `multithreading`, `rescaling`, `ttl_map`,
  `event_time` smoke-run correctly.
