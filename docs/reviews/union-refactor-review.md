> **Last refreshed:** 2026-09-21

# Review: union refactor toward fewer core internals in operator crates

Scope: the union and split operators
(`malstrom-combinators/src/operators/{union.rs, split.rs}`), the core helpers they build on
(`malstrom-core/src/stream/{operator_builder.rs, forward_logic.rs, operator.rs,
stream_builder.rs}`), and `malstrom-combinators/src/sources/fn_source.rs`. Since the first
revision of this review the union refactor has landed on the new `OperatorBuilder`; the open
work has moved to `split()`.

## What the union refactor does now

| Area | Change |
|---|---|
| `malstrom-core` | Adds `OperatorBuilder` (`operator_builder.rs`) and moves the forwarding logic into core as `Forward` (`forward_logic.rs`, re-exported from `stream`). `StreamBuilder` exposes `with_new_tail`, `swap_tail`, `add_operator`. |
| `malstrom-combinators` | `union()` builds each edge with `OperatorBuilder::new(...).with_direct_logic(Forward::new().into_logic()).build()`, then `self.swap_tail(&mut edge.input)`, `edge.link_to_input(&mut united_input)`, `self.add_operator(edge)`, and finally `self.with_new_tail(united_input)`. `split()` uses the same builder pattern (the former `split_v2` was promoted to the sole `split` and the local duplicate `Forward` was deleted, 2026-09-21). |

`union()` no longer constructs raw `Operator`s, touches `std::mem::swap`, or reaches into
`Operator.input`/`output` directly — it goes through `OperatorBuilder` and the
`swap_tail`/`link_to_input`/`with_new_tail` helpers. The earlier `forward_to` /
`forward_tail_to` `StreamBuilder` method that the previous revision reviewed is **gone**; union
now spells the four steps out itself.

## Lint status on Termux

`malstrom-kafka` (rdkafka) and `malstrom-k8s-proto` (`protoc` in `build.rs`) cannot build in
the Termux environment, and the custom cargo build ignores `--workspace --exclude` for the
proto build, so lint and test crate-scoped:

```
cargo fmt -p malstrom-core -p malstrom-combinators -- --check                 # pass
cargo clippy -p malstrom-core -p malstrom-combinators --all-targets -- -D clippy::correctness  # pass
cargo check -p malstrom-core -p malstrom-combinators --all-targets           # pass
cargo test -p malstrom-combinators --lib                                     # pass (33 tests)
```

## Remaining issues

1. ~~**Unused `Debug` bound in `fn_source.rs`.**~~ **Fixed 2026-09-21.** Reverted
   `FromIteratorSource<V>` to `V: Distributable + Data` and dropped the `std::fmt::Debug`
   import.

2. ~~**`split()` v1 vs `split_v2()` duplication.**~~ **Fixed 2026-09-21.** Promoted the
   `OperatorBuilder` version to the sole `split()`, deleted the v1 raw-`Operator` path
   (`std::mem::swap`, `link`, local `Forward`) and the now-duplicate `rt_split_v1` test.
   `const_split()` already delegated to `split()`, so it follows automatically; renamed its
   test to `const_split_divides`. `split_v2` no longer exists in the API.

3. ~~**Commented-out dead code in `split_v2`.**~~ **Fixed 2026-09-21** (removed when the
   method was promoted).

## Design notes

### `OperatorBuilder` as the encapsulation point

`OperatorBuilder` (`new` / `with_input` / `with_output` / `with_direct_logic` / `build`) is a
better home than a `StreamBuilder` method: it keeps `Operator` construction out of the operator
crates, and `with_output(Output::new_unlinked(partitioner))` is what makes it work for split's
custom partitioner. The four-step `swap_tail` + `link_to_input` + `add_operator` +
`with_new_tail` sequence is duplicated in `union()` and `split_v2()`; consider a single helper
that takes a `partitioner` and an iterator of targets, which would cover broadcast-union
(one target) and split (N targets) alike.

### `Operator.input` / `Operator.output` are still `pub`

The helpers replace inline patterns but do not yet shrink the public surface. `split()` v1 still
uses `std::mem::swap` on `partition_op.input`/`.output` directly; deleting v1 is what lets those
fields become private.

### `StreamBuilder::swap_tail` semantics

`swap_tail` mutates the builder's tail in place (the old tail is swapped into the edge). As with
the former `forward_to`, callers must not reuse the builder's tail afterwards; `with_new_tail`
is the way to continue.

### `Cloned` is a broadcast wrapper, kept deliberately

`malstrom-combinators/src/operators/cloned.rs` is not a distinct operator: `const_cloned` /
`cloned` call `const_split` / `split` with a broadcast partitioner (`[true; N]` / `outs.fill(true)`)
and no user closure. It offers no runtime capability `Split` lacks. It is kept as the ergonomic,
intention-revealing spelling of fan-out — `stream.cloned(name, 2)` versus
`stream.split(name, |_, outs| outs.fill(true), 2)` — and is referenced by the `cloned_streams`
example, the website joining/splitting guide, and doc-links in `split.rs`/`sink.rs`. The
rationale lives in the trait doc comment.

## Small things

- The `then` type-parameter rename `T → B` is cosmetic, no issue.
- `fn_source.rs` debug leftovers from the previous revision (`println!("x={x:?}")`) and the
  now-dead `Debug` bound are both gone.
- `union.rs` has no leftover `itertools` import or test prints — those were resolved.

## Bottom line

The union and split refactors are both complete: `union()` and `split()` are on
`OperatorBuilder` + core `Forward`, with no raw core-internals access, and the local duplicate
`Forward` and `Debug` bound are gone. `cargo test -p malstrom-combinators --lib` is green (32
tests) and the affected-crate correctness clippy passes.
