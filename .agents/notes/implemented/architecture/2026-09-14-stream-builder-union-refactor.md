# Agent Note: StreamBuilder helpers and the `OperatorBuilder` union/split refactor

Status: implemented

## Problem

Stream-building combinators in `malstrom-combinators` (union, split) historically needed direct
access to core internals to wire streams:

- `StreamBuilder::tail` and `Operator::input`/`output` are `pub`, so combinator code read,
  swapped, and rebuilt them by hand (`std::mem::swap`, `link()`, `Rc`, `PhantomData`).
- Each combinator that needed a no-op forward operator carried its own private `Forward` logic
  implementation (`union.rs` had one; `split.rs` had one).

The goal was a builder-level API that lets the operator crates assemble fan-in/fan-out without
importing how physical stream edges are built, so a future combinator does not repeat the
swap + link + register triple.

## Decision

Core exposes the construction primitives and owns the shared forward logic; the operator
crates build combinators through them and never touch `StreamBuilder::tail` or
`Operator::input`/`output` directly.

### `OperatorBuilder` (core)

`malstrom-core/src/stream/operator_builder.rs` provides
`OperatorBuilder::new(name)` / `with_input` / `with_output` / `with_direct_logic` / `build`.
Combinators wire edges with it plus:

- `StreamBuilder::swap_tail(&mut Input)` — swaps the builder's tail into the new operator's
  input in place.
- `StreamBuilder::with_new_tail(Input)` — returns a builder with the same runtime `Rc` and a
  new tail.
- `Operator::swap_input` / `Operator::link_to_input` — the named replacements for the inline
  swap/link triple.

### Shared `Forward` logic (core)

The no-op forwarding logic lives once in `malstrom-core/src/stream/forward_logic.rs` as
`Forward`, re-exported from `malstrom_core::stream`. The private `Forward` in `union.rs` and
the duplicate in `split.rs` are both deleted.

### Combinators

- `union()` builds one edge per input stream with `OperatorBuilder`, `.with_direct_logic(
  Forward::new().into_logic())`, then `swap_tail` + `link_to_input(united_input)` +
  `add_operator`, and returns `self.with_new_tail(united_input)`. No direct
  `tail`/`input`/`output`/`link`/`mem::swap`/`Rc` access remains.
- `split()` uses the same builder pattern. The `OperatorBuilder` version replaced the former
  `split_v2` and the old raw-`Operator` path (`std::mem::swap`, `link`, local `Forward`) was
  deleted, so `split` / `const_split` have a single implementation. `with_output(
  Output::new_unlinked(partitioner))` is what carries split's custom partitioner.

`Cloned` remains a thin wrapper over `Split` (a broadcast partitioner with no user closure); see
the rationale in `malstrom-combinators/src/operators/cloned.rs`.

## Alternatives considered

- **Status quo** — fix `union()` and leave `split()` as-is. Rejected: keeps the duplicated
  `Forward` and the raw-operand pattern, and every future combinator repeats the swap + link +
  register triple.
- **A generalized `StreamBuilder::forward_tail_to(name, partitioner, targets)` helper** — the
  original design (the review's Option A): one method covering union's broadcast/one-target and
  split's custom-partitioner/N-target cases. Superseded by `OperatorBuilder`: a builder keeps
  `Operator` construction out of the operator crates and composes with `with_output`, whereas a
  `StreamBuilder` method would have to grow to carry the partitioner and a heterogeneous target
  list. The four steps are now spelled out at each call site (two call sites), which is
  acceptable duplication for the clearer ownership boundary.
- **Shared helper code inside `malstrom-combinators`** — keep `Forward`/wiring in the operator
  crate. Rejected: leaves the operator crates coupled to core internals, which is the coupling
  this refactor removes.
- **Defer to the IO edge unification first** — the
  [unify-operator-io-edge-abstractions](../../proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md)
  proposal is complementary, not a prerequisite: this is builder-level construction and landed
  independently of the transport-level edge traits.

## Consequences

- `union()`, `split()`, and `const_split()` compile without importing `link`,
  `std::mem::swap`, `Rc`, `PhantomData`, `StreamBuilder::tail`, or `Operator::input`/`output`
  from core. `split.rs`'s duplicate `Forward` is gone.
- `StreamBuilder { tail, runtime }` struct literals are no longer used from the operator
  crates; `with_new_tail` is the construction path.
- **Still public:** `OperatorBuilder`, `Forward`, `StreamBuilder::swap_tail`/`with_new_tail`,
  and `Operator::swap_input`/`link_to_input` are all `pub`, and `Operator::input`/`output`
  are still `pub`. The public surface grew rather than shrank; a later visibility pass can
  close `Operator::input`/`output` once no combinator needs them.
- **Builder misuse is still possible:** `swap_tail` leaves the builder's tail disconnected, so
  a caller that reuses the builder without `with_new_tail` gets a silently broken stream. This
  was realized once: the union rewrite dropped the `add_operator(edge)` registration and the
  edge was silently discarded — see
  [fail-loud-on-dangling-operator-edges](../../proposed/architecture/2026-09-19-fail-loud-on-dangling-operator-edges.md).
- `cargo test -p malstrom-combinators --lib` and the union/split test semantics are unchanged.

## Related

- [unify-operator-io-edge-abstractions](../../proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md)
- [replace-operator-io-spsc-with-tokio-mpsc](../../proposed/architecture/2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md)
- [fail-loud-on-dangling-operator-edges](../../proposed/architecture/2026-09-19-fail-loud-on-dangling-operator-edges.md) — the builder-misuse risk above, realized.