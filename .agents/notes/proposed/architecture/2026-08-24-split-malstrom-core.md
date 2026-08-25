# Agent Note: Split malstrom-core into layered crates

Status: proposed

## Problem

`malstrom-core` (the published `malstrom` crate) is ~103 files across 12 top-level modules and
actually bundles **three layers** in one crate: the *kernel* (execution engine), the *stdlib*
(user-facing operators/sources/sinks), and the *distributed protocol* (keyed routing). The
signs:

- **File counts skew toward library code.** `operators` (21) + `sources` (3) + `sinks` (5) +
  the local keyed operators (`key_local`, `key_distribute`, `broadcast`,
  `worker_partitioners`) outnumber the engine itself; `keyed` is 20 files, ~16 of which are the
  `keyed/distributed` routing protocol (`pub(crate) mod distributed`).
- **`sources` and `keyed` each mix library and engine.** `sources/stateful.rs`
  (`SourceCoordinator` / `SourcePartitionOp`) is the *source engine* and depends on
  `keyed::distributed` (`malstrom-core/src/sources/stateful.rs:14`); `sources/fn_source.rs` is
  a library of `SourceImpl` impls. `keyed/key_local` is a library operator while
  `keyed/distributed` is the protocol layer, and `key_distribute` reaches into the protocol
  via `distributor::DistributorBuilder`.
- **Leaf dependencies are library-only.** `expiremap` (only `operators/ttl_map.rs`), `rand`
  (only `operators/time/util.rs`), `seahash` (`keyed/distributed/remote_receiver.rs` plus one
  hash in `stream/operator.rs`) are each used by a single module, yet sit in the kernel's
  manifest; `eyre` is declared and unused.

The seams already exist for two of the three layers — `malstrom-kafka` (connector) and
`malstrom-k8s` (runtime) are separate crates — because `StreamSource`/`StreamSink` and
`RuntimeFlavor`/`OperatorOperatorComm` are public. The *operator* layer never got the same
treatment: `channels::operator_io::{Input, Output}`, `stream::{Logic, SafeLogic,
SafeLogicWrapper}`, and `stream::BuildContext` are `pub(crate)`, so nothing outside the crate
can implement or compose operators.

## Proposal

Split `malstrom-core` along the kernel / stdlib / protocol boundary, with the kernel owning a
**public, versioned operator extension API** that the other crates build on.

| Crate | Contents | Depends on |
|---|---|---|
| `malstrom` (kernel) | `types`, `channels`, `stream`, `worker`, `coordinator`, `runtime`, `snapshot` (barrier protocol + `PersistenceBackend` trait), plus the **source engine** (`SourceImpl`/`SourcePartition` traits and the coordinator→distribute→reader graph) | — |
| `malstrom-operators` | `operators`, `sinks`, `fn_source`, local keyed ops (`key_local`, `key_distribute`, `broadcast`, `worker_partitioners`) | `malstrom`, `malstrom-distributed` |
| `malstrom-distributed` | `keyed/distributed` (routers, `remote_receiver`/`remote_sender`, `wire_message`, `Acquire`/`Collect`/`Interrogate`) | `malstrom` |
| `malstrom-snapshot-slatedb` | the SlateDB/object-store persistence backend | `malstrom` |
| `malstrom-testkit` | `testing` (operator_tester, `VecSink`, in-memory comm backends) | `malstrom` |
| `malstrom-kafka`, `malstrom-k8s`, `malstrom-macros` | (existing) | as today |

### The prerequisite: a public extension API

Extraction is downstream of one decision: **make the operator extension surface public.** The
crate split is trivial to *do* and pointless to do *first* — operators currently reach into
`pub(crate)` internals, so moving them out without an API just relocates the coupling. The
kernel must expose, as a stable (or `#[doc(hidden)]` `malstrom::__private`) surface:

- `channels::operator_io::{Input, Output}` and `link`/partitioners
- `stream::{Logic, SafeLogic, SafeLogicWrapper, BuildContext, OperatorContext}`
- the `Message`/`DataMessage`/`Kvt` types the operators already consume

This is the same reason `malstrom-kafka`/`malstrom-k8s` are already external: their seams
(`StreamSource`, `StreamSink`, `RuntimeFlavor`, `OperatorOperatorComm`) are public. The
operator seam is simply the missing one.

## Seams to cut first

Two cross-layer dependencies today would create a cycle once layers are crates; both must be
inverted before (or as part of) the split:

1. **`sources/stateful.rs` → `keyed::distributed`.** The source engine calls
   `.distribute(rendezvous_select)` and imports `distributed::{Acquire, Collect,
   Interrogate}`. The engine should take the distributor as a parameter (or via the extension
   API) rather than naming the distributed crate.
2. **`keyed/key_distribute.rs` → `distributor::DistributorBuilder`.** The `.distribute()`
   operator method is a library surface that reaches into the protocol layer; it must go
   through the seam (a `Distribute` extension point owned by the kernel or distributed crate)
   so `malstrom-operators` → `malstrom-distributed` stays acyclic.

## Phased order

1. **`malstrom-testkit`** — extract `testing` first: cheap, and it forces the first
   extension surface (`Operator`/`Input`/`Output` for the tester) into the open as a warm-up.
2. **`malstrom-operators`** — the big payoff: 30+ files and the leaf deps (`expiremap`,
   `rand`) leave the kernel manifest. Ships only after the extension API is public.
3. **`malstrom-distributed`** — isolate the least-stable, most complex WIP protocol; the
   kernel then compiles/tests without the router machinery. Blocked on the two seams above.
4. **`malstrom-snapshot-slatedb`** — move the heavy optional deps (`slatedb`, `object_store`,
   `tokio-stream`) into a connector crate, completing the "every connector is its own crate"
   story.

## Alternatives considered

- **Status quo (keep the monolith)** — zero churn and no extension-API contract to maintain.
  Lost: kernel builds without library/protocol deps, no isolation of the WIP distributed
  layer, and no forcing function to stabilize the operator API.
- **Extract without stabilizing the API** — move the files but keep reaching into
  `pub(crate)` internals via `#[path]` hacks or a `__private` grab-bag without a design. This
  is rework bait: the crates would be cosmetic and the API would still need defining later.
  Rejected; the API is the deliverable, the crate boundary is just where it lives.
- **Extract `malstrom-distributed` before `malstrom-operators`** — tempting because the
  protocol layer is the messiest, but it is the most entangled (the source engine and
  `key_distribute` both reach into it) and would force the two seams to be cut while the
  protocol is still moving. Do the cheap, clean extraction first.
- **One `malstrom-stdlib` crate (operators + distributed together)** — fewer crates, but it
  keeps the library and the protocol coupled and defeats the WIP-isolation goal. Rejected;
  keep the protocol boundary explicit.
- **Rust "workspace submodules" instead of crates** — Rust has no submodule isolation; real
  dependency isolation requires real crates. A single crate with feature-gated modules
  (`operators`, `distributed` as optional features) is a lighter-weight middle ground if full
  crates are too much churn (see Risks).

## Acceptance criteria

- `malstrom` (kernel) compiles and its unit/doctests pass **without** the extracted crates'
  dependencies (e.g. no `expiremap`/`rand`/`slatedb` in the kernel manifest).
- The operator extension API is public and versioned; downstream crates implement operators
  against it without `pub(crate)` reach-through.
- The crate graph is acyclic with the direction above (`malstrom` is depended on, never
  depends on its own layers).
- `cargo check --workspace`, `cargo test -p malstrom`, and the examples behave identically to
  before the split.

## Risks

- **The extension API becomes a versioned contract.** Every operator-facing type moved to
  `pub` is now semver surface; breaking it is a breaking change. This is a real maintenance
  cost, not just a rename.
- **Cycle risk.** If `malstrom-operators` and `malstrom-distributed` both need each other
  (keyed operators vs the distribute step), the split needs an intermediate seam; get the
  dependency direction wrong and the crates won't link.
- **The source engine is the hard part.** `sources/stateful.rs` is engine code entangled with
  the distributed layer; cutting seam 1 correctly (without regressing rescale/completion) is
  the riskiest piece of the whole proposal.
- **Extraction churn.** Moving `pub(crate)` types to `pub` and re-exporting them across crate
  boundaries is mechanical but broad; it should be one crate at a time to keep each step green.
