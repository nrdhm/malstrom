# Reviews & design notes

Code reviews with a concrete remediation sketch, one topic per file.

| # | Doc | Scope |
|---|-----|-------|
| 01 | [new-scheduler-tests-review.md](new-scheduler-tests-review.md) | `new-scheduler-tests` vs the implemented test-plan note — counts/bug-fix claims verified, dead code to remove, and algorithm simplifications |
| 02 | [union-refactor-review.md](union-refactor-review.md) | Staged union refactor toward fewer core internals — Termux lint caveats (exclude kafka/k8s-proto), debug leftovers to remove, and how to generalize `forward_to` before refactoring `split()` |
| 03 | [2026-09-08-tracing-timings.md](2026-09-08-tracing-timings.md) | How to debug function timings with `tracing`: span-close timings, async `.instrument()`, `FmtSpan::CLOSE`, and `tracing-timing` for percentiles |
| 04 | [2026-09-08-tracing-loop-spans.md](2026-09-08-tracing-loop-spans.md) | Ways to wrap a loop into a `tracing` span: whole-loop guard, per-iteration spans, nested spans, async `.instrument()` body, and `#[instrument]` |

Conventions mirror [`../overviews/README.md`](../overviews/README.md): link to code, don't
duplicate it; each file carries a `> **Last refreshed:**` header.
