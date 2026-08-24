# Agent Note: Collapse the source traits

Status: implemented

## Problem

The `sources` module's core smell: `stateless.rs` was a 130-line file whose entire job was to
**pretend a stateless source is a stateful one with empty state**. The adapter
(`SourceWrapper` + `PartitionWrapper`) confessed there was really only *one* source engine,
with "stateless" bolted on top of it. The layering was `StatelessSource` → `SourceWrapper` →
`StatefulSource` → `into_stream`. See the
[module review](../../../../docs/reviews/sources-module-review.md) for the full audit; the
concrete issues:

1. **Two parallel trait hierarchies, two conventions** — `StatelessSourceImpl<V, T>` used
   generic parameters where `StatefulSourceImpl` used associated types; the adapter
   translated back (`type Value = V`).
2. **`PhantomData` everywhere** — both wrappers carried `PhantomData<(V, T)>` only because
   `V, T` lived in the stateless trait's generics.
3. **Inconsistent parameter order** — `SourceWrapper<V, T, S>` vs `PartitionWrapper<S, V, T>`.
4. **Self-consuming `build_part` + `unreachable!()`** — `SingleIteratorSource::build_part`
   did `self.0.take()` and panicked on a second call.
5. **Part discovery rode the data plane** — `PartLister` emitted discovered parts as
   `DataMessage(part, NoData, Timestamp::MIN)`.
6. **`PartLister` re-sent every part on every `apply()`** — re-derived each schedule, deduped
   downstream by `contains_key`.
7. **Two operator-authoring models in one file** — raw `Logic` (hand-dispatched eight-arm
   `match`) next to `SafeLogic`.
8. **Lifecycle asymmetry + dead code** — `suspend` only on the stateless side; commented-out
   `is_finished`; `struct PartitionsFinished;` dead.
9. **`list_parts`/`build_part` sync on stateless, async on stateful** — an arbitrary
   capability gap.
10. **Bespoke `CommUtility` completion protocol** — part-finished flowed over a shared
    `COMM_CHANNEL_ID = u64::MAX`, hardcoded to worker 0.
11. **Naming** — "part" was overloaded five ways.
12. **A "stateless" source with hidden state** — `SingleIteratorSource`'s index timestamps
    were positional state, silently lost on restart.

## Decision

The [redesign](../../../../docs/reviews/sources-module-redesign.md) was implemented, with two
documented deviations (completion protocol, discovery message) and one rejected sub-proposal
(`#[derive(StatelessSource)]`).

1. **One `SourceImpl`/`SourcePartition` pair** in `malstrom-core/src/sources/stateful.rs`.
   `SourceImpl` carries associated types `PartitionKey`/`Value`/`Timestamp`/
   `PartitionState`/`Partition` plus `async fn discover(&mut self) -> Vec<PartitionKey>` and
   `async fn open(&mut self, key, state: Option<PartitionState>) -> Partition`.
   `SourcePartition` carries `poll`/`snapshot`/`collect`, all async; the framework owns its
   lifecycle. **Stateless is `PartitionState = ()`** — there is no separate stateless trait,
   and no `#[derive(StatelessSource)]` macro (the derive idea was dropped: the ceremony left
   is three no-op lines, not worth a proc-macro).
2. **`stateless.rs`, `single_iterator.rs` deleted; `fn_source.rs` added** — the old adapter
   shell is gone. `fn_source.rs` holds the convenience constructors on
   `sources::Source<SrcImpl>` (an unbounded struct; `impl Source<()>` is the carrier so
   `Source::from_iterator(…)` needs no turbofish):
   - `from_iterator` — untimed (`Timestamp = OnceTime`, records at `OnceTime::MIN`, finishes
     with `OnceTime::MAX`). Replaces `SingleIteratorSource`; **untimed by default** as the
     redesign specified.
   - `from_enumerated_iterator` — `Timestamp = usize` index (the old `SingleIteratorSource`
     semantics, explicit).
   - `from_poll_fn` / `from_stream` — closure/`Stream`-driven sources.
   - `from_impl` — wrap any `SourceImpl`.
3. **Framework-owned discovery (deviation: no control message)** — a `SourceCoordinator`
   (worker 0, raw `Logic`) discovers partitions once (`sent` flag, `debug_assert!` worker 0),
   hands them to a `.distribute(rendezvous_select)` step, which routes them to the
   `SourcePartitionOp` (a `SafeLogic` that polls partitions via `FuturesUnordered`). The
   redesign's first-class `Message::SourcePartitions` control variant was **not** added;
   discovery still rides the data plane as `DataMessage(part, NoData, Timestamp::MIN)`,
   matching the pre-existing runtime vocabulary (this is noted as a possible follow-up, not
   shipped).
4. **Completion: kept the global protocol (deviation from the redesign's step 4)** — the
   redesign proposed reader exhaustion → `distribute` frontier-merge → `Epoch(MAX)`, deleting
   `PartitionFinished`, `COMM_CHANNEL_ID`, and the hardcoded worker 0. **Not adopted.** A
   reader-local `Epoch(MAX)` races with partitions still in flight through the distribute
   step's async router on multi-worker jobs (a reader can see its own discovery-done signal
   before its in-flight partitions arrive, emitting a premature MAX and losing data). The
   shipped design keeps the coordinator-side global part set: readers send
   `PartitionFinished(key)` to worker 0 via `CommUtility`, the coordinator removes each key
   and emits `Epoch(Timestamp::MAX)` only when worker 0's full part set is empty
   (`listed_parts && parts.is_empty()`).
5. **Per-source comm channel ids (implements the previously rejected fix)** —
   `CommUtility::new(ctx, channel_id)` now takes an explicit `channel_id` instead of the
   shared `COMM_CHANNEL_ID = u64::MAX`, and the source operators pass
   `seahash::hash(source_name.as_bytes())`. This implements the previously rejected
   per-source-comm-channels proposal (the `rejected/bug-fix/2026-08-22-per-source-comm-channels.md`
   note was deleted as stale — see the Alternatives section); the rejection's premise — that
   the frontier-merge completion would remove the protocol entirely, leaving no shared
   channel to fix — was wrong because the protocol was kept (deviation 4). Multiple
   concurrent sources now deliver `PartitionFinished` on distinct channels.
6. **Naming** — `Part` → `PartitionKey`, `SourcePartition` (assoc type) → `Partition`,
   `PartitionState` (on the partition trait) → `State`; `PartLister`/`PartitionsFinished`
   gone. The `sources::Source` **struct** collides with the `operators::Source` **trait**:
   consumer code imports the struct explicitly and the trait anonymously —
   `use malstrom::operators::Source as _;` — everywhere both are needed.
7. **Dead files deleted** — `keyed_old/`, `sources/stateful_old.rs`,
   `coordinator/state_old.rs`, `channels/operator_io copy.rs`, and
   `testing/iterator_source.rs` (an undeclared dead module referencing the removed
   `SingleIteratorSource`/`IntoSource`; its `emits_values` test lives on in
   `fn_source.rs`).

## Alternatives considered

- **Fix in place (patch the adapter, keep two hierarchies)** — the adapter *is* the smell;
  two parallel conventions for the same concept would diverge again. Rejected.
- **`Default`-seeded state** (`PartitionState: Default` with a default `snapshot() { () }`)
  — a default `snapshot() { () }` does not type-check once a source overrides
  `PartitionState = MyState`; the explicit `Option<PartitionState>` in `open` was chosen.
- **Frontier-merge completion (the redesign's step 4)** — the multi-worker ordering hazard
  described in Decision 4 made a reader-local MAX unsafe; the global worker-0 coordinator
  was kept. The redesign's frontier-merge path remains available to revisit if the
  distribute step ever becomes a strict pipeline stage.
- **A first-class `Message::SourcePartitions` control variant** — deferred; discovery still
  uses `DataMessage(part, NoData, MIN)`, which the runtime already understands. The control
  variant would require touching every `Message` match site for marginal clarity.
- **Per-source comm channel id** — previously rejected, now implemented (Decision 5): the
  rejection assumed the shared channel would disappear with the protocol; the protocol
  stayed, so the collision fix was needed after all. The `rejected/` note was deleted as
  stale; its rationale lives on in this Decision.
- **Keeping the raw-`Logic` part-lister** — `SafeLogic` exists precisely so operators don't
  get the internal messaging invariants wrong; the reader op is `SafeLogic`. (The discovery
  coordinator is still raw `Logic` — a narrow, deliberate exception, since it is a pure
  control operator with no data-plane state machine.)
- **`#[derive(StatelessSource)]` proc-macro** — the remaining stateless ceremony is three
  no-op `snapshot`/`collect` lines; a macro (and its compile-time cost) was not worth it.

## Consequences

- **One convention** — sources are written once against `SourceImpl`/`SourcePartition`;
  statelessness is a state-type choice, not a parallel hierarchy. New authors see exactly
  one trait pair to learn.
- **Framework-owned lifecycle** — discovery (`discover`), open-on-assignment (`open`),
  resume-from-snapshot, rescale (`Acquire`/`Collect`/`Interrogate` on the reader op) and
  global completion are all framework code; the part-finished protocol is per-source
  collision-free.
- **Async discovery** — `discover`/`open` are `async`, closing the old sync-stateless /
  async-stateful capability gap (a stateless source can now do async discovery).
- **Behavior preserved** — the refactor is behavior-preserving for the existing graph:
  every example was smoke-run against both the old and new sources and behaves identically,
  including the pre-existing quirk that keyed chains without a terminal sink exit silently
  (`stateful_programs`, `keyed_streams` — reproduced on HEAD before the refactor; not a
  regression from this change).
- **API churn** — every example, operator doctest, and test module was migrated
  (`StatelessSource::new(SingleIteratorSource::new(…))` → `Source::from_iterator(…)`,
  `StatefulSource::new(…)` → `Source::from_impl(…)`, trait renames, `Source as _`
  imports); the root `README.md`, `docs/overviews/01-project.md` and
  `website/guide/CustomSources.md` now show the new API.
- **Verification** — `cargo check -p malstrom`, `--examples`, `--workspace`, and
  `cargo test -p malstrom` (50 unit + 10 doc tests) are green; representative examples
  (`look_ma_im_streaming`, `basic_stdout`, `ttl_map`, `multithreading`, `basic_operators`,
  `rescaling`, `split_streams`, `union_streams`, `cloned_streams`,
  `custom_stateless_operator`) smoke-run correctly. The doc-test count dropped 11 → 10
  because the dead `testing/iterator_source.rs` doctest was deleted with the file.
- **Cost** — the `Source as _` idiom is a small ergonomic tax on consumer code; the
  discovery coordinator remains raw `Logic`; the redesign's step-4 frontier-merge completion
  and the `SourcePartitions` control message remain unimplemented (both documented above as
  deliberate deviations).
