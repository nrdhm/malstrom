> **Last refreshed:** 2026-09-06

# Review: union refactor toward fewer core internals in operator crates

Scope: staged diff in `malstrom-core/src/stream/{operator.rs, operator_logic.rs, stream_builder.rs}`,
`malstrom-core/src/worker/builder.rs`, `malstrom-operators/src/operators/union.rs`, and
`malstrom-operators/src/sources/fn_source.rs`. The user plans to apply the same refactor to
`split()` next.

## What the diff does

| Area | Change |
|---|---|
| `malstrom-core` | Adds `StreamBuilder::forward_to` / `with_new_tail`, `Operator::swap_input` / `link_to`, moves the `Forward` logic into `stream_builder.rs`; cleans up imports in `operator.rs`, `operator_logic.rs`, `worker/builder.rs` |
| `malstrom-operators` | Rewrites `union()` on top of the new core helpers; removes the local `Forward` and the `union_2` stub; moves the test into the file. `fn_source.rs` contains debug leftovers |

The direction is correct: `union()` no longer touches `StreamBuilder::tail`, `Operator::input/output`,
`std::mem::swap`, `link`, `Rc`, or `PhantomData`. That materially reduces coupling between the
operator crates and core internals.

## Lint status on Termux

`malstrom-kafka` is the only rdkafka-dependent crate and correctly must be excluded locally, but it
is not the only Termux blocker: `malstrom-k8s-proto` needs `protoc` and its `build.rs` panics without
it. In addition, the custom cargo build appears to ignore `--workspace --exclude` for the proto
build, so the reliable local command is crate-scoped:

```
cargo fmt -p malstrom-core -p malstrom-operators -- --check                 # pass
cargo clippy -p malstrom-core -p malstrom-operators --all-targets -- -D clippy::correctness  # pass
cargo check -p malstrom-core -p malstrom-operators --all-targets           # pass
cargo test -p malstrom-operators --lib operators::union::tests::union_unites  # pass
```

The full test run was SIGKILLed by the environment after core integration tests passed; the union
test passes in isolation.

Local lint recommendation: use `-p <touched crates>` instead of workspace-wide
`--workspace --exclude malstrom-kafka ...`.

## Must fix before committing

1. **Debug leftovers in `fn_source.rs`**
   - `println!("x={x:?}")` in `FromIteratorPartition::poll` fires on every poll.
   - The new `V: Debug` bound on both `FromIteratorSource<V>` and `FromIteratorPartition<V>`
     exists only to support that print. Revert to `V: Distributable + Data`.
2. **Unused import** `itertools::Itertools` in `union.rs:46` (use std `.chain()`).
3. **Commented-out dead code** in `operator.rs` (`// pub fn link_from_2(...)`); it also triggers
   the "empty line after doc comment" clippy warning at `operator.rs:53`.
4. **Debug prints in the union test** (`println!("p test start")` / `eprintln!("e test start")`).

## Design notes

### `StreamBuilder::forward_to`

Good encapsulation of the swap + link + register triple. Caveat: after the call, `self.tail`
becomes a disconnected `Input` (the old tail is swapped into the forward op), so callers must not
reuse the builder afterwards. The doc should say this explicitly. Since it is an advanced
combinator used from another crate, consider `#[doc(hidden)]` or a more precise name such as
`forward_tail_to`.

### `StreamBuilder::with_new_tail`

Good: clones the runtime `Rc`, keeps `StreamBuilder` construction out of operator-crate code.

### `Operator::swap_input` / `Operator::link_to`

Reasonable first step, but note `Operator.input` / `Operator.output` are still `pub`; this diff
replaces inline patterns with named operations rather than shrinking the public surface.

## Before applying the same to `split()`

The current `forward_to` does **not** cover split because split needs:

1. a custom partitioner on the forward op's output (`Output::new_unlinked(partitioner)`), and
2. linking N downstream inputs to that one output.

Recommended options:

- **Option A (preferred): generalize `forward_to` to multi-target + partitioner**:
  `forward_to(name, partitioner, targets: impl IntoIterator<Item = &mut Input<M>>)`.
  `union()` uses a broadcast partitioner and one target; `split()` uses the split partitioner and N
  targets. One helper covers both combinators.
- **Option B**: keep single-target `forward_to` and add a separate core helper for split
  (e.g. `split_with(name, partitioner, targets)`). More API surface, simpler each method.

After the split refactor, delete the duplicated `Forward` struct in
`malstrom-operators/src/operators/split.rs` (core's `stream_builder.rs` already provides one).

## Small things

- `operator_logic.rs` / `worker/builder.rs` changes are pure import cleanup; they compile cleanly.
- Removing the unused `let name = ...` in `Operator::start` is correct: `to_build_context` consumes
  `self.name`, and nothing else needed the local.
- The `then` type-parameter rename `T → B` is cosmetic, no issue.

## Bottom line

The union refactor is a solid encapsulation improvement and passes the affected-crate lints/tests.
Finish the must-fix list, then generalize the helper for split (Option A) rather than forcing the
current single-target `forward_to` to handle split's multi-target/custom-partitioner case.