# Agent Note: Combinators are not operators — separate the two vocabularies

Status: proposed

## Problem

`malstrom-operators` holds two different kinds of thing under one name:

- **Operators** — `Logic`/`SafeLogic` implementations that process *messages*:
  `map`, `filter`, `filter_map`, `flatten`, `inspect`, the `time` operators,
  `stateful_op`/`stateful_map`/`ttl_map`, and the `sink`/`source` operators. Each is a
  1-input/1-output stream node; it depends only on the extension API (`SafeLogic`,
  `Input`/`Output`, `Message`, `OperatorContext`).
- **Combinators** — operations that reshape the *stream graph*: `union` (N→1), `split` /
  `const_split` / `split_v2` (1→N), and `cloned` / `const_cloned` (1→N broadcast). They do not
  process messages themselves; they wire edges and splice `StreamBuilder`s. They depend on
  kernel edge-construction internals — `OperatorBuilder`, `Forward`, `Input`/`Output`, `link` —
  that message operators never touch.

The conflation has costs:

- **The crate name lies.** `malstrom-operators` sounds like "the operators"; it also contains
  the graph combinators, which are closer to the kernel than to `map`/`filter`.
- **It hides the layering.** A combinator needs the kernel's edge-construction abstraction
  (`OperatorBuilder`), so it sits at a different layer than a message operator — but both live
  in one crate with one dependency set.
- **It muddies "where does a new thing go?"** and it is why
  [`OperatorBuilder`](2026-09-21-malstrom-core-internal-crate.md) looked like it might belong in
  the operator crate: the combinators that use it live there. They don't — the abstraction is
  kernel-owned; the *combinators* are the operator-crate residents that happen to need it.

The distinguishing test is **arity and substrate**, not size: does it transform messages
(operator) or the shape of the stream graph (combinator)?

## Proposal

Name and (eventually) package the two separately.

1. **Vocabulary.** Reserve *operator* for a message-processing `Logic`/`SafeLogic` (1→1), and
   *combinator* for a stream-graph operation (arity change or fan-out wiring). Document the
   distinction where users meet it (the operator guide).
2. **Keep the kernel abstractions in the kernel.** `OperatorBuilder` and `Forward` are
   core-owned (they hide `Operator`'s internals); combinators use them, they do not own them.
   See [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md).
3. **Introduce `malstrom-combinators`** for the graph combinators (`union`, `split`,
   `const_split`, `cloned`), leaving message operators in `malstrom-operators`. The facade
   re-exports both (`malstrom::combinators::{Union, Split, Cloned}`; the operators stay at
   `malstrom::operators`). This gives the combinator/kernel coupling its own crate edge, so the
   operator crate's dependencies shrink to the extension API.
4. **Do not rename the existing crate wholesale.** `malstrom-operators` is dominantly message
   operators; renaming it to `*-combinators` would be backwards. The split out is small and the
   names then match their contents.

## Alternatives considered

- **Leave it as one crate, document the split in module docs.** No new crate, no API churn — but
  the boundary stays advisory and the combinators keep pulling kernel edge helpers into the
  operator crate's dependency story. Rejected as the end state; acceptable as an interim.
- **Rename `malstrom-operators` → `malstrom-combinators`.** Rejected: the crate is mostly
  operators, so the name would then lie in the other direction.
- **Move the combinators into `malstrom-core`.** They are pure `StreamBuilder` extensions and
  would sit naturally next to `StreamBuilder`; but the kernel should not grow user-facing
  combinators, and it would put `union`/`split` in the same crate as the runtime. Considered;
  the separate crate keeps the kernel lean and the boundary explicit.
- **Move `OperatorBuilder`/`Forward` to `malstrom-operators`** (so combinators are self-
  contained). Rejected — inverts the abstraction's ownership; see
  [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md).

## Acceptance criteria

- The terms *operator* and *combinator* are used consistently (docs, note titles, crate/module
  names); a message processor is never called a combinator and vice versa.
- A `malstrom-combinators` crate (or an explicitly-owned submodule) contains `union`, `split`,
  `const_split`, `split_v2`, `cloned`, `const_cloned`, and depends on the kernel edge
  abstractions; `malstrom-operators` no longer depends on `OperatorBuilder`/`Forward`.
- The facade exposes both under clear paths (`malstrom::operators::…`, and combinators under
  their own path).
- `cargo test --workspace` green; the `union`/`split`/`cloned` tests move with the combinators.

## Risks

- **Public-path churn.** `malstrom::operators::{Union, Split, Cloned}` today; moving them
  changes user imports. Provide re-exports at the old path for a transition (or accept the
  break while pre-1.0).
- **Over-splitting.** A tiny crate for three combinators is fine only if the boundary stays
  meaningful; if more graph operations appear, it earns its keep, otherwise the documented
  split is enough.
- **Entangling with the extraction.** Do this *after* the
  [malstrom-core-internal](2026-09-21-malstrom-core-internal-crate.md) boundary is settled, so
  the two moves don't race.

## Related

- [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md) — the kernel-owned
  `OperatorBuilder`/`Forward` the combinators build on.
- [stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md)
  — the refactor where combinators moved onto `OperatorBuilder`.
- [`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
  — the surface audit this vocabulary sharpens.