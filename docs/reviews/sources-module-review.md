# Review — the `sources` module (StatelessSource in particular)

> **Last refreshed:** 2026-08-22 (new-scheduler @ a4c8fce)
> **Scope:** `malstrom-core/src/sources/{mod,stateless,stateful,single_iterator}.rs`
> **Companion:** [sources-module-redesign.md](sources-module-redesign.md) — the proposed replacement

## Verdict

`stateless.rs` is a 130-line file whose entire job is to **pretend a stateless source is a
stateful one with empty state**. That is the smell everything else flows from: the adapter
(`SourceWrapper` + `PartitionWrapper`) is the code confessing there is really only *one*
source engine, and "stateless" was bolted on top of it rather than designed beside it.

---

## The core smell: stateless-as-fake-stateful

`stateless.rs:62` says it outright — *"NewType on which we can implement StatefulSourceImpl"*.
The whole file translates the simple stateless API into the heavyweight stateful one:

- `SourceWrapper<V,T,S>: StatefulSourceImpl` with `PartitionState = ()` (line 74)
- `PartitionWrapper<S,V,T>: StatefulSourcePartition` with `snapshot() {}` and
  `collect(self) { self.0.suspend() }` (lines 98, 106–110)
- `into_stream` then unwraps the adapter and re-wraps it: `StatefulSource::new(self.0)`
  (line 129)

The layering is `StatelessSource` → `SourceWrapper` → `StatefulSource` → `into_stream`.
`StatelessSource` is a public shell around an adapter that is immediately discarded and
re-wrapped. If "stateless" were a first-class idea it would not need two adapter types and a
round-trip through the stateful trait.

---

## Concrete smells

### 1. Two parallel trait hierarchies, two conventions
`StatelessSourceImpl<V, T>` takes `V, T` as **generic parameters**; `StatefulSourceImpl`
expresses the same thing as **associated types** (`Value`, `Timestamp`). The adapter then has
to translate back: `type Value = V; type Timestamp = T` (`stateless.rs:72-73`). The `Kvt`
trait was introduced to kill exactly this verbosity — the sources didn't get the memo.

### 2. `PhantomData` everywhere is a symptom, not a solution
`SourceWrapper` (`stateless.rs:62`) and `PartitionWrapper` (`stateless.rs:90`) both carry
`PhantomData<(V, T)>`. They need it *only* because `V, T` live in the stateless trait's
generics instead of its associated types, so the wrapper cannot name them from `S` alone.
Move `Value`/`Timestamp` into the trait and both PhantomDatas evaporate.

### 3. Inconsistent parameter order
`SourceWrapper<V, T, S>` vs `PartitionWrapper<S, V, T>`. Trivial, but a sign nobody has read
this file with fresh eyes.

### 4. Self-consuming `build_part` + `unreachable!()`
`SingleIteratorSource::build_part` does `self.0.take()` and panics
`unreachable!("only has one part")` on a second call (`single_iterator.rs:64-68`). The
contract "each part is built exactly once, in some order, via `&mut self`" is implicit and
fragile. This is the `InputFormat`-era Flink model that Flink itself replaced in Source API v2
because listing-then-consuming-with-`&mut-self` doesn't compose.

### 5. Part discovery rides the data plane as a fake `DataMessage`
`PartLister` emits discovered parts as `DataMessage::new(part.clone(), NoData, Timestamp::MIN)`
(`stateful.rs:192`) — a **control** message (which partitions exist) smuggled through the
**data** channel with a meaningless value (`NoData`) and a meaningless timestamp (`MIN`).
The comment at `stateful.rs:166` is the author's own confession: *"Bit hacky using the
SrcImpl timestamp type here instead of OnceTime."*

### 6. `PartLister` re-sends every part on every `apply()`
`for part in self.parts.iter() { output.send(...) }` (`stateful.rs:190-194`) re-runs on each
operator schedule (the operator loop re-invokes `apply`). The downstream `add_partition`
dedupes via `contains_key`, so it "works" — but the parts are re-emitted on every tick. Emit
once; don't re-derive every schedule.

### 7. Two operator-authoring models in the same file
`PartLister` implements raw `Logic` and hand-dispatches eight `Message::*` arms
(`stateful.rs:196-207`); `StatefulSourcePartitionOp` implements `SafeLogic`
(`stateful.rs:307-406`). `SafeLogic` exists precisely so operators don't get the internal
messaging invariants wrong — the part-lister opted out. Inconsistent and error-prone.

### 8. Lifecycle asymmetry + dead code
`StatelessSourcePartition` has `suspend(&mut self)` (`stateless.rs:58`); `StatefulSourcePartition`
does not — the "halt-but-resumable vs cleanup" lifecycle exists on only one side. Both traits
carry commented-out `is_finished` (`stateful.rs:117`, `single_iterator.rs:82`), and
`struct PartitionsFinished;` (`stateful.rs:127`) is fully dead.

### 9. `list_parts`/`build_part` are sync on stateless, async on stateful
A stateless source cannot do async discovery (list a bucket, query a broker), while a stateful
one can — an arbitrary capability gap the adapter papers over by wrapping sync bodies in
`async`.

### 10. The `CommUtility` completion protocol is bespoke and brittle
Part-finished flows over a shared `COMM_CHANNEL_ID = u64::MAX`, hardcoded to *worker 0*, with
the comment *"a stray delivery on another worker must not panic — just ignore it"*
(`stateful.rs:210-212`). A protocol whose author has to defend against misrouted messages is
a red flag.

### 11. Naming
`Part` (a key), `Partition` / `SourcePartition` (a reader), `PartitionState` (per-reader
state), `PartLister`, `PartitionWrapper`, `StatefulSourcePartitionOp` — "part" is overloaded
five ways.

### 12. A "stateless" source with hidden state
`SingleIteratorSource` is documented stateless, yet its timestamp is the iterator index —
i.e. positional state that is silently lost on restart. Fine for an example, but it illustrates
that "stateless" is being used to mean "I didn't think about the state", not "there is none".

---

See [sources-module-redesign.md](sources-module-redesign.md) for the fix.
