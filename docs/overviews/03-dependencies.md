# Malstrom Core Dependency Map

> **Last refreshed:** 2026-08-23 (new-scheduler @ a4c8fce)
> **Scope:** non-dev dependencies of `malstrom-core/Cargo.toml`
> **Branch/commit:** new-scheduler @ a4c8fce (2026-08-23)
> **Method:** usage grep over `malstrom-core/src`; dead reference files excluded (see "Findings")
> **Keep fresh:** see "Keeping this file fresh" at the bottom

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

**tokio-stream** (optional, `slatedb`) — `StreamExt` over SlateDB entry stream
- `snapshot/slatedb.rs`

**console-subscriber** — `console_subscriber::init()`
- **only** `examples/multithreading.rs` (not in `src/`)

## Persistence (feature `slatedb`)

**slatedb** — `slatedb::db::Db` snapshot store; re-exported as `SlateDbBackend`/`SlateDbClient`
- `snapshot/slatedb.rs`, `snapshot/mod.rs` (re-export)

**object_store** — `ObjectStore`, `PutPayload`, `path::Path`, `Error::NotFound`, `ObjectMeta` (+ `memory::InMemory` in tests)
- `snapshot/slatedb.rs`

## Operators & helpers

**itertools** — `itertools::repeat_n` (production), `Itertools` trait methods (mostly tests/examples)
- production: `channels/operator_io.rs` (`Output::send` clones a message per recipient); tests/examples: `operators/{map,filter_map,flatten,inspect,stateful_map,ttl_map,time/assign_timestamps}`, `sources/fn_source`, `testing/`; imports in `coordinator/coordinator.rs`, `stream/{build_context,operator_context}`, `operators/stateful_op.rs` are currently unused (WIP)

**seahash** — `seahash::hash` (routing hashes), `seahash::SeaHasher::new` (partition assignment)
- `keyed/distributed/remote_receiver`, `stream/operator`

**rand** — `rand::random::<u32>()` (timestamp jitter)
- `operators/time/util.rs`

**expiremap** (serde) — `ExpireMap` as TTL-map state
- `operators/ttl_map.rs`

**malstrom-macros** (path dep) — `TTLState` derive (fields wrapped as `Option<(T, ts)>` + expire/is_empty)
- `operators/ttl_map.rs` (re-export)

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

- **`eyre` is dead** — declared in Cargo.toml, zero uses in the whole crate (src + examples). Remove candidate.
- **Dead reference files removed** (2026-08-23, `collapse-source-traits`): the undeclared,
  uncompiled reference copies `keyed_old/`, `sources/stateful_old.rs`,
  `coordinator/state_old.rs`, `channels/operator_io copy.rs` (and `testing/iterator_source.rs`)
  were deleted.
- `futures::SinkExt` imported in `snapshot/mod.rs` with no call site yet (WIP)
- Optional deps (`slatedb`, `object_store`, `tokio-stream`) are only active under the `slatedb` feature

## Keeping this file fresh

1. Diff the dep list: `cargo tree -p malstrom -e features`
2. For each dep, refresh call sites: `grep -rn "<crate>::" malstrom-core/src`
3. Only count files reachable from `lib.rs` — confirm a `mod <name>` declaration exists (skip `*_old`/copy files)
