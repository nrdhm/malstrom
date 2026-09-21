# Agent Note: Collapse kernel test support into `malstrom-testkit`

Status: proposed

## Problem

The kernel's test-only helpers live in **two** places, and the shared mock module is compiled
**four** times:

- `malstrom-core/src/test_support.rs` — `init_logs()`, gated behind the `test-support`
  feature (`malstrom-core/Cargo.toml`), which exists only to expose it to integration tests.
  It is part of the lib, so it is compiled once (and duplicated a second time in
  `malstrom-operators`, see below).
- `malstrom-core/tests/common/mod.rs` — 192 lines of in-process mocks (`MemoryComm`,
  `MemoryFlavor`, and the four `MemoryStream*`/`MemoryReqRes*` sender/receiver pairs),
  declared with `mod common;` by four integration-test binaries and therefore compiled once
  per binary.

Two consequences follow.

### 1. `tests/common/mod.rs` is compiled once per test binary

Each file in `tests/` is its own crate root, so `mod common;` textually re-expands the module
into every binary that declares it. Verified on the pinned toolchain (cargo 1.97.1 /
rustc 1.97.1, per `rust-toolchain.toml`) in a clean target dir:

```console
$ CARGO_TARGET_DIR=/tmp/tgt-count cargo test -p malstrom-core --no-run -q
$ grep -l "tests/common/mod.rs" /tmp/tgt-count/debug/deps/*.d
completion-d64dac9710c6e29f.d
rescale-fbe7b449efabb6ac.d
runtime_flavor_contract-4a451d37ea4c1a57.d
safe_logic_contract-552d2dcc5c05858a.d      # 4 compilation inputs
```

That is the number of compilations, not a timing artifact. (`target/debug/deps/` also holds
~92 such `.d` files repo-wide; the rest are stale leftovers from earlier toolchains.)

Only **one** of the four binaries actually uses the module's contents —
`runtime_flavor_contract.rs`. The other three declare `mod common;` without referencing
anything in it, so they each compile 192 unused lines and emit dead-code warnings. Counting
locations reported against `tests/common/mod.rs` under `cargo test -p malstrom-core --no-run`
gives 10 sites carrying 9 distinct messages (`associated function `new` is never used` fires
twice; the rest are `never constructed` / `never used` on `MemoryComm`, `MemoryFlavor`, the
four sender/receiver structs, `MemoryResponder`, and the `ReqRes` type alias).

### 2. `init_logs()` is duplicated across crates

`malstrom-core/src/test_support.rs` and `malstrom-operators/src/operators/union.rs` each
define an `init_logs()`. The operators copy is richer — it also wires OTLP export when
`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` is set (see
[otlp-tracing-in-operator-tests](../../implemented/testing/2026-09-13-otlp-tracing-in-operator-tests.md))
— but the `tracing_subscriber` registry/`EnvFilter`/`with_test_writer()` core is the same.
Test-only logging setup is exactly what `malstrom-testkit` already exists to host, and
`malstrom-operators` already dev-depends on it.

Both `test_support` and `tests/common/` are kernel-local only because of a belief recorded in
two live Agent Notes, examined below.

## Proposal

Move both surfaces into `malstrom-testkit`, which already owns the in-memory comm mocks
(`NoCommunication`) and the operator tester, and is the crate every non-kernel test crate
already depends on.

Concretely:

1. **Move `tests/common/mod.rs` into `malstrom-testkit`** as a module next to
   `communication.rs` (e.g. `malstrom-testkit/src/memory_comm.rs`, re-exported from
   `lib.rs`). Add `flume` (currently only a kernel dependency) to `malstrom-testkit`'s
   `[dependencies]`; `async-trait` and `tokio` are already there.
2. **Move `init_logs()` into `malstrom-testkit`**, and delete
   `malstrom-core/src/test_support.rs` together with the `test-support` feature, its
   `dep:tracing-subscriber` optional dependency, and the `malstrom-core` self-dev-dependency
   in `malstrom-core/Cargo.toml` that exists only to enable that feature.
3. **Point the kernel's tests at `malstrom_testkit::`** and delete `malstrom-core/tests/common/`.
   `runtime_flavor_contract.rs` drops its unused-`mod common` siblings' declarations; all four
   binaries drop `mod common;`.
4. **Dedup `malstrom-operators`' `init_logs()`** onto the testkit one, keeping the OTLP
   branch as a wrapper or an additional testkit helper.
5. **Correct the two Agent Notes** whose rationale this falsifies (below), in the same change.

### The dev-dep cycle is not a blocker — with one precise exception

[core-test-plan](../../implemented/testing/2026-08-25-core-test-plan.md) rejects this move as
*"impossible for the kernel: a kernel dev-dep on testkit (which depends on the kernel)
duplicates the kernel and breaks type identity"*, and
[split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
Decision 8 states a kernel dev-dep on testkit "makes cargo build a **second kernel
instance**".

**That does not reproduce for integration tests.** With `malstrom-testkit` added to
`malstrom-core`'s `[dev-dependencies]`, cargo accepts the cycle (`cargo metadata` succeeds)
and resolves features to a single kernel unit:

```console
$ CARGO_TARGET_DIR=/tmp/tgt-cycle2 cargo test -p malstrom-core --no-run   # exit 0
$ ls /tmp/tgt-cycle2/debug/deps/ | grep -cE '^libmalstrom_core-[0-9a-f]+\.rlib$'
1
```

Also verified workspace-wide with the dev-dep added (`CARGO_TARGET_DIR=… cargo test
--workspace --no-run`): exactly one `libmalstrom_core-*.rlib` was produced before the run
failed in `malstrom-operators` on its pre-existing errors (see Risks), so unification holds
across the whole dependency graph, not just the kernel package. A synthetic mirror exercising
the seam confirms coherence — a testkit type passed back as a kernel type, and a testkit
`impl` of a kernel trait, both compile and run from a kernel **integration** test.

The exception is real and narrow: a **library unit test** (`#[cfg(test)] mod tests` inside
`src/`) cannot use testkit items that mention kernel types. For the `lib test` target cargo
recompiles the lib, while testkit links against the *normal* lib unit, so the two kernel
crates are distinct. A mirror reproduces it exactly:

```text
error[E0308]: mismatched types
note: there are multiple different versions of crate `kernel` in the dependency graph
```

Kernel-type-free items are unaffected: `init_logs()` returns `()`, and a kernel lib unit test
calling `testkit::init_logs()` compiles and passes. So the single in-`src/` consumer
(`stream/operator_builder.rs`) is safe, while the mocks — which necessarily mention
`OperatorOperatorComm`, `WorkerCoordinatorComm`, `RuntimeFlavor`, and `KVT` types — remain
integration-test-only. They already are today: `tests/common/mod.rs` is unreachable from
`src/` unit tests. **The move therefore removes no capability.**

Because this constraint is subtle and easy to hit later, the implementing change should pin
it: a doc comment on the mocks stating they are integration-test-only, and (optionally) a
kernel `#[cfg(test)]` compile-fail or comment near `operator_builder.rs` recording that
lib unit tests may only call `()`-returning testkit helpers.

### Also record: why the kernel's own `test_support` was the alternative

Keeping the mocks in the kernel behind the existing `test-support` feature (i.e. moving
`tests/common/mod.rs` to `src/test_support/` instead of to testkit) also achieves a single
compilation, needs no new dependency, and keeps kernel tests importing only
`malstrom_core::`. It is the smaller change. It was **not** chosen because it cannot dedup
`init_logs` across crates — `malstrom-operators` would keep its own copy — and because it
leaves the `test-support` feature and self-dev-dep machinery in place purely to publish test
fixtures from a production lib. Testkit is the crate that exists for this purpose.

## Alternatives considered

- **Status quo** — zero churn, mocks compile four times, `init_logs` duplicated, 10 warnings
  from an unused shared module, and the `test-support` feature plus self-dev-dep remain. The
  lowest-risk option, but it leaves test fixtures shipped from the kernel's public feature
  surface. Rejected as the state to keep, not as a fallback.
- **Move the mocks into `malstrom_core::test_support` instead of testkit** (see above) —
  single compilation, no new crate edge, preserves `core-test-plan`'s "import only
  `malstrom_core::`" principle for Layer 1a. Rejected: does not dedup `init_logs`, and keeps
  a test-only feature on the production lib.
- **Delete the three unused `mod common;` declarations only** — already yields a single
  compilation today, with a one-line diff. Rejected: not structurally shared, so the next
  integration test that needs the mocks re-duplicates the module, and the fixtures stay in
  `tests/` rather than in the crate built for them.
- **Leave `init_logs` in the kernel and move only the mocks** — a smaller step that avoids
  touching the `test-support` feature. Rejected: splits the same concern across two crates
  and forfeits the operators-side dedup, which is half the value.
- **Trust the recorded blocker and change nothing** — rejected, because the blocker does not
  reproduce on the pinned toolchain (evidence above). Recording a stale rationale as fact is
  the failure this note exists to correct.

## Acceptance criteria

- `tests/common/` no longer exists under `malstrom-core`; the mocks are reachable from
  `malstrom_testkit::`.
- `malstrom-core/src/test_support.rs` is gone; `test-support` appears nowhere in
  `malstrom-core/Cargo.toml`, and the `malstrom-core` self-dev-dependency is removed.
- `malstrom-core/tests/common/mod.rs` is named by **zero** `.d` files in a clean target:
  `T=$(mktemp -d) CARGO_TARGET_DIR=$T cargo test -p malstrom-core --no-run`, then
  `grep -rl "tests/common/mod.rs" "$T/debug/deps/"*.d` returns nothing.
- `cargo test -p malstrom-core` is green. Baseline to match (current tree): 59 lib unit
  tests; integration binaries `completion` 2, `rescale` 1, `runtime_flavor_contract` 3,
  `safe_logic_contract` 1 (7 total); 8 doc tests.
- The `never constructed` / `never used` warnings attributable to `tests/common/mod.rs`
  (10 sites) are gone.
- The kernel lib unit test in `stream/operator_builder.rs` still compiles and passes via the
  testkit `init_logs()` — this is the regression gate for the lib-unit-test constraint above.
- `python3 scripts/verify-agent-notes.py` passes. At the time of writing the repo's
  `.venv/bin/python3` symlink was dangling (it points into a Termux path that does not exist
  in this Alpine container) while system Python 3.14.7 works and was used.
- `core-test-plan`'s Alternatives entry and `split-malstrom-core` Decision 8 are corrected to
  match the toolchain's actual behavior, keeping both notes cross-linked.

## Risks

- **The two Agent Notes currently assert the opposite.** Correcting them is part of the
  change, not optional; leaving them stale re-arms the same wrong conclusion. Partial
  supersession only — the hosting decisions themselves stand, so both notes are edited and
  cross-linked rather than deleted.
- **The lib-unit-test constraint is easy to trip.** A future `#[cfg(test)]` unit test in
  `src/` that imports a mock will fail with a confusing "multiple different versions of crate
  `malstrom-core`" error. Mitigated by documenting it on the mocks and pinning
  `operator_builder.rs` as a passing case.
- **New kernel → testkit dev-dependency edge.** Kernel integration tests stop importing only
  `malstrom_core::`, which `core-test-plan` treats as a principle for Layer 1a. This is a real
  loss of that property and the main argument for the rejected kernel-local alternative;
  accepted deliberately for cross-crate dedup.
- **`init_logs` is not a pure duplicate.** The operators variant also configures OTLP
  export; consolidating must preserve that behavior, so the OTLP path stays in
  `malstrom-operators` (as a wrapper over the testkit helper) rather than moving.
- **Timing benefit is modest.** Touching `tests/common/mod.rs` costs ~14–16 s versus ~11 s for
  a single non-shared test file — the wall-clock saving is roughly 3 s, because relinking the
  ~54 MB test binaries dominates. The justification is correctness-of-structure and the
  warning/duplication cleanup, not build speed. Do not sell this as a performance win.
- **`cargo test --workspace` does not currently build.** `malstrom-operators` fails with 175
  errors on the untouched tree (verified independently of any probe). Workspace-wide
  verification of this change must therefore wait on, or exclude, that crate; the kernel and
  testkit packages are the meaningful gates.
- **Verification was done in mirrors, not the real tree.** The single-instance and coherence
  results come from a synthetic kernel/testkit/loggy mirror plus `malstrom-core` resolution
  with the dev-dep added (no testkit-consuming test code was written into the repo). The
  implementing change is the first real exercise of the seam and should run the full kernel
  suite.

## Related

- [core-test-plan](../../implemented/testing/2026-08-25-core-test-plan.md) — owns the kernel
  test strategy and the Alternatives entry this note corrects.
- [split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md) —
  Decision 8 records the dev-dep-cycle blocker; corrected here.
- [otlp-tracing-in-operator-tests](../../implemented/testing/2026-09-13-otlp-tracing-in-operator-tests.md)
  — the source of the richer, OTLP-aware `init_logs` variant.
