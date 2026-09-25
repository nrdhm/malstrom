> **Last refreshed:** 2026-09-22

# Review: complexity / simplification survey

Scope: a whole-tree survey through the
[`dsh-find-simplifications`](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/skills/dsh-find-simplifications/SKILL.md)
lens — dead, duplicated, speculative, over-built, or hand-rolled surface — run against the
current `new-scheduler` tree (17 workspace members, ~17k LOC of Rust). It does not re-audit the
items already tracked in
[`fix-warning-backlog`](../../.agents/notes/proposed/process/2026-08-25-fix-warning-backlog.md)
beyond naming the remaining step; it ranks the highest-leverage complexity moves and states
what is deliberately **not** a candidate.

## Method

Intent first, then evidence. The Agent Note tree was read for settled seams and in-flight
design; `cargo`/`rg`/manifest inspection established consumers. Candidates were classified by
the skill's consumer rule (production corpus vs tests/docs/notes vs ambiguous), and a candidate
that only relocates complexity was rejected. Because the repository already has the lifecycle
and class machinery, the survey optimises for **closing loops that let complexity accumulate
silently**, not for a longer cleanup list.

## Ranking

1. Finish the lint gate (Step 4) and delete the surface it exposes.
2. Unify the operator edge abstraction before adding the next transport.
3. Consolidate the source control-plane design and re-sync the notes to the post-split tree.

## 1. Finish the lint gate (Step 4)

**Pattern:** a standing complexity control is half-installed. `correctness` is denied, but four
rustc lints and eight clippy lints are still `allow`ed in the workspace manifest
([`Cargo.toml`](../../Cargo.toml) lines 28–50), so dead and speculative code can still land
unnoticed. `fix-warning-backlog` Steps 0–3 are done; **Step 4, the judgment lints, is open**.

**Evidence — concrete first wins:**

- **`WatchMap` is dead.** [`malstrom-core/src/coordinator/watchmap.rs`](../../malstrom-core/src/coordinator/watchmap.rs)
  is 238 LOC with zero call sites: the only reference in the tree is `mod watchmap;`
  ([`coordinator/mod.rs:13`](../../malstrom-core/src/coordinator/mod.rs)). It is a fully
  formed `Send + Sync` notify-on-change map with no consumer.
- **Two routers compile only because their tail is unreachable.**
  [`routers/interrogate.rs:64`](../../malstrom-distributed/src/routers/interrogate.rs)
  (`RouterInput::DataMessage(..) => todo!()`) and
  [`routers/normal.rs:60`](../../malstrom-distributed/src/routers/normal.rs)
  (`RouterInput::Complete(..) => todo!()`) are propped up by
  `#[allow(unused_variables, unreachable_code)]`; the adjacent
  [`collect.rs:151`](../../malstrom-distributed/src/routers/collect.rs) is `// TODO !!!!!`.
- **~~A dead dependency is still declared.~~ Retracted on verification (2026-09-22):**
  `malstrom-macros` is no longer dead in the kernel — `instrument_debug` landed in 8 kernel
  files with `instrument-debug-by-default`. The earlier audit's note is now
  [`rejected`](../../.agents/notes/rejected/simplification/2026-08-25-remove-dead-malstrom-macros-dep.md).
- **The gate is not yet real across the workspace.** k8s/kafka crates lack
  `[lints] workspace = true` and cannot be checked in the Termux environment, so
  `-D warnings` in CI is partly aspirational there.

**Remediation sketch:** land Step 4 as small, individually-reviewable changes — one lint group
per change — starting with `dead_code` (delete `watchmap.rs` or keep it only with a reasoned
`#[allow(dead_code)]` naming it a deliberate extension point), then make the unfinished router
arms explicit (implement, or feature-gate / a single `unimplemented!` instead of a straddled
`todo!`), and add `[lints] workspace = true` to every
member. The end state is the one the backlog note already specifies: no `allow` lines remain
and `cargo clippy --workspace --all-targets -- -D warnings` is green.

**Why first:** the ~250 LOC is incidental. A real `-D warnings` gate is the *mechanism* that
keeps the other two findings from regrowing.

## 2. Unify the operator edge abstraction

**Pattern:** two parallel edge implementations chosen by *placement*, the skill's "two
representations mirror the same fact" plus "moving the code changes the code path."

**Evidence:**

- Local inter-op edges are hardwired to the concrete `channels::spsc` type
  ([`operator_io.rs`](../../malstrom-core/src/channels/operator_io.rs): `Vec<Rc<spsc::Sender<Message<M>>>>`,
  `IndexMap<.., spsc::Receiver<..>>` at lines 24, 169, 196), documented unbounded but capped at
  a hidden `CAPACITY`, and with a receiver-drop/backpressure gap (`Receiver::drop` does not wake
  a sender parked on a full queue; `no_receivers()` at
  [`operator_io.rs:142`](../../malstrom-core/src/channels/operator_io.rs) is the workaround).
- Cross-worker edges are `flume::bounded(1024)` behind `StreamSender`/`StreamReceiver` traits,
  entered through `OperatorCommReceiver`.

The input side has a common trait; the output side does not, so local output is concrete and
remote output is trait-based. The consequence is that a new transport (TCP, IPC, shared memory)
forks the IO layer on both sides, and the `recv_trait::Receiver` "do we still need this trait?"
TODO cannot resolve because there is no symmetric sender.

**Remediation sketch:** land the design in
[`unify-operator-io-edge-abstractions`](../../.agents/notes/proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md) —
one `Allocate`-style factory returning symmetric async `Push`/`Pull`, with `spsc` adapted
behind it as the initial implementation. Interface first, **channel swap second**; the note's
own alternatives explain why swapping `spsc` first touches `Output`/`Input` twice and still
leaves the output concrete. The follow-up tokio-mpsc swap is
[`replace-operator-io-spsc-with-tokio-mpsc`](../../.agents/notes/proposed/architecture/2026-09-13-replace-operator-io-spsc-with-tokio-mpsc.md).

**Guardrail:** do not touch the twin comm trait/impl trees
(`runtime/communication/` vs `runtime/threaded/communication/`) or
[`malstrom-core-internal`](../../.agents/notes/proposed/architecture/2026-09-21-malstrom-core-internal-crate.md)
as part of this. Those are deliberately documented audience seams; per the skill, a recorded
seam needs its rationale beaten, not a surface look-alike.

## 3. Consolidate the source control-plane design

**Pattern:** the control plane rides the data plane, and the design to fix it is spread across
four overlapping proposed notes — design-space complexity of the "one fact, several homes" kind.

**Evidence:**

- `SourceCoordinator` announces partitions as fake records:
  `DataMessage::new(part, NoData, Timestamp::MIN)`, so discovery flows through
  `VersionedMessage`/`TargetedMessage` and the data router, which exist for records. Completion
  still uses a `PartitionFinished` side channel plus a hardcoded worker-0 authority.
- Four notes own slices of the same mechanism:
  [`first-class-source-discovery-message`](../../.agents/notes/proposed/architecture/2026-08-24-first-class-source-discovery-message.md),
  [`source-discovery-wire-message`](../../.agents/notes/proposed/architecture/2026-08-24-source-discovery-wire-message.md),
  [`frontier-merge-source-completion`](../../.agents/notes/proposed/architecture/2026-08-24-frontier-merge-source-completion.md),
  and
  [`safelogic-source-coordinator`](../../.agents/notes/proposed/architecture/2026-08-24-safelogic-source-coordinator.md).
- **The notes have drifted from the post-split tree.** All four cite
  `malstrom-core/src/sources/stateful.rs` and
  `malstrom-core/src/keyed/distributed/wire_message.rs`, which moved to
  [`malstrom-combinators/src/sources/stateful.rs`](../../malstrom-combinators/src/sources/stateful.rs)
  and [`malstrom-distributed/src/wire_message.rs`](../../malstrom-distributed/src/wire_message.rs)
  in the crate splits. That is exactly the path drift the note rules exist to prevent.

**Remediation sketch:** pick one owning note — control-plane/data-plane separation — fold the
other three's unique rationale into it, update every path to the current crates, and implement a
single in-band `SourcePartition` control message rather than layering a local variant plus a wire
variant. Respect the recorded ordering: the wire half is blocked on
[`point-k8s-and-kafka-at-local-malstrom`](../../.agents/notes/proposed/process/2026-08-22-point-k8s-and-kafka-at-local-malstrom.md)
because `malstrom-k8s` is pinned to the published `malstrom 0.1.0`.

**Why third:** it is the most invasive and partly design work, but it is the move that prevents
a second control protocol from being built on the first.

## Non-candidates (do not simplify these)

- **Undoing the crate splits.** The facade / `malstrom-core` / `malstrom-core-internal` /
  `malstrom-distributed` / `malstrom-combinators` boundaries are recorded, deliberate
  audience seams. A facade that preserves the historical import tree is a compatibility seam.
- **Deleting `malstrom-distributed` or the k8s/kafka crates** because they are unfinished.
  Distribution is a headline product pillar; make incompleteness explicit (finding 1), do not
  erase the direction.
- **Collapsing the `VersionedMessage` / `TargetedMessage` / `WireMessage` / `RouterOutput`
  wrappers** on inspection. They encode different hops (local sender+version, target+version,
  serialization, router output); finding 2 changes the edges beneath them, not necessarily
  these wrappers. Prove a consumer merge with call sites first.
- **Twin comm trait/impl trees.** Trait/impl split by design.
- **`malstrom-testkit`.** A test-only package by design, consumed by sibling unit tests, not a
  product package.

## Suggested sequence

1. Step 4 lint cleanup: `watchmap.rs`, the `todo!()` router arms, then the remaining judgment
   lints — a few small PRs, each removing an `allow`.
2. Land the symmetric edge trait (no channel swap yet) plus the placement-transparency test.
3. Coalesce the four source notes into one, repair the stale paths, then implement the control
   message; the k8s re-pointing unblocks the wire half.

## Related

- [`docs/overviews/08-public-api-surface.md`](../overviews/08-public-api-surface.md) — the
  audience matrix behind the "do not leak internal types" boundary.
- [`docs/overviews/06-channels.md`](../overviews/06-channels.md) and
  [`07-remote-transports-survey.md`](../overviews/07-remote-transports-survey.md) — the channel
  and transport baselines finding 2 builds on.