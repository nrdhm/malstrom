# Malstrom — Project Overview

> **Last refreshed:** 2026-08-23 (new-scheduler @ a4c8fce)

## What it is

**Malstrom** is a **distributed, stateful stream processing framework written in Rust**,
in the same problem space as [Apache Flink](https://flink.apache.org/) or
[bytewax](https://bytewax.io), but with a fundamentally different implementation.
The name comes from *Moskstraumen* — one of the strongest tidal currents in the world.

Its stated goals are **best-in-class usability, reliability, and performance**:

- **Distributed** — jobs run across many machines sharing the workload, with zero-downtime
  scaling; Kubernetes is a first-class deployment target.
- **Stateful** — jobs can hold arbitrary state, snapshotted regularly to persistent storage
  (disk, S3, etc.); jobs resume from the last snapshot after failure. The
  [ABS algorithm](https://arxiv.org/abs/1506.08603) guarantees **exactly-once** message
  processing.
- **Usable** — a straightforward dataflow API with a simple threading model: no async,
  no complex lifetimes, no `Send`/`Sync` required in user code. Data only needs to be
  serializable when crossing process boundaries.
- **Reliable** — safe Rust throughout, with zero-downtime scaling (and planned zero-downtime
  upgrades).

> **Status:** v0.1.0. All documented functionality works, but the project describes itself as
> more a **proof of concept** than production-ready.

## Repository layout

A Cargo workspace (`Cargo.toml`) with the following members:

| Path | Crate | Purpose |
|---|---|---|
| `malstrom-core/` | `malstrom` (crates.io) | The core stream processing framework |
| `malstrom-k8s/runtime/` | `malstrom-k8s` | Kubernetes runtime flavor (gRPC-based distributed execution) |
| `malstrom-k8s/operator/` | `malstrom-operator` | Kubernetes operator that manages Malstrom jobs |
| `malstrom-k8s/operator/crds/` | `crds` | The `MalstromJob` CRD definition |
| `malstrom-k8s/artifact-downloader/` | — | Sidecar that downloads job binaries into pods |
| `malstrom-k8s/artifact-manager/` | — | Service that serves job artifacts (Rocket) |
| `malstrom-kafka/` | `malstrom-kafka` | Kafka protocol sources and sinks (via `rdkafka`) |
| `malstrom-examples/` | — | runnable examples (framework-level: engine demos; operator-level: operator/sink/source demos), see its README |
| `malstrom-snapshot-slatedb/examples/*` | — | the SlateDB persistence examples |

Supporting material: `website/` (VitePress documentation site), `.github/workflows/`
(CI: container images via `ghcr.yaml`, docs site via `pages.yaml`), `malstrom-k8s/dev-scripts/`
(kind cluster + install scripts).

## Core concepts (`malstrom-core`)

**Dataflow API** — programs are built by chaining operators on streams:

```rust
provider
    .new_stream()
    .source("words", Source::from_iterator(["Look".to_string(), "ma'".to_string(), "I'm".to_string(), "streaming".to_string()]))
    .map("upper", async |x| x.to_uppercase())
    .sink("stdout", StatelessSink::new(StdOutSink));
```

Key modules (kernel; `malstrom-core/src/`; since 2026-08-24 the operators/sinks/sources/keyed
layers live in the `malstrom-operators` / `malstrom-distributed` crates):

- **`stream/`** — the stream builder and operator abstraction: `BuildableOperator`,
  `RunnableOperator`, contexts, and standard/chained operator plumbing. Custom operators can
  be written via `stateful_op`/`stateless_op` (in `malstrom-operators`).
- **`operators/`** (now `malstrom-operators`) — built-in operators: `map`, `filter`,
  `filter_map`, `inspect`, `flatten`, `split`, `cloned`, `stateful_map`, `ttl_map`, plus
  event-time operators (`assign_timestamps`, `generate_epochs`, `inspect_frontier`).
- **`sources/` & `sinks/`** (now `malstrom-operators`) — one unified
  `SourceImpl`/`SourcePartition` abstraction plus `Source::from_*` constructors for
  iterators/streams/poll closures ("stateless" sources are `SourceImpl` with
  `PartitionState = ()`), stateless/stateful sinks, stdout and in-memory vec sinks.
  `malstrom-kafka` adds Kafka endpoints.
- **`keyed/`** (now `malstrom-distributed`) — keyed streams: key distribution across workers,
  partitioners, and a message router that fans messages to the right worker/partition.
- **`runtime/`** — runtime flavors: in-process `MultiThreadRuntime` (single- and
  multi-threaded) and the distributed gRPC backend. **Workers are the unit of parallelism** —
  the runtime spawns identical workers up to the configured parallelism.
- **`coordinator/`** — the coordinator orchestrates workers (communication, watchmaps, state,
  rescaling).
- **`snapshot/`** — persistence traits and the barrier mechanism that drives exactly-once
  snapshots; the `slatedb`/cloud-store backend is the separate
  `malstrom-snapshot-slatedb` crate.
- **`channels/`** — internal operator I/O: SPSC channels, linking, merging, broadcast.
- **`types/`** — core message types (`Message`, keys, timestamps, worker IDs, partitioners).

A user's dataflow closure runs without async/lifetimes/`Send`, keeping the API ergonomic;
serialization (`rmp-serde`) is only required at process boundaries.

## Kubernetes story (`malstrom-k8s`)

- A **`MalstromJob` CRD** (group `malstrom.io`) declares a job: which binary artifact to run,
  where to fetch it (with auth via env), initial scale, and job state (`Running`/`Suspended`).
- The **operator** (`malstrom-operator`) uses `kube`/`kube-runtime` to watch `MalstromJob`
  resources and reconcile them into Kubernetes `StatefulSet`s, with finalizers and health
  checks. This is what enables zero-downtime scaling (rescaling a job while it runs).
- The **`malstrom-k8s` runtime** lets the *same* job code run distributed: `execute_auto()`
  decides from environment whether to run as coordinator or worker, communicating over
  gRPC streams (`exchange.proto` / `k8s_operator_api.proto`): worker↔worker
  (`OperatorOperator`), worker↔coordinator, and coordinator↔operator APIs.
- **Artifact management**: the `artifact-manager` serves job binaries; the
  `artifact-downloader` sidecar pulls them into pods on startup.

## Documentation & site (`website/`)

A VitePress site (malstrom.io) with a home page, "What is Malstrom", a stream-processing
primer, a Flink/bytewax comparison, and a user guide covering getting started, custom
operators/sources/sinks, keyed streams, joining/splitting, TTL maps, event-time, Kafka, and
the Kubernetes guide.

## Current development state

- `main` branch; open feature branches suggest active work: `new-scheduler`,
  `frontier-in-persistent-map`, `rich-function`, `split-operator`, `stateful-map`.
- Recent commits: repoint k8s/kafka crates to `crates.io` releases of `malstrom`, update
  README/keywords — i.e. preparing crates for public release.

## Quick start

```bash
cargo run -p malstrom-examples --example look_ma_im_streaming   # simplest example
cargo run -p malstrom-examples --example basic_operators        # operators tour
cargo run -p malstrom-examples --example stateful_programs      # state + snapshots
# SlateDB persistence examples: cargo run -p malstrom-snapshot-slatedb --example slatedb_backend
```

Docs: `website/` (dev server: `bun run docs:dev`).
