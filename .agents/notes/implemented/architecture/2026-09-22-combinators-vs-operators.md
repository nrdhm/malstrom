# Agent Note: Combinators are not operators — rename to `malstrom-combinators`

Status: implemented

## Problem

`malstrom-operators` held one kind of thing under a name that described another. Everything
user-facing in it was an extension of the stream builder — a trait implemented for
`StreamBuilder<…>` (or `InitialStreamBuilder` for the seed case):

```
Map, Filter, FilterMap, Flatten, Inspect, Sink, Split, Union, Cloned,
StatefulMap, StatefulOp, TtlMap, AssignTimestamps, GenerateEpochs,
InspectFrontier, KeyLocal, KeyDistribute, WorkerBroadcast, Distribute   → StreamBuilder<…>
Source                                                                   → InitialStreamBuilder
```

plus the traits a user implements to plug into those (`SourceImpl`, `StatelessSinkImpl` /
`StatefulSinkImpl`, `StatefulLogic`, `TTLState`, …). None of these is an *operator*: the
**Operator API** — `Logic`/`SafeLogic`/`LogicBuilder`, `Operator`/`OperatorBuilder`,
`StreamBuilder`, `Input`/`Output`, `Message`, the types — lives in `malstrom-core`, and an
operator is the runtime node that API describes. A thing that *extends the builder* is a
**combinator**.

The mismatch misled in practice: it made `OperatorBuilder` look like it might belong in the
operator crate, when it is a kernel abstraction that hides `Operator`'s internals (see
[malstrom-core-internal-crate](../../proposed/architecture/2026-09-21-malstrom-core-internal-crate.md)).

## Decision

One test — **does it extend the builder?** — and the crate renamed to match.

- **Vocabulary.** A **combinator** is a `StreamBuilder`/`InitialStreamBuilder` extension. All
  such traits, including the extension-point traits folded into the same surface (`SourceImpl`,
  `*SinkImpl`, `StatefulLogic`, `TTLState`, …), are the **combinator surface**. An **operator**
  is a `Logic`/`SafeLogic` node; the **Operator API** is the kernel machinery for authoring and
  wiring one.
- **Renamed `malstrom-operators` → `malstrom-combinators`** (package, directory, workspace
  member, dependencies) and the internal module `operators` → `combinators`.
- **Facade path `malstrom::operators` → `malstrom::combinators`**, and the facade feature
  `operators` → `combinators`. Call sites updated across examples, tests, `namespace.rs`, the
  website guides, the docs and the `TTLState` derive's generated path.
- **The Operator API stays kernel-owned** in `malstrom-core`; in particular
  `OperatorBuilder`/`Forward` remain there.

## Alternatives considered

- **Keep the name `malstrom-operators`.** Zero churn — but the name keeps describing the wrong
  role and keeps inviting the "does `OperatorBuilder` belong here?" confusion.
- **Split graph-wiring combinators (`union`/`split`/`cloned`) into their own crate.** Earlier
  framing, based on arity (N→1, 1→N). Rejected: arity is a proxy; by the actual test every
  builder extension is a combinator, so the whole crate is combinator surface and a split would
  separate `map` from `union` for no conceptual reason.
- **Move the combinator surface into `malstrom-core`.** It is builder extensions and would sit
  next to `StreamBuilder`; but the kernel should not grow user-facing combinators and plugin
  traits, and it would put `map`/`union` in the same crate as the runtime. Rejected.
- **Rename only the package, keep the facade path `malstrom::operators`.** Less user churn, but
  leaves the wrong name in the user-facing path. Rejected in favour of renaming both.

## Consequences

- The crate name now matches its contents, and "operator" is reserved for the kernel node
  abstraction, so the layering reads correctly: users compose **combinators**; they build new
  ones against the **Operator API**.
- **User-facing path churn:** `malstrom::operators::…` → `malstrom::combinators::…`. Pre-1.0,
  taken as a clean break (no deprecated alias added).
- The crate keeps its internal `Logic`/`SafeLogic` implementations (each combinator wraps an
  operator); that is expected — the crate is named after its public surface, the combinators.
- `cargo test` (combinators 32, kernel 51) and the clippy/rustdoc/format gates are green.

## Related

- [malstrom-core-internal-crate](../../proposed/architecture/2026-09-21-malstrom-core-internal-crate.md) —
  the kernel-owned `OperatorBuilder`/`Forward` the combinators build on.
- [stream-builder-union-refactor](../architecture/2026-09-14-stream-builder-union-refactor.md)
  — where the combinators moved onto the kernel's `OperatorBuilder`.
- [`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
  — the surface audit this vocabulary sharpens.