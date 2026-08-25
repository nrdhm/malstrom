# Agent Note: Remove the dead `malstrom-macros` dependency from the kernel

Status: proposed

## Problem

`malstrom-core/Cargo.toml` still declares `malstrom-macros = { path = "../malstrom-macros" }`,
but nothing in `malstrom-core/src` references it: the `TTLState` derive it provides moved to
`malstrom-operators` during
[split-malstrom-core](../../implemented/architecture/2026-08-24-split-malstrom-core.md)
(`malstrom-operators/src/operators/ttl_map.rs` uses it, `operators/mod.rs` re-exports it, and
`malstrom-operators` declares the dependency itself). The kernel-manifest cleanup recorded as
Decision 9 of the split note dropped `rand`, `expiremap`, `eyre`, and the SlateDB stack but
missed this entry.

Harmless at runtime (cargo ignores unused dependencies), but it makes the kernel's
`Cargo.toml` overstate its real dependency surface, so the split note's "kernel manifest is
lean" claim is one line short of true.

## Proposal

Delete the `malstrom-macros = { path = "../malstrom-macros" }` line from
`malstrom-core/Cargo.toml`.

## Alternatives considered

- **Status quo** — zero churn; the dependency is inert. Lost: the kernel manifest keeps
  advertising a dependency it does not use, and the split note's leanness claim stays slightly
  inaccurate.
- **Re-add a `malstrom-macros` user to the kernel** — pointless; the only consumer
  (`TTLState`) belongs in the operator layer and already lives there.

## Acceptance criteria

- `cargo check --workspace` and `cargo test --workspace` are green.
- `malstrom-macros` no longer appears in `malstrom-core/Cargo.toml` and
  `cargo tree -p malstrom -i malstrom-macros` reports no path.

## Risks

- None. `malstrom-macros` is a proc-macro crate with no kernel call sites; removing the
  declaration cannot change kernel behavior.
