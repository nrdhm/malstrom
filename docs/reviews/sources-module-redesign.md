# Design — rethinking the `sources` module

> **Last refreshed:** 2026-08-22 (new-scheduler @ a4c8fce)
> **Companion:** [sources-module-review.md](sources-module-review.md) — the smells this fixes

## Guiding principles

1. **One source abstraction.** "Stateless" is the special case `PartitionState = ()`, not a
   second trait hierarchy.
2. **Framework owns the plumbing.** Discovery, distribution, completion and rescaling are the
   framework's job; a source author only describes *what partitions exist* and *how to read one*.
3. **Control signals are first-class.** Discovery and completion are not smuggled through data
   messages.
4. **The common case is one closure.** A single-stream/poll source should be a one-liner.

---

## 1. The collapsed trait

```rust
/// A partitioned, possibly stateful source.
///
/// Stateless sources simply set `PartitionState = ()` (or use `Source::from_*`).
pub trait SourceImpl: 'static {
    /// Identifies a partition (one shard / split / file / topic-partition / …).
    type PartitionKey: Distributable + Key;

    /// Values this source emits.
    type Value: Distributable + Data;

    /// Timestamps this source emits (`NoTime` if untimed).
    type Timestamp: Distributable + Timestamp;

    /// Per-partition state persisted across restarts and moved on rescale.
    /// `()` = stateless.
    type PartitionState: Distributable;

    /// The reader produced by [SourceImpl::open].
    type Partition: SourcePartition<
        PartitionKey = Self::PartitionKey,
        Value = Self::Value,
        Timestamp = Self::Timestamp,
        State = Self::PartitionState,
    >;

    /// Discover the partitions this source exposes.
    ///
    /// Called by the framework at build time (worker 0) and again on rescale.
    /// May perform I/O (list a bucket, query a broker, …).
    async fn discover(&mut self) -> Vec<Self::PartitionKey>;

    /// Open a reader for `key`, resuming from `state` if one was persisted.
    ///
    /// Pure relative to discovery: the framework calls this for each discovered
    /// (or restored) partition; a source never hands out readers via self-mutation.
    async fn open(
        &mut self,
        key: &Self::PartitionKey,
        state: Option<Self::PartitionState>,
    ) -> Self::Partition;
}
```

```rust
/// One partition reader. The framework owns its lifecycle.
pub trait SourcePartition {
    type PartitionKey: Distributable + Key;
    type Value: Distributable + Data;
    type Timestamp: Distributable + Timestamp;
    type State: Distributable;

    /// Return `None` once no further records will be produced by this partition.
    /// MUST be cancel-safe.
    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)>;

    /// Capture the state to resume from later.
    async fn snapshot(&self) -> Self::State;

    /// Shut down and return the final state (moves to another worker / job end).
    async fn collect(self) -> Self::State;
}
```

### What this removes

- The whole `stateless.rs` adapter (`SourceWrapper`, `PartitionWrapper`, both `PhantomData`s,
  the `StatelessSource` shell and the round-trip through `StatefulSource::new`).
- The generics-vs-associated-types split (`StatelessSourceImpl<V, T>` vs `StatefulSourceImpl`).
- `suspend` and the dead `is_finished` / `PartitionsFinished`.

### What "stateless" now looks like

```rust
struct MySource; // a real stateless source, explicitly

impl SourceImpl for MySource {
    type PartitionKey = NoKey;
    type Value = String;
    type Timestamp = NoTime;
    type PartitionState = ();                    // stateless
    type Partition = MyPartition;

    async fn discover(&mut self) -> Vec<NoKey> { vec![NoKey] }
    async fn open(&mut self, _: &NoKey, _: Option<()>) -> MyPartition { MyPartition }
}

impl SourcePartition for MyPartition {
    type PartitionKey = NoKey;
    type Value = String;
    type Timestamp = NoTime;
    type State = ();

    async fn poll(&mut self) -> Option<(String, NoTime)> { … }
    async fn snapshot(&self) {}                  // three no-op lines for stateless,
    async fn collect(self) {}                    // or sugar them with a derive/macro
}
```

The three no-op lines are the *only* stateless ceremony left (versus ~50 lines of adapter
today). A `#[derive(StatelessSource)]` in the existing `malstrom-macros` crate can generate
them, mirroring how `TTLState` already sugars the TTL operator.

> **Rust-mechanics note:** we deliberately do *not* put `type PartitionState: Distributable = ()`
> with default method bodies, because a default `snapshot() { () }` would not type-check once a
> source overrides `PartitionState = MyState`. `Default`-seeded state (`PartitionState: Default`)
> is an alternative if you prefer "unset state" semantics over the explicit `Option` in `open`.

---

## 2. Convenience constructors — the common case is one closure

```rust
// sources/from_poll_fn.rs  (or a single `sources/fn_source.rs`)

pub struct PollSource<F, V, T> { f: F, _m: PhantomData<(V, T)> }

impl<F, V, T> PollSource<F, V, T> {
    pub fn new(f: F) -> Self { Self { f, _m: PhantomData } }
}

impl<F, V, T, Fut> SourceImpl for PollSource<F, V, T>
where
    F: FnMut() -> Fut + 'static,
    Fut: Future<Output = Option<(V, T)>>,
    V: Distributable + Data,
    T: Distributable + Timestamp,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = T;
    type PartitionState = ();
    type Partition = PollPartition<F, V, T>;

    async fn discover(&mut self) -> Vec<NoKey> { vec![NoKey] }
    async fn open(&mut self, _: &NoKey, _: Option<()>) -> PollPartition<F, V, T> {
        PollPartition { f: Rc::new(RefCell::new(std::mem::take(&mut self.f))) }
    }
}
```

`Source::from_stream(s: impl Stream<Item = (V, T)>)` and
`Source::from_iterator(i: impl IntoIterator<Item = V>)` are the same shape over a `Stream`
and an iterator respectively. `SingleIteratorSource` becomes `Source::from_iterator`, and its
"timestamp = index" behaviour — which is really positional *state*, see review §12 — should
default to `NoTime`, with an explicit `enumerate()`-based variant if the index is wanted.

Usage then reads:

```rust
provider.new_stream()
    .source("numbers", Source::from_poll_fn(|| async { read_one().await }))
    .sink("out", …);
```

The framework's `Source` (the sealed operator-API side) stays as-is; only the *authoring*
surface becomes `SourceImpl` + the `Source::from_*` constructors.

---

## 3. Framework-owned discovery, distribution, completion

The operator graph the framework builds from a `SourceImpl` is essentially the current one,
but with the three pieces made first-class:

```
[discover] ──(PartitionKey control msgs)──▶ [distribute/rendezvous] ──▶ [reader]
                                                   ▲
                              reader ops report   │  (framework-owned frontier merge)
                              "partition done" ───┘
```

- **Discovery** runs on worker 0 at build/rescale and emits a first-class
  `Message::SourcePartitions(Vec<PartitionKey>)` (or feeds the reader ops' initial state
  directly), instead of `DataMessage(part, NoData, MIN)` through a distribute operator.
- **Completion** falls out of the framework's existing epoch/barrier alignment: a reader whose
  partition returns `None` simply stops; when *all* partitions (across workers) are exhausted,
  the `distribute` operator's frontier merge emits `Epoch(MAX)`. No bespoke `PartitionFinished`
  over a shared `COMM_CHANNEL_ID`, no hardcoded worker 0.
- **Rescale** keeps using the keyed `Acquire`/`Collect`/`Interrogate` protocol (the reader op
  already implements `on_acquire`/`on_collect`/`on_interrogate`) — that part is sound and
  stays.

### One operator model

Both the discovery operator and the reader operator implement `SafeLogic` (no more raw `Logic`
with a hand-rolled eight-arm `match`). The `PartLister`'s manual dispatch disappears.

---

## 4. Naming

| Today | Proposed | Meaning |
|---|---|---|
| `Part` / `SrcImpl::Part` | `PartitionKey` | identifies a shard/file/topic-partition |
| `SourcePartition` / `Partition` | `Partition` (reader) | reads one partition |
| `PartitionState` | `State` | per-reader resume state |
| `PartLister` / `PartitionsFinished` | — (framework-internal) | removed / renamed |
| `StatelessSource(Impl/Partition)` | — | gone (folded into `PartitionState = ()`) |

---

## 5. Migration path (incremental, not a rewrite)

1. **Rename + merge the traits** inside `stateful.rs` first (it already has the associated-type
   shape): `PartitionKey`, `State`, `discover`/`open`. This is mechanical.
2. **Delete `stateless.rs`** by adding the three-line no-op `snapshot`/`collect` to every
   stateless impl, plus a `#[derive(StatelessSource)]` in `malstrom-macros` if you want it.
3. **Replace the discovery-as-data hack** with a first-class control message; keep the
   distribute/reader ops otherwise intact.
4. **Replace the `PartitionFinished` protocol** with "reader exhaustion → frontier merge →
   `Epoch(MAX)`", reusing the `merge_frontiers` alignment already present in
   `keyed/distributed/remote_receiver.rs`.
5. **Add `Source::from_poll_fn` / `from_stream` / `from_iterator`** and migrate the examples;
   `SingleIteratorSource` becomes `from_iterator` (untimed by default).
6. **Delete** `keyed_old/`, `stateful_old.rs` and the commented `is_finished` after the new
   path is green.

Each step compiles and the existing doctests/examples keep passing — the redesign reuses the
current engine rather than throwing it away.
