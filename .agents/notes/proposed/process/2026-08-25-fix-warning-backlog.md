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

Re-enable and clear the compiler-guided rustc lints, one group at a time: `unused_imports`,
`unused_variables`, `unused_mut`, `unused_macros`, `unused_doc_comments`, `unused_must_use`,
then the tail (`unreachable_code`, `dropping_references`, `type_alias_bounds`). Delete dead
imports and drop or `_`-rename unused bindings. **Watch grouped `use` lines** (rustc reports the
whole group when only part is unused) and **per-target reporting** (an import unused in the lib
target may be needed by the test target — move it there, don't delete it).

**Done (2026-09-21):** all of the above; every `allow` line removed. `unused_mut` via
`cargo fix`, the rest hand-fixed. Findings worth keeping:

- `unused_must_use` surfaced a **real bug**: `operators/time/inspect_frontier.rs::on_data`
sent without `.await`, dropping every data message — fixed.
- Two reasoned scoped `#[allow]`s remain: `routers/interrogate.rs::apply`
 (`#[allow(unused_variables, unreachable_code)]`) — its data-message arm is a `todo!()` stub, so
the written `route`/`send` tail is unreachable (remove when the router is implemented); and
`operators/stateless_op.rs` (`#[allow(type_alias_bounds)]`) — the alias body needs `In: Kvt` to
name `In::Key`, so the lint's suggested removal does not compile.

### Step 2 — Mechanical clippy one-offs

Re-enable and clear the lints needing no design judgment: `let_and_return`,
`redundant_field_names`, `needless_borrow`/`needless_borrows_for_generic_args`,
`collapsible_if`, `map_clone`, `useless_conversion`/`unnecessary_to_owned`, `derivable_impls`,
`redundant_closure`, `redundant_pattern_matching`, `option_map_unit_fn`, `clone_on_copy`,
`doc_lazy_continuation`, `empty_line_after_doc_comments`, `unused_unit`, `upper_case_acronyms`.

**Done (2026-09-21):** all cleared; every `allow` removed. `cargo clippy --fix` applied most
(24 files); the rest hand-fixed (`doc_lazy_continuation` blank-line insertion, an
`empty_line_after_doc_comments` doc→`//`, `if let Some(_)`→`.is_some()`, and the
`KVT`→`Kvt` test-local alias rename).

### Step 3 — Document the public extension API

Re-enable `missing_docs` and write the doc comments for the items the crate splits made `pub`
(stream/operator logic traits, `channels::operator_io`, `runtime::communication`,
protocol-message constructors, …). Keep the docs **meaningful**, not filler.

> Re-enable subtlety: `missing_docs` is `allow` **by default** in rustc, so this step sets
> `missing-docs = "warn"` (it does **not** just remove an `allow` line, which would leave the
> lint off). The other rustc lints in Step 1 are `warn` by default, so removing their `allow`
> does re-enable them.

**Done (2026-09-21):** 39 items documented across the buildable crates (core, macros,
distributed, operators, testkit); `missing-docs = "warn"` in `[workspace.lints.rust]`.
`cargo clippy -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc` are both green.

### Step 4 — Judgment lints (decide, don't blanket-allow)

Re-enabled and re-measured 2026-09-22 (all remaining groups set to `warn`, all buildable
crates, `--all-targets`; see also the [complexity survey](../../../../docs/reviews/2026-09-22-complexity-simplification-survey.md), finding 1): the counts have drifted sharply
since the Step 0 measurement — most notably `unwrap_used` is no longer 9 but **181 sites**,
because the tree has grown tests and examples since. Current picture:

| Group | Count | Where |
|---|---|---|
| `clippy::unwrap_used` | 181 (155 `Result` + 26 `Option`) | ~55 production; rest in `tests/`, `examples/`, testkit, slatedb examples |
| `async_fn_in_trait` | 23 | `operator_logic.rs` (12, the `Logic`/`SafeLogic` family), combinator stateful sources/ops (10), `recv_trait.rs` (1) |
| `dead_code` | ~40 items | `watchmap.rs` (10), `channels/signal.rs` (6), `tests/common/mod.rs` (10), memory-comm/router stubs, `types/message.rs` (3), scattered |
| `type_complexity` | 11 | internal plumbing signatures |
| `multiple_crate_versions` | 12 | transitive: `syn` 2/3, `thiserror` 1/2, `rand` 0.8/0.9/0.10, `windows-sys`, `getrandom`, … |
| `await_holding_refcell_ref` | 3 | `fn_source.rs:207`, `sources/stateful.rs:211,366` |
| one-offs | 6 | `private_interfaces` (`worker/builder.rs:24`), `too_many_arguments` (`build_context.rs:35`, 8/7), `new_without_default` (`Forward`), `module_inception` ×2 (`types/sealed.rs`, `worker/mod.rs`), `wrong_self_convention` ×1, `refining_impl_trait_reachable` ×1 (`core-internal/src/spsc.rs:150`) |

Plan: one commit per sub-step; delete the group's line from `[workspace.lints]` as it closes
(Step 5 rule).

#### 4a — `dead_code`: delete first, judge second

- Delete `coordinator/watchmap.rs` outright (238 LOC, zero consumers; the survey's first
  win).
- Delete verifiably dead kernel items: `types/message.rs`'s `PartOrData` et al., the `ReqRes`
  alias, `KeyByWidUnwrapper`, `Condition`/`ConditionIter`; audit `channels/signal.rs` (it is
  `pub(crate)`, so nothing outside the kernel can use it — if the threaded runtime does not
  either, it is dead).
- Unfinished-but-deliberate distributed stubs (`StreamSendClient`/`StreamRecvClient`, the
  memory-comm flavor, `remote_sender`): reasoned `#[allow(dead_code)]` naming the owning note
  ([unify-operator-io-edge-abstractions](../architecture/2026-09-13-unify-operator-io-edge-abstractions.md)),
  or delete if truly orphaned.
- Test-support items (`tests/common/mod.rs`, testkit fixtures): file-top
  `#[allow(dead_code)]` with the reason "shared helpers; not every test binary uses every
  helper" (each `tests/*.rs` is its own crate).

#### 4b — `clippy::unwrap_used`: split the policy by target kind

- **Tests, examples and testkit keep `unwrap`** — panicking loudly is the correct contract
  there. Scope: file-top `#[allow(clippy::unwrap_used)]` (with that reason) in `tests/`,
  `examples/`, and `malstrom-testkit`.
- **Production code: convert every `unwrap` to `expect("invariant")`** — or `?` where an
  error path exists (~55 sites: `inter_thread.rs` 13, `cluster.rs` 12, `stateful_op.rs` 9,
  `assign_timestamps.rs` 6, and smaller).
- End state: `unwrap_used = "deny"` in `[workspace.lints]`, with scoped allows only where
  panic-on-failure is the contract.

#### 4c — `async_fn_in_trait` / `refining_impl_trait_reachable`: reasoned allow, scoped to the traits

The native-async `Logic`/`SafeLogic` design is deliberate; desugaring to
`impl Future + Send` would force `Send` bounds through the whole operator API for no benefit.
Allow **on the trait definitions** (5 sites) with the reason written there — not
workspace-wide. `recv_trait.rs` and its `spsc` impl get the same treatment, citing
[unify-operator-io-edge-abstractions](../architecture/2026-09-13-unify-operator-io-edge-abstractions.md),
which will likely delete the trait.

#### 4d — The six one-offs: fix, don't allow

- `private_interfaces`: `WorkerBuilder::root_operator` leaks the `pub(crate)` `RootLogic` —
  restructure the builder API (or make the type properly `pub`).
- `too_many_arguments` (`build_context.rs`): introduce a params struct.
- `new_without_default` (`Forward`): trivial `impl Default`.
- `module_inception` ×2: rename (`types::sealed`, `worker::worker`).
- `wrong_self_convention`: rename or take `&self`.
- `await_holding_refcell_ref` ×3: restructure to drop the borrow before `.await` — genuine
  correctness hazards; only `#[allow]` with a written proof if restructuring is impossible.

#### 4e — `clippy::multiple_crate_versions`

Try `cargo update` dedup first; duplicates that remain are transitive dependencies we do not
own → reasoned workspace `allow`.

Final Step 4 state: at most the scoped, reasoned allows from 4b/4c/4e remain; every other
line is deleted from `[workspace.lints]`.

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
`cargo clippy -p malstrom-core -p malstrom-macros -p malstrom-distributed -p malstrom-combinators -p malstrom-testkit -p malstrom-snapshot-slatedb --all-targets -- -D warnings`.
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