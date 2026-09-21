# Malstrom examples

Runnable examples for Malstrom, grouped by **what each one teaches**. The rule is not
"which crates the example imports" — a runnable dataflow needs `malstrom-operators` no
matter what — but **the subject of the lesson**: framework-level examples exercise the
engine (scheduling, multi-threading, rescaling, stateful programs); operator-level examples
demonstrate individual operators, sinks and sources.

## Framework-level (the engine)

These exercise `malstrom` kernel machinery: the runtime's worker model, the coordinator's
rescale protocol, and snapshot persistence.

| Example | What it exercises |
|---|---|
| `basic_noop` | The minimal job shape: `SingleThreadRuntime::builder()` → `.build(closure)` → `.execute()` |
| `multithreading` | The runtime's worker model: `MultiThreadRuntime`, `parrallelism(n)`, and operator placement across workers (`@ Worker {ctx.worker_id}`) |
| `rescaling` | The rescale protocol: `api_handle.rescale(n)` moves state between workers while the job runs |
| `stateful_programs` | Snapshot persistence: `.snapshots(duration)` makes operator state recoverable |
| `stateful_program_multiple_keys` | Per-key state + snapshots: multiple keys each carry state that survives restart |

## Operator-level (the library)

These demonstrate individual capabilities of `malstrom-operators` (and
`malstrom-distributed` for keyed routing).

| Example | What it demonstrates |
|---|---|
| `look_ma_im_streaming` | Hello world: source → map → stdout sink |
| `basic_operators` | The operator tour (map/filter/inspect/…) |
| `basic_stdout` | Basic operators with the stdout sink |
| `custom_stateless_operator` | Writing a custom stateless operator (`StatelessLogic`) |
| `custom_stateful_operator` | Writing a custom stateful operator (`StatefulLogic`) |
| `file_source_stateless` / `file_source_stateful` | Writing custom sources (`SourceImpl`) |
| `file_sink_stateless` / `file_sink_stateful` | Writing custom sinks (`StatelessSinkImpl`/`StatefulSinkImpl`) |
| `event_time` / `event_time_out_of_order` | Event-time semantics: `assign_timestamps`, `generate_epochs`, late data |
| `keyed_streams` | Keyed routing: `KeyLocal`/`KeyDistribute`/`rendezvous_select` across workers |
| `split_streams` / `union_streams` / `cloned_streams` | Stream algebra: split, union, cloned |
| `ttl_map` | TTL state (`TtlMap` with an `ExpireMap`) |

### Borderline calls (how the line is drawn)

- **`keyed_streams` uses `MultiThreadRuntime` but is operator-level** — its lesson is the
  keyed *operators*, not the runtime; the multi-thread runtime is just where keyed
  distribution shows up.
- **`stateful_programs` uses `stateful_map` (an operator) but is framework-level** — its
  lesson is *snapshot recovery*, which is kernel machinery (persistence + barriers); the
  operator is the vehicle.
- The reverse test: would the example still teach the same thing if its operator were
  swapped for another? `rescaling` with a `map` instead of a `sum` teaches the same thing;
  `ttl_map` with a different runtime does not.

The SlateDB persistence demos live in `malstrom-snapshot-slatedb/examples/` (the connector
crate owns its backend's examples); `malstrom-kafka` ships Kafka connector examples.

## Run them

```bash
# framework-level
cargo run -p malstrom-examples --example basic_noop
cargo run -p malstrom-examples --example multithreading
cargo run -p malstrom-examples --example rescaling
cargo run -p malstrom-examples --example stateful_programs
cargo run -p malstrom-examples --example stateful_program_multiple_keys

# operator-level
cargo run -p malstrom-examples --example look_ma_im_streaming
cargo run -p malstrom-examples --example basic_operators
# …and so on for the rest
```
