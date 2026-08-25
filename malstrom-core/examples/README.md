# Kernel examples (framework-level)

These examples live in the `malstrom` (kernel) package even though they import
`malstrom_operators`. The rule is **not** "which crates does the example import" — a runnable
dataflow needs operators no matter what — but **what the example is about**: an example lives
in the crate whose **feature is the subject of the lesson**.

| Example | The subject (what it teaches) | The operator bits are incidental |
|---|---|---|
| `basic_noop` | A minimal job: `SingleThreadRuntime::builder()` → `.build(closure)` → `.execute()` | none — the only kernel-only example |
| `multithreading` | **The runtime's worker model**: `MultiThreadRuntime`, `parrallelism(n)`, and the `@ Worker {ctx.worker_id}` placement of operators across workers | an `inspect` that prints the worker id |
| `rescaling` | **The rescale protocol**: `api_handle.rescale(n)` moves state between workers while the job runs | a `map`/`sink` pipeline that keeps producing during the rescale |
| `stateful_programs` | **Snapshot persistence**: `.snapshots(duration)` makes operator state recoverable, driven by the kernel's barrier/persistence machinery | a `stateful_map` that accumulates a sum |
| `stateful_program_multiple_keys` | **Per-key state + snapshots**: multiple keys each carry state that survives restart | a keyed `stateful_map` |

Contrast with `malstrom-operators/examples`, where the subject is an **operator / sink /
source capability**: `basic_operators` (operator tour), `custom_stateful_operator` /
`custom_stateless_operator` (writing operators), `file_source_*` / `file_sink_*` (writing
sources/sinks), `ttl_map` (TTL state), `event_time*` (watermark/event-time semantics),
`split_streams` / `union_streams` / `cloned_streams` (stream algebra), `keyed_streams`
(keyed routing), `look_ma_im_streaming` (hello world). And
`malstrom-snapshot-slatedb/examples` teaches the SlateDB persistence backend itself.

### The borderline calls (how the line is drawn)

- **`keyed_streams` uses `MultiThreadRuntime` but lives in `malstrom-operators`** — its
  lesson is the *keyed operators* (`KeyLocal`/`KeyDistribute`/`rendezvous_select`), not the
  runtime; the multi-thread runtime is just where keyed distribution shows up.
- **`stateful_programs` uses `stateful_map` (an operator) but lives in the kernel** — its
  lesson is *snapshot recovery*, which is kernel machinery (persistence + barriers); the
  operator is the vehicle.
- The reverse test: would the example still make sense if its operator were swapped for
  another? `rescaling` with a `map` instead of a `sum` teaches the same thing; `ttl_map`
  with a different runtime does not — that's the signal.

This placement is a **curation choice, not a technical constraint**: all five could
technically compile inside `malstrom-operators` (its examples may use `malstrom`). The
kernel keeps them so its own example set tells the engine's story, and operator examples
tell the library's story. If the line ever feels arbitrary, the clean end-state is a single
dedicated `malstrom-examples` crate holding all of them.

## Run them

```bash
cargo run -p malstrom --example basic_noop
cargo run -p malstrom --example multithreading
cargo run -p malstrom --example rescaling
cargo run -p malstrom --example stateful_programs
cargo run -p malstrom --example stateful_program_multiple_keys
```
