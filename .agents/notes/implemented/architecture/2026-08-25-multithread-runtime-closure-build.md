# Agent Note: Make `MultiThreadRuntime::build` take a closure instead of a function pointer

Status: implemented

## Problem

`MultiThreadRuntime`'s `build` field was a **function pointer**
(`build: fn(&mut dyn StreamProvider) -> ()`, `malstrom-core/src/runtime/threaded/multi.rs`),
so callers could not capture state — the builder accepted only free functions. This forced the
kernel-level test `multi_thread_runtime_runs_dataflow_on_all_workers` (in the same file) to
smuggle its reporting channel through a `static REPORTED: OnceLock<…>` and
`REPORTED.get_or_init(flume::unbounded)` at every use site, because the per-worker
`RecordWorker` operator needed the sender but the dataflow closure could not capture it.

It was also inconsistent with `SingleThreadRuntime<P, F>`
(`malstrom-core/src/runtime/threaded/single.rs`), which already takes a generic
`build: F` with `F: FnOnce(&mut dyn StreamProvider)` — the single-thread runtime lets
callers capture freely and the multi-thread one did not, for no structural reason.

## Decision

Make `MultiThreadRuntime` generic over the build closure, mirroring `SingleThreadRuntime`:

```rust
pub struct MultiThreadRuntime<P, F> {
    #[builder(finish_fn)]
    build: F,
    persistence: P,
    snapshots: Option<Duration>,
    parrallelism: u64,
    // api_handles, rescale_req unchanged
}

impl<P, F> MultiThreadRuntime<P, F>
where
    P: PersistenceBackend + Clone + Send + Sync,
    F: Fn(&mut dyn StreamProvider) + Clone + Send + 'static,
{
```

- **`Fn`, not `FnOnce`** — `build` is invoked once per worker (the initial spawn loop *and* the
  rescale scale-up loop in `execute()`), so it must be callable multiple times.
- **`Clone`** — `execute()` passes a fresh clone to each worker thread (`self.build.clone()`),
  as the previous `fn` pointer was implicitly `Copy`.
- **`Send + 'static`** — the clone is moved into `std::thread::spawn`.

`spawn_worker`'s parameter changed from `build_fn: fn(&mut dyn StreamProvider)` to
`build_fn: F`, and the two call sites (`execute()`'s initial loop and the rescale loop) pass
`self.build.clone()`.

The test dropped the `static` entirely and captures its flume channel in the build closure:

```rust
let (tx, rx) = flume::unbounded();
MultiThreadRuntime::builder()
    .parrallelism(4)
    .persistence(NoPersistence)
    .build(move |provider: &mut dyn StreamProvider| {
        let tx = tx.clone();
        provider
            .new_stream()
            .then(Operator::built_by("numbers", |_| async { Numbers(0, 5) }))
            .then(Operator::built_by(
                "record-worker",
                move |_| async move { RecordWorker(tx) },
            ));
    })
    .execute()
    .unwrap();
```

Free functions still coerce to `Fn`, so the multi-thread examples (`multithreading`,
`rescaling`, `keyed_streams`, `look_ma_im_streaming`) compile unchanged.

## Alternatives considered

- **Status quo (keep the `fn` pointer, use a `static` in the test)** — zero production churn;
  the test keeps the `OnceLock` channel and stays polluted. Rejected: it papers over the
  inconsistency and blocks every future capture-requiring test/example.
- **`std::sync::LazyLock` stopgap in the test** — removes the `get_or_init` noise but keeps the
  global; strictly test-scoped. Rejected; the runtime API could change instead.
- **`Arc<dyn Fn(&mut dyn StreamProvider) + Send + Sync>` instead of a generic `F`** — avoids
  the extra type parameter but adds dynamic dispatch and an allocation per runtime, and still
  makes `build` a trait-object field. Rejected; the generic matches `SingleThreadRuntime`.
- **Keep `FnOnce` like `SingleThreadRuntime`** — impossible: the multi-thread runtime invokes
  `build` once per worker (and again on rescale), so `FnOnce` cannot be moved multiple times.

## Consequences

- **`MultiThreadRuntime<P>` → `MultiThreadRuntime<P, F>`** — a breaking signature change for
  any consumer who names the type (not just calls the builder). Pre-1.0 this is acceptable.
- **The kernel test needs no globals** — `multi_thread_runtime_runs_dataflow_on_all_workers`
  captures its flume channel in the build closure; the `static`/`OnceLock` machinery is gone.
  This closes the thread of work that started with the fn-pointer constraint (shared `Vec` +
  `Mutex` → flume channel via a static → captured closure).
- **Capture-requiring callers become possible** — the multi-thread runtime is now as
  ergonomic as the single-thread one for closures.
- **Rescale still works** — the scale-up loop re-invokes the same closure with a fresh clone.
- **Verification** — `cargo check --workspace` clean (0 warnings); tests green: `malstrom`
  19 unit (incl. the rewritten test asserting 4 workers × 5 records grouped per worker),
  `malstrom-operators` 31 unit + 9 doc, `malstrom-testkit` 1, `malstrom-snapshot-slatedb` 5;
  the multi-thread examples build and `rescaling` (which exercises the rescale re-invocation)
  and `multithreading` smoke-run correctly.
