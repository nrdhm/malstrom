# Agent Note: Share the k8s gRPC protos via a `malstrom-k8s-proto` crate

Status: implemented

## Problem

`malstrom-k8s/runtime` and `malstrom-k8s/operator` both compiled the same protos
(`exchange.proto`, `k8s_operator_api.proto`) into their own tonic stubs, and the operator
did it by a **hard-coded path reach across the crate boundary**:

```rust
// malstrom-k8s/operator/build.rs
PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()                          // malstrom-k8s
    .join("runtime").join("proto")     // ← reaches into the RUNTIME crate's directory
    .join("k8s_operator_api.proto");
assert!(proto.exists());               // panics (build exit 101) if the layout changes
tonic_build::compile_protos(proto)?;
```

Problems: (1) the operator's build depended on files owned by the runtime crate with no
Cargo edge — renaming/moving/restructuring the runtime breaks the operator's build, and
`cargo package` of the operator would fail (the proto file is not shipped with the package);
(2) both crates independently generated stubs from the same proto, so the shared wire
contract existed as two copies that must stay in sync; (3) the failure surfaced in CI as the
same exit-101 class as the missing-`protoc` error, complicating diagnosis.

## Decision

Extract a shared **`malstrom-k8s-proto`** crate (`malstrom-k8s/proto/`) that owns the protos
and generates the tonic types **once**; the runtime and operator depend on it and import the
same types, so the client and server sides of each wire contract are identical by
construction:

- `malstrom-k8s/proto/proto/{exchange,k8s_operator_api}.proto` — moved from
  `malstrom-k8s/runtime/proto/`.
- `malstrom-k8s/proto/build.rs` — `tonic_build::compile_protos` for both protos (the only
  crate that needs `protoc`).
- `malstrom-k8s/proto/src/lib.rs` — `pub mod exchange` + `pub mod k8s_operator` wrapping the
  generated code (`#[allow(missing_docs)]` — docs live in the protos).
- `malstrom-k8s/runtime` — build.rs removed; drops `tonic-build`/`prost`; imports the types
  via `use malstrom_k8s_proto::{exchange, k8s_operator};` in `communication/mod.rs` (the
  existing `super::exchange`/`super::k8s_operator` paths keep resolving).
- `malstrom-k8s/operator` — build.rs reduced to the CRD-YAML generation; drops
  `tonic-build`/`prost`; `coordinator_api/mod.rs` re-exports the client and `RescaleRequest`
  from `malstrom_k8s_proto::k8s_operator` instead of `tonic::include_proto!`.
- CI: the gate installs `protobuf-compiler` (the one crate that still compiles protos is
  `malstrom-k8s-proto`); no crates are excluded.

## Alternatives considered

- **Copy the proto into the operator crate** — removes the path reach but keeps two
  generated copies that must stay in sync. Rejected in favor of one generated source of
  truth.
- **Operator depends on the runtime crate and reuses its generated types** — one codegen,
  but the in-cluster operator would pull the entire runtime crate (tonic/prost/comm
  traits/malstrom) for one RPC, and the runtime's generated modules would need exposing.
  Rejected.
- **Status quo** — the path reach and duplicate codegen stay; the operator's `cargo package`
  remains broken and the layout is brittle. Rejected.

## Consequences

- **Single source of truth** for the gRPC wire contracts; the runtime (server) and operator
  (client) types cannot drift apart.
- **No cross-crate path coupling** — each crate's build is self-contained; `cargo package` of
  the operator no longer depends on the runtime's directory.
- **One crate needs `protoc`** (`malstrom-k8s-proto`); CI installs it and gates the whole
  workspace (no exclusions).
- **Verification** — `cargo check --workspace` clean of errors; runtime tests (5) pass;
  `cargo clippy --workspace --all-targets -- -D clippy::correctness` passes with 0 errors.
