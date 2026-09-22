# Agent Note: `malstrom-combinators` — the builder-extension surface

Status: proposed

## Problem

Everything user-facing in `malstrom-operators` is an extension of the stream builder: a trait
implemented for `StreamBuilder<…>` (or `InitialStreamBuilder` for the seed case). That is the
whole public surface:

```
Map, Filter, FilterMap, Flatten, Inspect, Sink, Split, Union, Cloned,
StatefulMap, StatefulOp, TtlMap, AssignTimestamps, GenerateEpochs,
InspectFrontier, KeyLocal, KeyDistribute, WorkerBroadcast, Distribute   → StreamBuilder<…>
Source                                                                   → InitialStreamBuilder
```

plus the traits a user implements to plug into those (`SourceImpl`, `StatelessSinkImpl` /
`StatefulSinkImpl`, `StatefulLogic`, `TTLState`, …). None of this is an *operator*: the crate
name describes a role the crate's contents do not play.

The **Operator API** — the basis every one of these is authored against — lives in
`malstrom-core`: `Logic`/`SafeLogic`/`LogicBuilder`, `Operator`/`OperatorBuilder`,
`StreamBuilder`, `Input`/`Output`, `Message`, the types. An "operator" is the runtime node that
API describes; a thing that *extends the builder* is a **combinator**.

The mismatch costs:

- **The name misleads.** `malstrom-operators` reads as "the operators"; it is actually the
  combinator DSL, and the operators (in the `Logic`/`SafeLogic` sense) are kernel vocabulary.
- **It blurs ownership.** The combinators consume kernel abstractions (`OperatorBuilder`,
  `Forward`) — which made `OperatorBuilder` look like it belonged in the operator crate, when
  it is kernel-owned. See [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md).
- **It gives the substrate no name.** "Operator" should name the core node abstraction
  (`SafeLogic` + the wiring API), not the library of builder methods.

## Proposal

Use one test — **does it extend the builder?** — and rename to match.

1. **Definition.** A **combinator** is a `StreamBuilder`/`InitialStreamBuilder` extension. All
   such traits, including the impl/extension-point traits folded into the same surface
   (`SourceImpl`, `*SinkImpl`, `StatefulLogic`, `TTLState`, …), are the **combinator surface**.
   An **operator** is a `Logic`/`SafeLogic` node; the **Operator API** is the kernel machinery
   for authoring and wiring one (`SafeLogic`, `Operator`, `OperatorBuilder`, `StreamBuilder`,
   `Input`/`Output`, `Message`).
2. **Rename the crate** `malstrom-operators` → **`malstrom-combinators`**, and the facade path
   `malstrom::operators` → **`malstrom::combinators`**.
3. **The Operator API stays in `malstrom-core`** — it is the basis, not a combinator. In
   particular `OperatorBuilder`/`Forward` remain kernel-owned.
4. **Document the layering:** users compose *combinators*; they extend the DSL by implementing
   the combinator surface's extension points; they build new combinators against the *Operator
   API* in the kernel.

## Alternatives considered

- **Keep the name `malstrom-operators`.** Zero churn — but the name keeps describing the wrong
  role and keeps inviting the "does `OperatorBuilder` belong here?" confusion.
- **Split graph-wiring combinators (`union`/`split`/`cloned`) into a separate crate.** Earlier
  framing, based on arity (N→1, 1→N). Rejected: arity is a proxy; by the actual test every
  StreamBuilder extension is a combinator, so the whole crate is combinator surface and a split
  would put `map` and `union` in different crates for no conceptual reason.
- **Move the combinator surface into `malstrom-core`.** It is builder extensions and would sit
  next to `StreamBuilder`; but the kernel should not grow user-facing combinators and plugin
  traits, and it would put `map`/`union` in the same crate as the runtime. Rejected.
- **Rename only the package, keep the facade path `malstrom::operators`.** Less user churn, but
  leaves the wrong name in the user-facing path. Rejected in favour of renaming both.

## Acceptance criteria

- A **combinator** is anything extending `StreamBuilder`/`InitialStreamBuilder`; an **operator**
  is a `Logic`/`SafeLogic`. The terms are used consistently in docs, guides and note titles.
- The crate is `malstrom-combinators`; the facade exposes it at `malstrom::combinators`.
- `malstrom-core` owns the Operator API (`SafeLogic`, `Operator`, `OperatorBuilder`,
  `StreamBuilder`, IO, types); no combinator lives in the kernel.
- Migration is mechanical: rename the package and its directory, update `malstrom::operators` →
  `malstrom::combinators` at call sites (examples, tests, docs, `namespace.rs`), re-export the
  old path during a transition if desired.
- `cargo test --workspace` and the lint/doc gates stay green.

## Risks

- **Facade-path churn.** `malstrom::operators::…` is used across examples and the guide; the
  rename touches all of them. Pre-1.0, accept the break (or add a deprecated `operators`
  re-export for one cycle).
- **Overlap with the core-internal boundary.** Sequence after
  [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md) settles, so the
  crate-graph moves don't race.
- **"Operator" still appears** inside the crate (the `Logic` impls each combinator wraps).
  That is fine — they are operators; the crate is named after its public surface, the
  combinators.

## Related

- [malstrom-core-internal-crate](2026-09-21-malstrom-core-internal-crate.md) — the kernel-owned
  `OperatorBuilder`/`Forward` the combinators build on.
- [stream-builder-union-refactor](../../implemented/architecture/2026-09-14-stream-builder-union-refactor.md)
  — where the combinators moved onto the kernel's `OperatorBuilder`.
- [`docs/overviews/08-public-api-surface.md`](../../../../docs/overviews/08-public-api-surface.md)
  — the surface audit this vocabulary sharpens.