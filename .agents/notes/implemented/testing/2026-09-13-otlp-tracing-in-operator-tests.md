# Agent Note: OTLP tracing in operator tests

Status: implemented

## Problem

`cargo test -p malstrom-combinators union_unites` hung for minutes (only an external
timeout killed it) after the test module gained a `tracing-opentelemetry` layer pointed
at a local Tempo instance. Tempo was reachable and answered fast; the logs showed 404s,
then a `Cannot drop a runtime in a context where blocking is not allowed` panic, then a
flood of `SimpleProcessor.OnEnd.Error` — not a slow-backend stall.

The stack was `SimpleSpanProcessor` plus the OTLP HTTP exporter's default
`reqwest-blocking-client`. The SDK documents that this combination requires spans to be
emitted from a **non-Tokio** thread. Malstrom's single-thread test runtime emits and
closes `tracing` spans inside `rt.execute()`, so the first in-runtime export dropped the
blocking client's internal Tokio runtime in a blocking-not-allowed context and panicked.
The panic poisoned the processor's exporter mutex, so every later span close failed and
the test never completed cleanly.

A second issue was endpoint semantics: programmatic `.with_endpoint(...)` is used
verbatim by the OTLP crate. Passing a base URL like `http://host:4318` made the exporter
POST to `http://host:4318/` (404) instead of `http://host:4318/v1/traces`.

## Decision

- Use `SdkTracerProvider::builder().with_batch_exporter(exporter)` in the
  malstrom-combinators test tracing setup. `BatchSpanProcessor` exports on its own
  dedicated thread, which is compatible with the default blocking HTTP client even when
  spans are emitted from Tokio runtime threads.
- Read the endpoint solely from `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` and pass it verbatim
  as a full OTLP/HTTP URL (e.g. `http://host:4318/v1/traces`). No base-endpoint fallbacks
  and no signal-path appending.
- Keep the provider in a process-wide `OnceLock<SdkTracerProvider>` and call
  `force_flush()` after `rt.execute()` returns, on the test thread outside the Tokio
  runtime, so queued spans are exported before the test process exits.
- When `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` is unset, install only the local fmt layer;
  no OTel layer and no provider are created.

## Alternatives considered

- **SimpleSpanProcessor:** its synchronous on-close export is attractive for tests, but
  it is incompatible with the default blocking HTTP client when spans close on a Tokio
  thread: it panics and then poisons its mutex. Rejected for this test harness.
- **Async `reqwest-client` feature:** that would make `SimpleSpanProcessor` usable inside
  `rt.execute()`, but spans also close on the synchronous test thread before and after
  the runtime, where an async client is not usable. Batch export avoids picking a
  per-thread client at all.
- **Lowering `OTEL_EXPORTER_OTLP_TRACES_TIMEOUT` / probing Tempo reachability:** treats a
  slow backend as the cause; the evidence showed Tempo answered fast and the failure was
  a panic, so a timeout would not have fixed it.
- **Base URL (`OTEL_EXPORTER_OTLP_ENDPOINT`) plus a `/v1/traces` appending helper:**
  possible, but the standard signal-specific variable already encodes the full URL,
  removes the path-assembly logic, and matches OTel's own programmatic-endpoint
  behavior. Chosen.

## Consequences

- `cargo test -p malstrom-combinators union_unites` completes in ~5s (the remaining
  latency is the coordinator's 5s completion poll, not tracing) and
  `HttpClient.ExportSucceeded` confirms traces reach Tempo.
- Test traces are batched, so they are not visible in Tempo immediately on span close;
  the explicit `force_flush()` after execution bounds the delivery lag.
- The test process keeps a batch worker thread and a provider until process exit;
  provider drop shuts the worker down and flushes remaining spans.
- Tracing is opt-in via `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`; without it, tests behave as
  before (fmt-only subscriber).
- Only the `union_unites` test module currently installs this layer; other malstrom
  crates' test harnesses are untouched.
- Exported traces can be inspected without Grafana via
  `scripts/tempo-search.sh` (search by service with Tempo's HTTP API on port 3200,
  summarize a trace by ID with per-span busy/idle and span events).