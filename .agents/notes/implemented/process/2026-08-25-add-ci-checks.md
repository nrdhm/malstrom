# Agent Note: Add CI checks for tests, lints, and formatting

Status: implemented

## Problem

The repository had no CI gate for code quality. The only two workflows were
`.github/workflows/ghcr.yaml` (pushes Docker images) and `.github/workflows/pages.yaml`
(deploys the docs site) — both triggered **only on `push` to `main`**, so pull requests ran
nothing: no tests, no lints, no format check. PR #2 (`nrdhm/malstrom` fork) consequently
showed no checks.

Two things would have made even a naive CI gate toothless:

- `.cargo/config.toml` set `-Awarnings` in `RUSTFLAGS`, silently suppressing every warning —
  including the workspace lints in the root `Cargo.toml` (`correctness = deny`, clippy
  `suspicious`/`complexity`/`perf`/`style`, `missing-docs = warn`). A `clippy -- -D warnings`
  step would pass vacuously until this was removed.
- There was no `rust-toolchain.toml`, so the toolchain was unpinned (the crates use edition
  2024, which needs a recent stable).

A further known gap (not fixed here): `malstrom-kafka`/`malstrom-k8s` pin the **published**
crates.io `malstrom 0.1.0`, so `--workspace` builds them against the old crate, not the branch —
CI is honest only for the `malstrom*` crates until
[point-k8s-and-kafka-at-local-malstrom](../../proposed/process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md)
lands.

## Decision

Added a `ci.yaml` workflow and made the correctness gate meaningful:

1. **`.github/workflows/ci.yaml`** — one job, `on: pull_request` + `push: { branches: [main] }`
   + `workflow_dispatch`:
   - `actions/checkout@v4`
   - `dtolnay/rust-toolchain@stable` with `toolchain: 1.97.1`, `components: rustfmt, clippy`
   - `Swatinem/rust-cache@v2`
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D clippy::correctness`
   - `cargo check --workspace --all-targets`
   - `cargo test --workspace`

   `--all-targets` covers examples/benches/tests, including the `console-subscriber` example
   that relies on the `tokio_unstable` cfg in `.cargo/config.toml`.

2. **Removed `-Awarnings`** from `.cargo/config.toml` (kept `--cfg tokio_unstable`). This
   surfaced a **large backlog the proposal under-estimated**: ~420 rustc/clippy warnings,
   mostly **unused imports** left over from the crate splits (split-malstrom-core,
   rename-kernel-and-add-malstrom-facade), plus ~50 `missing-docs` on the public extension
   API and ~30 assorted clippy lints.
3. **Scoped the clippy gate (per decision)** — the tree is not yet warning-clean, so the
   gate is `-D clippy::correctness` (the correctness group is already `deny` in the
   workspace lints; without `-Awarnings` it is now actually enforced). The 5 real
   correctness errors that surfaced — ignored `File::write` results (`unused_must_use`) in
   `file_sink_stateful.rs`/`file_sink_stateless.rs` — were fixed (`write` → `write_all`).
   Widening to `-D warnings` is a tracked follow-up (the cleanup is documented in
   `.cargo/config.toml`).
4. **Pinned the toolchain** — `rust-toolchain.toml` with `channel = "1.97.1"` (exact stable)
   + `rustfmt`/`clippy` components, matching the workflow action.
5. **Fork-PR trigger nuance** — for `pull_request` from a fork, GitHub runs the workflow from
   the **target (base) branch**, not the head fork (a security measure). So `ci.yaml` must
   land on the branch PR #2 targets before that PR will run the checks.

## Alternatives considered

- **Keep the push-only triggers and add a test job to the existing workflows** — mixes image
  push/docs deploy with code gates and still needs a `pull_request` trigger; a single dedicated
  `ci.yaml` is clearer and independently cacheable.
- **`cargo clippy -- -D warnings` without removing `-Awarnings`** — the gate would pass
  vacuously (all warnings suppressed); pointless.
- **Fix the whole warning backlog now** (unused imports, missing docs, all clippy groups) —
  the "full cleanup" option; ~150+ mechanical edits. Not chosen: the correctness gate is the
  meaningful minimum, and the cleanup is better done as its own tracked change.
- **`--exclude malstrom-k8s --exclude malstrom-kafka` in check/test** — makes the "we don't
  validate them against the local kernel" fact explicit, but removes coverage of their own
  tests (k8s 5, kafka 9) which do pass against `0.1.0`. Keep them included and document the
  gap instead.
- **Run clippy/check/test as a matrix (stable + nightly)** — edition 2024 and `tokio_unstable`
  work on stable; nightly adds noise for no current benefit. Defer until something needs it.

## Consequences

- **PRs now run a real gate** — format, clippy-correctness, check, and tests run on every
  pull request and push to `main`.
- **Warnings are visible again** — removing `-Awarnings` means the ~420-warning backlog shows
  in every build (noisy but non-blocking at the correctness level). The gate is honest about
  what it does and does not enforce.
- **Toolchain is pinned** — `rust-toolchain.toml` (1.97.1) gives local/CI parity.
- **Known gap documented** — kafka/k8s still compile against crates.io `malstrom 0.1.0`;
  CI is honest for the `malstrom*` crates until the k8s/kafka re-pointing lands.
- **Verification** — `cargo fmt --all -- --check` clean (51 files reformatted to rustfmt);
  `cargo clippy --workspace --all-targets -- -D clippy::correctness` passes with 0 errors;
  `cargo check --workspace --all-targets` clean; `cargo test --workspace` green (malstrom-core
  19, operators 40, testkit 2, slatedb 5, kafka 9, k8s 5; the full-workspace run is slow —
  the first cold run exceeds 10 minutes compiling rdkafka/k8s).
