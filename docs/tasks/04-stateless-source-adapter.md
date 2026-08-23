# T4 — Finish the stateless → stateful source adapter

> **Status:** [ ] open · **Errors:** 12 · **Files:** `sources/stateless.rs` (lines 62–130)

## Context

`sources/stateful.rs` defines the **new** traits (zero generics, associated types, async):

- `StatefulSourceImpl` (`stateful.rs:26`): assoc. types `Part`, `Value`, `Timestamp`,
  `PartitionState`, `SourcePartition`; `async fn list_parts(&mut self)`,
  `async fn build_part(&mut self, part, part_state)`.
- `StatefulSourcePartition` (`stateful.rs:102`): assoc. types `PartitionState`, `Value`,
  `Timestamp`; `async fn poll`, `async fn snapshot(&self)`, `async fn collect(self)`.
  **No `suspend` method.**
- `StatefulSource<SrcImpl>` (`stateful.rs:60`): exactly **one** generic parameter.

`sources/stateless.rs` was half-migrated (last commit "fixed stateful source"): it still
implements the **old** signatures — 2 generics on the traits, sync methods, no
`Value`/`Timestamp` assoc. types, a `suspend` that doesn't exist, and
`StatefulSource::<(S::Part, V, T), _>` with a bogus second generic.

## Errors (all in `sources/stateless.rs`)

`E0407` (108: `suspend` not in trait) · `E0107` (64, 90: 2 generics on 0-generic traits) ·
`E0053` (75: `list_parts(&self)` vs `&mut self`) · "must be async" (79: `build_part`) ·
`E0207` (90: unconstrained `V`, `T`) · `E0046` (64, 90: missing `Value`/`Timestamp`) ·
`E0107` + `E0277` + `E0599` (129: `StatefulSource` takes 1 generic; tuple not a
`StatefulSourceImpl`).

## Fix — replace lines 62–115 with the new-signature impls

```rust
impl<V, T, S> StatefulSourceImpl for SourceWrapper<V, T, S>
where
    V: Data,
    T: Timestamp,
    S: StatelessSourceImpl<V, T>,
    S::Part: Key,
{
    type Part = S::Part;
    type Value = V;
    type Timestamp = T;
    type PartitionState = ();
    type SourcePartition = PartitionWrapper<S::SourcePartition>;

    async fn list_parts(&mut self) -> Vec<Self::Part> {
        self.0.list_parts()
    }

    async fn build_part(
        &mut self,
        part: &Self::Part,
        _part_state: Option<Self::PartitionState>,
    ) -> Self::SourcePartition {
        PartitionWrapper(self.0.build_part(part))
    }
}

impl<V, T, S> StatefulSourcePartition for PartitionWrapper<S>
where
    V: Data,
    T: Timestamp,
    S: StatelessSourcePartition<V, T>,
{
    type PartitionState = ();
    type Value = V;
    type Timestamp = T;

    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)> {
        self.0.poll().await
    }

    async fn snapshot(&self) -> Self::PartitionState {}

    async fn collect(self) -> Self::PartitionState {
        self.0.suspend();   // StatelessSourcePartition::suspend still exists — call it on shutdown
    }
    // fn suspend(&mut self) { … }  ← DELETE: not a member of StatefulSourcePartition
}
```

And fix the construction at line 129 (drop the tuple/bogus generic — `self.0` is already a
`SourceWrapper`):

```rust
builder.source(name, StatefulSource::new(self.0))
```

The `StreamSource<(S::Part, V, T)>` impl (line 117) needs no change: it matches
`StatefulSource`'s `StreamSource<(SrcImpl::Part, SrcImpl::Value, SrcImpl::Timestamp)>`
impl in `stateful.rs:71`.

## Verify

```bash
cargo check -p malstrom   # errors 9–20 gone (12 errors)
```

## Risks

- `StatefulSourceImpl: 'static` — confirm `V`/`T`/`S` satisfy it via the existing bounds
  (`Data`/`Timestamp` are `'static` in `types/`).
- `S::Part: Key` bound is required; the `StatelessSource` `into_stream` already carries it.
- This is the area of the last commit — a careful review of the diff
  (`git diff main...HEAD -- malstrom-core/src/sources/`) is recommended before merging.
