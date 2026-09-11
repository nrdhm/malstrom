> **Last refreshed:** 2026-09-08

# Debugging function timings with `tracing`

Practical patterns for this repo. Start with span-close timings; go to instrumented async
functions or histograms only when you need more detail.

## 1. Spans around functions you care about

The simplest pattern that gives **wall time per call** without changing dependencies:

```rust
fn handle(msg: &Message) {
    let start = std::time::Instant::now();
    let span = tracing::debug_span!("handle", ?msg);
    let _guard = span.enter();

    // ... work ...

    tracing::debug!(elapsed_ms = start.elapsed().as_secs_f64() * 1000.0, "handle done");
}
```

The better `tracing` way is to let the subscriber print span open/close with built-in
timings (next section).

## 2. Make the subscriber print span-close timings

`malstrom-core/tests/common/mod.rs` currently has:

```rust
let _ = fmt().with_env_filter(filter).with_test_writer().try_init();
```

Change it to emit span close events:

```rust
use tracing_subscriber::fmt::format::FmtSpan;

let _ = fmt()
    .with_env_filter(filter)
    .with_span_events(FmtSpan::CLOSE)
    .with_test_writer()
    .try_init();
```

Any span then prints something like:

```text
DEBUG handle{msg=Data(42)}: close time.busy=1.32ms time.idle=203µs
```

- `time.busy` — time the span was **entered** (actual execution).
- `time.idle` — time the span existed but was **not entered** (awaiting, queued).
- Works with `RUST_LOG=your_crate=debug cargo test ... -- --nocapture`.

## 3. For async functions, use `.instrument()` to get busy/idle split

Manual `_guard = span.enter()` around `.await` keeps the span entered across the await, so
you get total wall time but **no busy/idle split**. To see executing vs. waiting, instrument
the future instead:

```rust
use tracing::Instrument;

let fut = async {
    // ... async work ...
}.instrument(tracing::debug_span!("handle_message", ?msg));

tokio::spawn(fut);
```

For an `async fn`, `#[instrument]` is the ergonomic version — it requires the `attributes`
feature:

```toml
tracing = { version = "0.1", features = ["log", "attributes"] }
```

```rust
#[instrument(skip_all)]
async fn process(&mut self, input: &mut Input<M>) {
    // busy = code actually running, idle = time spent awaiting
}
```

## 4. Suggested debugging workflow in this repo

1. Add `.with_span_events(FmtSpan::CLOSE)` to `init_logs()`.
2. Run with a focused filter:

   ```bash
   RUST_LOG=malstrom_core=debug,tokio=warn \
     cargo test -p malstrom-core --test completion -- --nocapture
   ```

3. Add temporary spans to the hot path being investigated:

   ```rust
   let span = tracing::debug_span!("operator_apply", op = %self.name);
   let _guard = span.enter();
   ```

4. Look at `time.busy` vs `time.idle`:
   - high `idle` → waiting on channels/barriers
   - high `busy` → operator logic itself is the cost

## 5. If you need percentiles over many calls

`tracing-timing` records histograms per span (mean, p50/p99, …) and is better for "which
operator is slow across 100k messages" than per-call output. Start with `FmtSpan::CLOSE`
first; it is usually enough to find the bottleneck.

## Caveats

- Spans in hot loops have non-zero overhead, even when disabled. Add them temporarily and
  remove before finishing.
- `FmtSpan::CLOSE` prints only when the span **closes**. A long-lived span (e.g. one created
  in `execute()` for the whole worker) prints once at shutdown; for per-iteration timing,
  create a fresh span per iteration or log `elapsed_ms` manually.
- `#[instrument]` requires the `attributes` feature; manual `debug_span!` + `enter()`
  needs no new features.