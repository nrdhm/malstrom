# Malstrom Core Dependency Map

> **Last refreshed:** 2026-09-03

> **Scope:** non-dev dependencies of `malstrom-core/Cargo.toml` — **the kernel**.
> **Method:** usage grep over `malstrom-core/src`; dead reference files excluded (see "Findings")
> **Keep fresh:** see "Keeping this file fresh" at the bottom

> **Post-split (2026-08-24):** the crate split moved dependency-heavy code out of
> `malstrom-core`. `rand`, `expiremap`, `seahash`, `malstrom-macros` now live in
> `malstrom-operators`; `seahash` additionally in `malstrom-distributed`;
> `slatedb`, `object_store`, `tokio-stream` in `malstrom-snapshot-slatedb`; `eyre` was removed
> as dead; `console-subscriber` is a dev-dependency of the kernel (multithreading example).
> The usage lists below were not re-audited after the split — treat them as approximate.

## Data & serialization

**serde** — `#[derive(Serialize, Deserialize)]` on wire/state types; `de::DeserializeOwned` state bound; `ser::SerializeStruct` custom impl
- `types/{data,key,time,message,distributable}`, `snapshot/mod.rs`, `keyed/{key_distribute,broadcast,distributed/*}`, `coordinator/{messages,cluster}`, `sources/stateful`, `sinks/stateful`, `stream/{build_context,operator_context}`, `operators/{stateful_map,ttl_map,stateful_op,time/generate_epochs}`, `runtime/communication/{mod,operator_operator}`

**rmp-serde** — `to_vec`/`from_slice`, `encode`/`decode` (MessagePack)
- `snapshot/mod.rs` (`serialize_state`/`deserialize_state`), `snapshot/slatedb.rs` (commit list), `types/distributable.rs`

**indexmap** (serde) — `IndexMap` (ordered map), `IndexSet` (ordered key sets)
- `coordinator/{cluster,messages,watchmap}`, `worker/{worker,builder,coordination_task,sys_message}`, `keyed/distributed/*` + `keyed/broadcast`, `stream/{build_context,operator_context}`, `channels/{alignment,recv_trait}`, `operators/{stateful_op,com_utility,ttl_map}`, `sinks/stateful`, `sources/stateful`

## Async & runtime

**tokio** (`rt`, `rt-multi-thread`, `macros`, `sync`, `time`) — `runtime::{LocalRuntime, Builder::new_multi_thread}`, `sync::{mpsc,oneshot,watch,broadcast,Notify,Mutex}`, `time::{sleep,timeout}`, `select!`, `task::JoinHandle`, `#[tokio::test]`
- `worker/*`, `stream/{operator,build_context}`, `channels/{operator_io,spsc,signal}`, `keyed/distributed/*`, `coordinator/{coordinator,api,snapshot,watchmap}`, `sources/stateful`, `snapshot/{mod,slatedb}`, `runtime/threaded/multi`

**futures** — `StreamExt`, `FutureExt`, `TryFutureExt`, `SinkExt`, `stream::FuturesUnordered` (concurrent polling), `future::join_all`
- `channels/{operator_io,alignment,recv_trait,spsc}`, `snapshot/mod.rs`, `worker/worker`, `coordinator/{coordinator,cluster}`, `sources/stateful`, `operators/com_utility`, `runtime/communication/{mod,operator_operator}`, `keyed/distributed/remote_receiver`

**async-trait** — `#[async_trait]` on async comm/backend trait methods
- `runtime/communication/{mod,reqres,stream,operator_operator}`, `runtime/threaded/{single,communication/*}`, `testing/operator_tester`

**flume** — `bounded`, `Sender`, `Receiver` (cross-thread channels; coordinator API request queue)
- `coordinator/{api,snapshot,coordinator}`, `runtime/threaded/communication/{mod,stream,reqres,inter_thread}`

**pin-project** — `#[pin_project]` on pinned self-referential futures
- `channels/spsc`, `runtime/communication/operator_operator`

**tokio-stream** — `StreamExt` over SlateDB entry stream
- **moved** with `malstrom-snapshot-slatedb` (2026-08-24)

**console-subscriber** — `console_subscriber::init()`
- **dev-dependency** — **only** `examples/multithreading.rs`

## Persistence connectors

**slatedb** — `slatedb::db::Db` snapshot store; `SlateDbBackend`/`SlateDbClient`
- **moved** to `malstrom-snapshot-slatedb` (2026-08-24)

**object_store** — `ObjectStore`, `PutPayload`, `path::Path`, `Error::NotFound`, `ObjectMeta` (+ `memory::InMemory` in tests)
- **moved** to `malstrom-snapshot-slatedb` (2026-08-24)

## Operators & helpers

**itertools** — `itertools::repeat_n` (production), `Itertools` trait methods (mostly tests/examples)
- production: `channels/operator_io.rs` (`Output::send` clones a message per recipient); tests/examples: `operators/{map,filter_map,flatten,inspect,stateful_map,ttl_map,time/assign_timestamps}`, `sources/fn_source`, `testing/`; imports in `coordinator/coordinator.rs`, `stream/{build_context,operator_context}`, `operators/stateful_op.rs` are currently unused (WIP)

**seahash** — `seahash::hash` (routing hashes), `seahash::SeaHasher::new` (partition assignment)
- kernel: `stream/operator`; `malstrom-distributed/remote_receiver` (own dep)

**rand** — `rand::random::<u32>()` (timestamp jitter)
- **moved** with `malstrom-operators` (2026-08-24)

**expiremap** (serde) — `ExpireMap` as TTL-map state
- **moved** with `malstrom-operators` (2026-08-24)

**malstrom-macros** (path dep) — `TTLState` derive (fields wrapped as `Option<(T, ts)>` + expire/is_empty)
- **moved** with `malstrom-operators` (2026-08-24)

## Errors & logging

**thiserror** — `#[derive(Error)]` with `#[from]`/`#[source]`
- `coordinator/{api,coordinator}`, `worker/{worker,builder,coordination_task}`, `runtime/threaded/{single,multi,communication/{mod,inter_thread}}`, `runtime/communication/{mod,operator_operator}`, `channels/signal`, `operators/com_utility`, `snapshot/slatedb`, `testing/communication`

**tracing** (log) — `info!`/`debug!`/`warn!`/`error!`, `Value` bound
- `worker/{worker,builder,coordination_task}`, `coordinator/{coordinator,snapshot}`, `runtime/communication/{mod,operator_operator}`, `runtime/threaded/communication/{mod,inter_thread}`, `operators/{flatten,time/generate_epochs}`

**eyre** — declared, **zero uses** (src + examples); dead dependency, remove candidate
- (none)

## Builders

**bon** — `#[derive(Builder)]` with `#[builder(finish_fn|default = …)]`
- `runtime/threaded/{single,multi}`

---

## Findings & caveats

- **`eyre` was dead and is gone** — declared in Cargo.toml with zero uses; removed in the split (2026-08-24).
- **Dead reference files removed** (2026-08-23, `collapse-source-traits`): the undeclared,
  uncompiled reference copies `keyed_old/`, `sources/stateful_old.rs`,
  `coordinator/state_old.rs`, `channels/operator_io copy.rs` (and `testing/iterator_source.rs`)
  were deleted.
- `futures::SinkExt` imported in `snapshot/mod.rs` with no call site yet (WIP)
- The `slatedb` feature of `malstrom` was removed with the extraction (2026-08-24); the backend lives in `malstrom-snapshot-slatedb`

## Keeping this file fresh

1. Diff the dep list: `cargo tree -p malstrom -e features`
2. For each dep, refresh call sites: `grep -rn "<crate>::" malstrom-core/src`
3. Only count files reachable from `lib.rs` — confirm a `mod <name>` declaration exists (skip `*_old`/copy files)
