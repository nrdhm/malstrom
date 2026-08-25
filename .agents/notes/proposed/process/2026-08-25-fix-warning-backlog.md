# Agent Note: Clean the warning backlog and enforce `-D warnings` in CI

Status: proposed

## Problem

[add-ci-checks](../../implemented/process/2026-08-25-add-ci-checks.md) removed the
`-Awarnings` suppression and gated CI on `cargo clippy -- -D clippy::correctness` — but the
tree is **not** warning-clean, so the gate is honest yet weak: the ~420 remaining rustc
warnings and ~30 warn-level clippy lints are visible but unenforced, and can be reintroduced
silently by any change. Widening the gate to `-D warnings` is impossible until the backlog is
gone.

Measured backlog (2026-08-25, with `-Awarnings` removed, before the 5 `unused_must_use`
errors were fixed):

- **~215 `unused import` warnings** — fallout from the crate splits (split-malstrom-core,
  rename-kernel-and-add-malstrom-facade): imports that became dead when modules moved or the
  facade took over the `malstrom::…` surface. Mostly in `malstrom-examples` and the layer
  crates.
- **~72 `unused variable`** — the same migrations left unused bindings (incl. `_`-able
  parameters).
- **~50 `missing_docs`** on the public operator extension API widened in split-malstrom-core.
- **~30 assorted clippy/rustc warns** — `dead_code` (~6), `unused_mut`, `unused_doc_comments`,
  `async_fn_in_trait`, `type_alias_bounds`, `refining_impl_trait`, `private_interfaces`,
  `mismatched_lifetime_syntaxes`, `dropping_references`, `unreachable_code`, `unused_macros`.

## Proposal

Clean the backlog and then widen the gate, keeping the toolchain pin (1.97.1) so the
`-D warnings` gate is stable:

1. **Unused imports/variables sweep** — compiler-guided: delete dead `use` lines and drop or
   `_`-rename unused bindings across `malstrom-examples` and the layer crates. Mechanical;
   iterate `cargo clippy` until the `unused*` warnings are gone. Where a test module's import
   is dead, delete it (behavior unchanged).
2. **Document the public extension API** — write the ~50 missing doc comments for the items
   `split-malstrom-core` made `pub` (stream/operator logic traits, `channels::operator_io`,
   `runtime::communication`, protocol-message constructors, etc.). Keep the
   `missing-docs = warn` workspace lint; docs are the contract for the now-public surface.
3. **Fix the tail of one-off lints** — `dead_code` (remove or `#[allow]` with a reason),
   `async_fn_in_trait`, `type_alias_bounds`, `refining_impl_trait`, `private_interfaces`, and
   the rest. **Review each before a mechanical fix**: some (e.g. `async_fn_in_trait`,
   `refining_impl_trait`) may indicate a real API smell worth a deliberate decision rather
   than a drive-by `#[allow]`.
4. **Widen the CI gate** — in `.github/workflows/ci.yaml`, change the clippy step from
   `-D clippy::correctness` to `-D warnings` (covers rustc + clippy warnings in one flag),
   and keep `cargo check --workspace --all-targets` (it must emit zero warnings). Update the
   `.cargo/config.toml` note that this is a tracked follow-up.
5. **Prevent regression** — the widened gate is the enforcement; no per-file allow-lists
   except for genuinely noisy lints with a written reason.

## Alternatives considered

- **Status quo (correctness-only gate)** — zero work, but warning drift accumulates
  invisibly and the ~420-warning noise stays in every build. Rejected: the backlog is mostly
  mechanical, and the gate is the point of having a gate.
- **Allow-list the noisy groups in CI** (e.g. `-A unused-imports`, `-A missing-docs`) —
  keeps the backlog but makes the gate stay weak; unused imports are dead surface that
  should be removed, not suppressed. Rejected.
- **`#![deny(warnings)]` per crate** — less flexible than the CI flag (can't distinguish
  rustc vs clippy contexts, awkward for test targets) and spreads the policy across files.
  Rejected; the CI `-D warnings` flag is the single standard place.
- **`cargo fix --all-targets` auto-fix first** — safe for many `unused_imports`, but
  `missing_docs` and the one-off lints need human judgment; use `cargo fix` only as a first
  pass for imports/vars.

## Acceptance criteria

- `cargo clippy --workspace --all-targets -- -D warnings` passes with **zero** warnings and
  errors.
- `cargo check --workspace --all-targets` emits zero warnings.
- `cargo fmt --all -- --check` clean and `cargo test --workspace` green (unchanged).
- The CI clippy step in `ci.yaml` is widened to `-D warnings`; the `.cargo/config.toml`
  "follow-up" note is removed.
- The public API added by split-malstrom-core is documented (no `missing_docs` warnings).

## Risks

- **`missing_docs` effort may exceed the ~50 measured** — writing *meaningful* docs (not
  filler) for the public extension API is the real cost of this change; the lint can also
  surface items in non-obvious places once others are fixed. Scope it (complete the docs,
  don't weaken the lint).
- **One-off lints may be API smells** — `async_fn_in_trait` and `refining_impl_trait` on
  public traits deserve a deliberate read (they are part of the versioned extension API);
  do not blanket-`#[allow]` them without a written reason.
- **`-D warnings` brittleness** — new rustc/clippy lints on newer toolchains would fail CI;
  mitigated by the pinned `rust-toolchain.toml` (1.97.1). Toolchain bumps must re-clean the
  tree.
- **Diff size** — the unused-import sweep touches dozens of files; keep it to one commit so
  it is reviewable, separate from any behavior change.
