# Agent Note: Record `#[instrument]` spans at DEBUG via a project wrapper

Status: implemented

## Problem

`tracing::instrument` records its span at `INFO` unless the call site spells out
`level = "DEBUG"` (`tracing-attributes` `attr.rs`: `self.level.clone().unwrap_or(Level::Info)`).
Malstrom instruments hot internal paths — operator `apply` loops, channel `send`, worker and
coordination-task entry points — where per-call spans are diagnostic detail, not operational
events. At the default filter they either flood the output or force every call site to repeat
the same `level = "DEBUG"` argument, which is easy to forget when adding a new instrumented
function.

Raising the subscriber's filter default alone does not help: the filter selects *which*
records are shown, but the span's own level is what makes a DEBUG-only run surface it. A span
created at `INFO` stays `INFO` regardless of the filter, so a `RUST_LOG=debug` run would still
label these spans as informational.

## Decision

`malstrom-macros` exports an attribute macro `instrument_debug` that expands to
`#[::tracing::instrument(level = "DEBUG", ...)]`, forwarding every other argument unchanged.

- Call sites use `#[instrument_debug(skip_all)]` (or `skip(...)`, `fields(...)`, …) instead of
  `#[tracing::instrument(...)]` / `#[instrument(...)]`. All 29 real call sites across
  `malstrom-core`, `malstrom-distributed`, `malstrom-combinators` and `malstrom-testkit` were
  converted; the sole commented-out `#[tracing::instrument(..., level = "TRACE", ...)]` in
  `malstrom-core/src/stream/operator.rs` is left as-is.
- An explicit `level = ...` argument is **honoured**, not overwritten. The macro extracts it
  and passes it in place of the `DEBUG` default, so a call site can still opt into `TRACE` or
  any other level. It must be removed before forwarding, because emitting both the wrapper's
  `level` and the call site's would expand to a duplicate argument, which
  `tracing::instrument` rejects.
- `malstrom-core`, `malstrom-combinators` and `malstrom-macros` already depended on
  `malstrom-macros`; `malstrom-distributed` and `malstrom-testkit` gained the dependency.
- The test harness defaults its filter to `debug`: `init_logs()` and `tempo_init_tracing()` in
  `malstrom-testkit/src/test_support.rs` fall back to `EnvFilter::new("debug")` instead of
  `"info"` when `RUST_LOG` is unset. Without this, the newly-DEBUG spans would be invisible in
  tests unless the runner set `RUST_LOG=debug` by hand. `RUST_LOG` still overrides the default.

## Alternatives considered

- **Add `level = "DEBUG"` at each call site by hand.** No new abstraction and no new
  dependency edge, but it relies on every future call site remembering the argument — exactly
  the failure mode that produced the inconsistent levels this change removes.
- **Make only the subscriber filter default to `debug`.** Rejected: it does not change any
  span's level. An `INFO` span stays `INFO` and the DEBUG-only run is still misleading about
  which records are detail. This was the initial reading of the request and the reason the
  span level is the actual fix.
- **Change `tracing-attributes`' default.** It is an upstream crate; the default cannot be
  changed without vendoring or patching it, which the workspace does not do.
- **A blanket `#[instrument]`-shaped macro that also rewrites `skip`/`fields` parsing.** The
  wrapper stays deliberately thin: it owns only the level and defers all other argument
  semantics to `tracing::instrument`, so it cannot drift from upstream parsing.

## Consequences

- Every instrumented function now emits a DEBUG span by default, so a `debug` filter shows
  span open/close for the whole operator graph. `RUST_LOG` narrows this per target when the
  volume is unwelcome. To see the span timings themselves, enable span-close events on the
  subscriber (`FmtSpan::CLOSE`); without that the span is only visible when something logs
  inside it. For async functions, `.instrument(span)` attributes both busy and idle time to
  the span, which is what makes the per-call wall/busy split legible.
- Test output is noisier by default: any test that calls `init_logs()` prints DEBUG records on
  failure or under `--nocapture` without setting `RUST_LOG`. This is the intended trade for
  making the spans visible at all.
- `#[instrument_debug]` is the project convention; a bare `#[tracing::instrument]` in
  `malstrom*` crates is now the outlier and should be converted.
- These spans are diagnostic, not operational: `malstrom-core` enables
  `release_max_level_info`, so in a release build of a binary that does not otherwise raise
  the cap the DEBUG spans are compiled out entirely. The default `debug` test filter cannot
  resurrect them there.
- The macro is pinned by `malstrom-macros/tests/instrument_debug.rs`, which records created
  span levels through a capturing layer and asserts the `DEBUG` default, that `skip_all` and
  `fields` are forwarded, and that an explicit `level = "TRACE"` is honoured.

## Related

- [OTLP tracing in operator tests](../testing/2026-09-13-otlp-tracing-in-operator-tests.md)
  owns the test-harness tracing setup this change alters the default filter of.
- [Collapse kernel test support into `malstrom-testkit`](../../proposed/testing/2026-09-16-collapse-kernel-test-support-into-testkit.md)
  proposes deduplicating `init_logs()`; the `debug` default set here moves with that work.
