> **Last refreshed:** 2026-09-21

# Review: pre-commit assessment of the `new-scheduler-tests` working tree

Scope: the staged working tree on `new-scheduler-tests` (45 staged files, ~800 insertions)
plus the untracked paths — the `OperatorBuilder` / `Forward` refactor, the union/split rework,
the tracing/OTLP harness, new Agent Notes and review docs. This supersedes the former
`2026-09-13-current-changes-review.md`, which described an earlier state of the same tree;
several of its "must fix" items are still open and are re-listed below.

## State

- `cargo check` is green for `malstrom-core`, `malstrom-combinators`, `malstrom-macros`,
  `malstrom-testkit` and `malstrom-examples` (`--all-targets`).
- `scripts/verify-agent-notes.py` passes.
- `git diff --check` (staged and unstaged) is clean; `cargo fmt --all -- --check` passes.
- Tests pass: `cargo test -p malstrom-core` (unit + 4 integration binaries + 8 doc),
  `cargo test -p malstrom-combinators --lib` (32), `cargo test -p malstrom-macros --test
  instrument_debug` (4), `cargo test -p malstrom-testkit`.
- The branch has **diverged 22** from `fork/new-scheduler-tests`.

## Commit blockers

1. ~~**`cargo test` doctest break (fixed).**~~ **Fixed 2026-09-21.** The staged
   `malstrom-testkit/src/lib.rs` narrowed the crate root re-export to
   `pub use operator_tester::OperatorTester;`, dropping `FakeCommunication` and `SentMessage`.
   The `FakeCommunication` doctest still imported `malstrom_testkit::FakeCommunication`, so
   `cargo test -p malstrom-testkit` failed to compile its doctests. Restored the full re-export
   (`{FakeCommunication, OperatorTester, SentMessage}`); these are documented public types —
   `SentMessage` is the return type of `recv_from_operator`, and both are named in
   [`../overviews/05-architecture.md`](../overviews/05-architecture.md).

2. ~~**Commented-out dead code.**~~ **Fixed 2026-09-21.** Removed the empty
   `impl<T> SharedInner<T> {}` in `spsc.rs`, the commented `// let finalized_signal = ...` in
   `operator_io.rs`, the `// let Operator { operator_id, name } = self;` line in `operator.rs`,
   and the commented `// let worker = WorkerBuilder::new(...)` line in `operator_builder.rs`.
   (The `stream_builder.rs` swap alternatives listed in the first revision no longer existed —
   that code was already refactored away.)

3. ~~**Leftover `println!` / `eprintln!` scaffolding.**~~ **Fixed 2026-09-21.** Removed the nine
   debug prints from `operator_builder.rs`'s `builder_works` test, `println!("split got: ...")`
   from `split.rs`, and `println!("epoch: {x:?}")` from `fn_source.rs`.

4. ~~**Unused `Debug` bound.**~~ **Fixed 2026-09-21.** Reverted `fn_source.rs`'s
   `FromIteratorSource<V>` to `V: Distributable + Data` and dropped the `fmt::Debug` import.
   (`stateful.rs`'s `PartitionKey: Debug` remains load-bearing for
   `debug!("partition finished: {part:?}")` and was kept.)

5. ~~**Hot-loop tracing noise.**~~ **Resolved 2026-09-21.** Dropped the value-less
   `debug!("else branch")` in `operator.rs` and fixed a trailing-space `no_receivers ` typo.
   Kept the informative events/spans: they fire only on operator exit or once per `apply`
   (`select loop`, `logic apply`, `loop on_schedule`, `before/after input.recv`,
   `output_closed.wait_for`, `no_receivers`). They are gated at DEBUG and compiled out in
   release (`release_max_level_info`), so the overhead is bounded and they are the diagnostic
   value of the tracing changeset.

6. ~~**Deliberate-or-revert panic.**~~ **Made explicit 2026-09-21.** `stateful.rs` now handles
   the `CommUtility::send` result with an `if let Err` whose panic message states the invariant
   and names the partition: worker 0 is the discovery coordinator and is always in the build-time
   worker set, so `WorkerIdNotConnected` is an invariant violation, not a shutdown race. Failing
   loud matches the branch's
   [fail-loud direction](https://github.com/MalstromDevelopers/malstrom/blob/main/.agents/notes/proposed/architecture/2026-09-19-fail-loud-on-dangling-operator-edges.md).

7. ~~**Union test TODO.**~~ **Resolved 2026-09-21.** Replaced
   `// TODO: debug high latency before ending.` in `union.rs` with a comment recording the
   answer: the ~5 s is the coordinator's completion poll, not union latency.

8. ~~**`forward_tail_to` docs.**~~ **Obsolete.** The `forward_tail_to` / `forward_to`
   `StreamBuilder` method no longer exists; union builds its edges with `OperatorBuilder` and
   the `swap_tail` / `link_to_input` / `with_new_tail` helpers. The reuse caveat now lives on
   `StreamBuilder::swap_tail` and in
   [`stream-builder-union-refactor`](https://github.com/MalstromDevelopers/malstrom/blob/main/.agents/notes/implemented/architecture/2026-09-14-stream-builder-union-refactor.md).
   See [`union-refactor-review.md`](union-refactor-review.md).

## Structural / policy concerns

9. **`docs/notes` symlink contradicts the Agent Note rules.** `.agents/notes/README.md` states
   "there is no symlink between them \[notes and docs\]. Keep the two surfaces separate", yet a
   `docs/notes -> ../.agents/notes` symlink is staged. **Still open** — decide: drop the symlink,
   or update the rule with a note in the same change.

10. ~~**Agent-note coverage.**~~ **Resolved 2026-09-21.** The `OperatorBuilder` / `Forward` /
    union/split refactor is covered by
    [`stream-builder-union-refactor`](https://github.com/MalstromDevelopers/malstrom/blob/main/.agents/notes/implemented/architecture/2026-09-14-stream-builder-union-refactor.md),
    which was stale (it described the superseded `forward_tail_to` design) and is now moved to
    `implemented/` and corrected to the shipped `OperatorBuilder` + `forward_logic.rs`
    mechanism. Inbound links from the spsc-mpsc and fail-loud notes were repaired.

11. ~~**Possible dev-dependency cycle.**~~ **Resolved 2026-09-21.** `cargo test -p malstrom-core`
    (full run, not `--no-run`) links and passes: unit tests, the four integration binaries, and
    the 8 doc tests are all green despite the `malstrom-core` ↔ `malstrom-testkit` dev-dep pair.

12. ~~**`.gitignore`.**~~ **Resolved 2026-09-21.** The `site/` entry is removed and no `site/`
    directory exists or would be ignored; nothing untracked reappears.

13. ~~**`malstrom-macros/tests/instrument_debug.rs` symlink.**~~ **Fixed 2026-09-21.** Replaced
    the symlink into a proot container path with a real 75-line file; the 4 integration tests
    now run (`cargo test -p malstrom-macros --test instrument_debug`).

14. **`website/bun.lockb`** changed without an apparent relation to the Rust work. **Still
    open** — verify it is intentional rather than incidental.

15. ~~**`cloned_streams` example no longer compiles.**~~ **Fixed 2026-09-21 — decision: keep
    `Cloned`.** The branch deleted `malstrom-combinators/src/operators/cloned.rs` and its
    `mod.rs` re-export, breaking `cloned_streams.rs`, the `namespace.rs` facade test, the
    website joining/splitting guide, and doc-links in `split.rs`/`sink.rs`. Restored `Cloned`
    as the thin broadcast wrapper over `Split` it always was, now taking `impl Into<String>` to
    match `Split`. See the rationale in
    [`cloned.rs`](https://github.com/MalstromDevelopers/malstrom/blob/main/malstrom-combinators/src/operators/cloned.rs): it is kept for the
    ergonomic, intention-revealing `cloned(name, N)` spelling of fan-out, not for any runtime
    capability `Split` lacks.

## Suggested pre-commit sequence

1. ~~Keep the re-export fix (1); re-run `cargo test -p malstrom-testkit`.~~ Done.
2. ~~Delete commented dead code (2), debug prints (3), and the unused `Debug` bound (4).~~ Done.
3. ~~Triage hot-loop spans (5), the `expect` (6), and the union TODO (7).~~ Done.
4. ~~Finish the `split()` v1→v2 collapse~~ Done 2026-09-21 (see
   [`union-refactor-review.md`](union-refactor-review.md)).
5. **Open:** decide the `docs/notes` symlink vs. the README rule (9).
6. ~~Ensure the `OperatorBuilder` / `Forward` / union/split refactor has an Agent Note (10).~~
   Done.
7. ~~Real-file the macros test symlink (13)~~ Done; **open:** confirm `bun.lockb` (14).
8. Confirm the restored `Cloned` is the intended keep (15) — it is.
9. Run the full `cargo test` and `cargo clippy` for the touched crates, then reconcile the 2↔2
   divergence with `fork/new-scheduler-tests` (`git pull` / rebase) before committing.

## Bottom line

The tree now compiles and tests green, and blockers 1–8, 10–13 and 15 are resolved
(2026-09-21): the `malstrom-testkit` doctest break, the dead code and prints, the unused
`Debug` bound, the debug-span triage, the `stateful.rs` invariant made explicit, the union
TODO, the `split()` v1 collapse, agent-note coverage, the dev-dep cycle check, the `.gitignore`,
the macros test symlink, and the `cloned_streams` break. Two items remain before commit, both
decisions rather than fixes:

- the staged `docs/notes` symlink contradicts `.agents/notes/README.md` (9);
- `website/bun.lockb` changed with no clear relation to the Rust work (14).