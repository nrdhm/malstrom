> **Last refreshed:** 2026-09-08

# Wrapping a loop into a `tracing` span

Patterns from simplest to most granular. In all examples, `FmtSpan::CLOSE` in the
subscriber makes the span timings visible; without it spans only appear when you log inside
them.

## 1. Whole loop in one span (sync)

Use an `enter()` guard or `in_scope()` so the span lives for the entire loop:

```rust
let span = tracing::debug_span!("process_all");
let _guard = span.enter();

for item in items {
    process(item);
}
// span closes when `_guard` drops (end of scope)
```

Or the closure form, which makes the lifetime explicit:

```rust
tracing::debug_span!("process_all").in_scope(|| {
    for item in items {
        process(item);
    }
});
```

This gives **one** span covering all iterations. With `FmtSpan::CLOSE` you get a single
`time.busy=...` for the whole loop.

## 2. One span per iteration

Create the span inside the loop body; it closes at the end of each iteration:

```rust
for (i, item) in items.iter().enumerate() {
    let span = tracing::debug_span!("process_item", i, ?item);
    let _guard = span.enter();

    let result = process(item);
    tracing::debug!(?result, "item processed");
}
```

Useful for per-iteration close timings and for spotting which items are slow.

## 3. Nested: outer loop + inner per-iteration spans

```rust
let outer = tracing::debug_span!("process_all");
let _outer_guard = outer.enter();

for (i, item) in items.iter().enumerate() {
    let inner = tracing::debug_span!("process_item", i);
    let _inner_guard = inner.enter();
    process(item);
}
```

With `FmtSpan::CLOSE` you get both:

```text
process_all: close time.busy=12.3ms time.idle=0s
process_item{i=0}: close time.busy=1.1ms
process_item{i=1}: close time.busy=2.4ms
```

## 4. Async loop: instrument the body future

For async loops (including `tokio::select!`), **don't** hold `enter()` across `.await` if
you want busy/idle splits — the span would stay "busy" while waiting. Instead
`.instrument()` the body future per iteration:

```rust
use tracing::Instrument;

loop {
    let span = tracing::debug_span!("operator_iter", op = %name);

    async {
        tokio::select! {
            msg = input.recv() => handle(msg),
            _ = output.closed() => break,
        }
    }
    .instrument(span)
    .await;
}
```

Now `time.busy` = actual code running, `time.idle` = waiting on the channel/select.

For a whole-loop async span, instrument the loop future once:

```rust
async {
    loop { ... }
}
.instrument(tracing::debug_span!("worker_main"))
.await;
```

## 5. `#[instrument]` on the function containing the loop

If the loop is inside a function, the macro wraps the entire function call — all iterations
share one span:

```rust
#[instrument(skip_all)]
async fn run(&mut self, input: &mut Input<M>) {
    loop { ... }
}
```

Requires `tracing` with the `attributes` feature:

```toml
tracing = { version = "0.1", features = ["log", "attributes"] }
```

## Which to use in this repo

For the operator loop in `malstrom-core/src/stream/operator.rs`:

- **While debugging one hot op**: wrap the `tokio::select!` body with `.instrument(span)`
  per iteration (pattern 4) — you'll see busy vs. idle per loop turn.
- **When profiling overall throughput**: wrap the whole `loop` in one span (pattern 1/5) —
  less noise.
- **Per-message diagnostics**: put the span inside the `logic.apply(...)` branch
  (pattern 2) and log the message type.

## Caveats

- `debug_span!` is cheap when disabled, but not free in a tight loop; use `trace_span!` or
  remove after debugging.
- `enter()` across `.await` is usually wrong for timing — prefer `.instrument()` so idle
  time is attributed correctly.
- `FmtSpan::CLOSE` in `init_logs()` is what makes the timings visible.