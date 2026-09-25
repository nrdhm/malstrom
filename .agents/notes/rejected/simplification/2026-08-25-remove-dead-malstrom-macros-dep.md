# Agent Note: Remove the dead `malstrom-macros` dependency from the kernel

Status: rejected — the dependency stopped being dead before this could land:
[instrument-debug-by-default](../../implemented/architecture/2026-09-20-instrument-debug-by-default.md)
made the kernel a real `malstrom-macros` consumer.

## Problem

`malstrom-core/Cargo.toml` still declares `malstrom-macros = { path = "../malstrom-macros" }`,
but nothing in `malstrom-core/src` references it: the `TTLState` derive it provides moved to
`malstrom-combinators` during
[split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
(`malstrom-combinators/src/combinators/ttl_map.rs` uses it, `combinators/mod.rs` re-exports it,
and `malstrom-combinators` declares the dependency itself). The kernel-manifest cleanup
recorded as Decision 9 of the split note dropped `rand`, `expiremap`, `eyre`, and the SlateDB
stack but missed this entry.

Harmless at runtime (cargo ignores unused dependencies), but it makes the kernel's
`Cargo.toml` overstate its real dependency surface, so the split note's "kernel manifest is
lean" claim is one line short of true.

## Proposal

Delete the `malstrom-macros = { path = "../malstrom-macros" }` line from
`malstrom-core/Cargo.toml`.

**Rejected (2026-09-22):** the premise is gone. `instrument-debug-by-default` (commit
`c481c08`) spread the `instrument_debug` derive across the kernel — 8 call sites
(`channels/operator_io.rs`, `stream/operator.rs`, `stream/operator_logic.rs`,
`coordinator/coordinator.rs`, `worker/{worker,builder,root_logic,coordination_task}.rs`). The
dependency is now live and required; removing it would break the kernel build. This note is
kept because the measurement that motivated it can recur: a dependency can become dead again
only through the same kind of later drift, and any future "dead dep" claim must be checked
against the current tree, not an older audit.

## Alternatives considered

- **Status quo** — zero churn; the dependency is inert. Lost: the kernel manifest keeps
  advertising a dependency it does not use, and the split note's leanness claim stays slightly
  inaccurate.
- **Re-add a `malstrom-macros` user to the kernel** — dismissed as pointless in 2026-08-25
  (the only consumer, `TTLState`, belongs in the operator layer). This is effectively what
  happened via `instrument_debug`, which is why the removal is now impossible.