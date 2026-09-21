# Agent Note: Clean the warning backlog and enforce `-D warnings` in CI

Status: proposed

## Problem

[add-ci-checks](../../implemented/process/2026-08-25-add-ci-checks.md) removed the
`-Awarnings` suppression and gated CI on `cargo clippy -- -D clippy::correctness` — but the
tree is **not** warning-clean, so the gate is honest yet weak: the remaining rustc/clippy
warnings are visible but unenforced, and can be reintroduced silently by any change. Widening
the gate to `-D warnings` is impossible until the backlog is gone.

Measured backlog (2026-09-21, buildable crates — see "Scope" below), under
`cargo clippy --all-targets -- -D warnings`:

| Group | Count | Examples |
|---|---|---|
| `missing_docs` | 30 | public extension API widened by split-malstrom-core |
| `unused_imports` | 25 | dead imports left by the crate splits (many on grouped `use` lines) |
| `dead_code` | 23 | unreferenced items, chiefly `coordinator/watchmap.rs` |
| `async_fn_in_trait` | 13 | the `SafeLogic` extension trait |
| `clippy::unwrap_used` | 9 | production + test unwraps |
| `unused_variables` | 5 | unused bindings/params |
| `clippy::clone_on_copy`, `option_map_unit_fn`, `doc_lazy_continuation`, `unused_must_use` | 3 each | one-off lints |
| `let_and_return`, `unused_doc_comments`, `redundant_field_names`, `collapsible_if`, `needless_borrow`, `module_inception` | 2 each | one-off lints |
| `refining_impl_trait_reachable`, `unused_mut`, `private_interfaces`, `multiple_crate_versions`, `map_clone`, `useless_conversion`, `derivable_impls`, `redundant_closure`, `too_many_arguments`, `wrong_self_convention`, `new_without_default`, `type_complexity`, `redundant_pattern_matching`, `empty_line_after_doc_comments`, `unused_unit`, `unused_macros` | 1 each | one-off lints |
| **Total (non-docs)** | **116** | |

This is the residue of the crate splits (split-malstrom-core, rename-kernel-and-add-malstrom-facade)
and the module moves since. It is far below the ~420 measured on 2026-08-25; most of that
earlier backlog has already been cleaned.

Incidental finding (2026-09-21): the staged tree had a latent **build break** — the manual
`impl Debug for Message` needed `#[derive(Debug)]` on `Collect`/`Interrogate` that was missing.
Fixed by adding the derives. Called out here because it is the kind of inconsistency a
warning-clean, `-D warnings` gate would have surfaced immediately.

## Proposal

**Strategy: disable every firing lint explicitly (clean gate now), then re-enable one group
at a time, fixing each as it comes back.** This lands the widened `-D warnings` gate
immediately (so nothing new can be introduced silently) and makes the cleanup incremental and
reviewable: each step re-enables a group, fixes its diagnostics, and leaves the gate green.
The end state has **no lint rules ignored**.

### Step 0 — Explicitly disable every firing lint (done 2026-09-21)

Every lint that fires under `-D warnings` is set to `allow` in `[workspace.lints]` (root
`Cargo.toml`), with `priority = 1` on clippy lints that belong to an enabled group so they do
not trip `lint_groups_priority`. CI clippy is widened to
`cargo clippy --workspace --all-targets -- -D warnings`. Result: the gate is green across all
buildable crates with everything still visibly listed and tracked.

Every crate must carry `[lints] workspace = true` for this to apply; `malstrom-macros` was
missing it (and the k8s/kafka crates still are — they cannot be checked locally but must be
fixed for the CI gate to be real).

Enforced allow-list at Step 0:

**`[workspace.lints.rust]`** — `missing_docs`, `async_fn_in_trait`, `unused_imports`,
`unused_variables`, `unused_mut`, `unused_macros`, `unused_must_use`, `unused_doc_comments`,
`dead_code`, `private_interfaces`, `refining_impl_trait_reachable`, `unreachable_code`,
`dropping_references`, `type_alias_bounds`.

**`[workspace.lints.clippy]`** (all `allow`, `priority = 1`) — `unwrap_used`,
`clone_on_copy`, `option_map_unit_fn`, `doc_lazy_continuation`, `let_and_return`,
`redundant_field_names`, `collapsible_if`, `needless_borrow`,
`needless_borrows_for_generic_args`, `module_inception`, `empty_line_after_doc_comments`,
`unused_unit`, `multiple_crate_versions`, `map_clone`, `useless_conversion`,
`derivable_impls`, `redundant_closure`, `too_many_arguments`, `wrong_self_convention`,
`new_without_default`, `type_complexity`, `redundant_pattern_matching`,
`unnecessary_to_owned`, `upper_case_acronyms`, `await_holding_refcell_ref`.

The clippy groups (`correctness`, `suspicious`, `complexity`, `perf`, `style`, `cargo`) keep
their existing levels; individual group members are allowed on top of them.

> Enumeration note: some lints only surface once earlier ones are cleared (and clippy's
> cached `check` pass hides rustc lints in plain `-D warnings` mode). Enumerate iteratively:
> run `-D warnings`, add each newly-seen lint to the allow-list, repeat until clean.

### Step 1 — Mechanical rustc sweep

Re-enable and clear the compiler-guided rustc lints, one group at a time. Delete dead imports
and drop or `_`-rename unused bindings. **Watch grouped `use` lines**: rustc reports the whole
group as unused when only part is, so edit per item rather than dropping the line (dropping a
needed import broke `PhantomData` during this work). Order: `unused_imports` (25),
`unused_variables` (5), `unused_mut` (1), `unused_macros` (1), `unused_doc_comments` (2),
`unused_must_use` (3), then the tail (`unreachable_code`, `dropping_references`,
`type_alias_bounds`).

**Progress:** `unused_imports` done (2026-09-21) — `cargo fix --all-targets` applied most
(36 files), the remaining 9 grouped/test-module imports were hand-fixed, and the
`unused_imports = "allow"` line was removed from `[workspace.lints.rust]`. Note the per-target
subtlety: an import can be unused in the lib target but needed by the test target (e.g.
`SafeLogic` in `assign_timestamps.rs`), in which case it moves to the test module's own import
rather than being deleted.

### Step 2 — Mechanical clippy one-offs

Re-enable and clear the lints needing no design judgment: `let_and_return`,
`redundant_field_names`, `needless_borrow`/`needless_borrows_for_generic_args`,
`collapsible_if`, `map_clone`, `useless_conversion`/`unnecessary_to_owned`, `derivable_impls`,
`redundant_closure`, `redundant_pattern_matching`, `option_map_unit_fn`, `clone_on_copy`,
`doc_lazy_continuation`, `empty_line_after_doc_comments`, `unused_unit`,
`upper_case_acronyms`.

### Step 3 — Document the public extension API

Re-enable `missing_docs` and write the ~30 doc comments for the items the crate splits made
`pub` (stream/operator logic traits, `channels::operator_io`, `runtime::communication`,
protocol-message constructors, …). Keep the docs **meaningful**, not filler.

### Step 4 — Judgment lints (decide, don't blanket-allow)

Re-enable each and resolve with a deliberate decision; any remaining `#[allow]` must carry a
written reason:

- `dead_code` (23) — remove the item, or reasoned `#[allow(dead_code)]` for a deliberate
  extension point.
- `async_fn_in_trait` (13) and `refining_impl_trait_reachable` (1) on `SafeLogic` — decide
  whether the trait returns `impl Future`/boxed futures, or reasoned `#[allow]`.
- `private_interfaces`, `too_many_arguments`, `type_complexity`, `wrong_self_convention`,
  `new_without_default`, `module_inception` — small; fix or reasoned `allow`.
- `clippy::unwrap_used` (9) — replace with `?`/`expect` where an error path exists, else
  reasoned `#[allow]`.
- `clippy::multiple_crate_versions` — resolve the duplicate dependency or reasoned `allow`.
- `clippy::await_holding_refcell_ref` (6) — a real async-correctness smell; fix the hold
  across await or reasoned `allow`.
- Unused imports that are `pub use` re-exports — decide keep (reasoned allow) vs drop.

### Step 5 — Verify no suppressions remain

The end state has **no `allow` for any lint**. When a step's group is clean, its line is
**deleted** from `[workspace.lints]` (restoring the default/group level), so the gate enforces
it again. When all lines are gone the manifest returns to `missing-docs = "warn"` plus the
clippy groups, with nothing ignored.

### Step 6 — Prevent regression

The widened gate is the enforcement; no per-file allow-lists except genuinely noisy lints with
a written reason (none should remain when this note is done). Add `[lints] workspace = true`
to every crate that lacks it (`malstrom-macros` fixed 2026-09-21; the k8s/kafka crates still
need it).

## Scope

The local Termux environment cannot build `malstrom-kafka` (rdkafka) or `malstrom-k8s/proto`
(`protoc`), so steps are verified with
`cargo clippy -p malstrom-core -p malstrom-macros -p malstrom-distributed -p malstrom-operators -p malstrom-testkit -p malstrom-snapshot-slatedb --all-targets -- -D warnings`.
The full `--workspace` gate (including the k8s/kafka crates) runs in CI.

## Alternatives considered

- **Status quo (correctness-only gate)** — zero work, but warning drift accumulates
  invisibly and the backlog stays in every build. Rejected: the backlog is mostly mechanical,
  and the gate is the point of having a gate.
- **Allow-list the noisy groups in CI** (e.g. `-A unused-imports`) — keeps the backlog but
  makes the gate stay weak; unused imports are dead surface that should be removed, not
  suppressed. Rejected. `missing_docs` is the **one** temporary exception (Step 0), because
  writing meaningful docs is a separate, larger effort; it is removed in Step 3.
- **Permanently allow `missing_docs`** — rejected: it would leave a lint ignored forever. The
  final state has **no lint rules ignored**.
- **`#![deny(warnings)]` per crate** — less flexible than the CI flag (can't distinguish
  rustc vs clippy contexts, awkward for test targets) and spreads the policy across files.
  Rejected; the CI `-D warnings` flag is the single standard place.
- **`cargo fix --all-targets` auto-fix everything** — safe for many `unused_imports`, but the
  one-off lints, `missing_docs`, and `dead_code` need human judgment; use `cargo fix` only as
  a first pass for imports/vars.

## Acceptance criteria

- Step 0 done: `cargo clippy --workspace --all-targets -- -D warnings` is green with every
  firing lint explicitly `allow`ed in `[workspace.lints]`, and CI runs that exact command.
- Final state: **every `allow` line is removed** and the gate is still green — i.e. **no lint
  rules are ignored**. `[workspace.lints]` returns to `missing-docs = "warn"` plus the clippy
  groups, with no per-lint `allow`.
- `cargo check --workspace --all-targets` emits zero warnings.
- `cargo fmt --all -- --check` clean and `cargo test --workspace` green (unchanged).
- Every workspace member carries `[lints] workspace = true` (the k8s/kafka crates included).
- `.cargo/config.toml` no longer describes a "follow-up".

## Risks

- **Step 0 is a real suppression** — every firing lint is `allow`ed, so the gate enforces
  only what is re-enabled step by step. This is deliberate and temporary; the acceptance
  criteria require every `allow` line to be removed, leaving no lint ignored.
- **Crates missing `[lints] workspace = true`** silently escape the workspace config; the
  gate is only real where a crate opts in (k8s/kafka still need it).
- **`missing_docs` effort may exceed the ~30 measured** — writing *meaningful* docs is the
  real cost of Step 3; the lint can surface items in non-obvious places once others are
  fixed. Scope it (complete the docs, don't weaken the lint).
- **One-off lints may be API smells** — `async_fn_in_trait`, `refining_impl_trait_reachable`,
  and `await_holding_refcell_ref` deserve a deliberate read (the last is a real
  async-correctness hazard); do not blanket-`#[allow]` them without a written reason.
- **`-D warnings` brittleness** — new rustc/clippy lints on newer toolchains would fail CI;
  mitigated by the pinned `rust-toolchain.toml` (1.97.1). Toolchain bumps must re-clean the
  tree.
- **Enumeration is iterative** — some lints only appear once earlier ones are cleared, and
  clippy's cached `check` pass hides rustc lints in plain `-D warnings` mode; each step must
  re-run `-D warnings` to catch stragglers.
- **Diff size** — the sweep touches dozens of files; land it as one commit so it is
  reviewable, separate from any behavior change.