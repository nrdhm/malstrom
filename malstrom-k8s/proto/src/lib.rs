//! Shared gRPC types for the Malstrom k8s runtime and operator, generated once from
//! the protos in [`proto/`](proto). Both crates import these types, so the client and
//! server sides of each wire contract are identical by construction (previously the
//! operator's build.rs reached into the runtime's `proto/` directory by path and both
//! crates generated the types independently).

/// The intra-job coordinator/worker gRPC API (`exchange.proto`).
///
/// Generated code — docs live in the protos.
#[allow(missing_docs)]
pub mod exchange {
    include!(concat!(env!("OUT_DIR"), "/malstrom_k8s.rs"));
}

/// The coordinator ↔ k8s-operator gRPC API (`k8s_operator_api.proto`).
///
/// Generated code — docs live in the protos.
#[allow(missing_docs)]
pub mod k8s_operator {
    include!(concat!(env!("OUT_DIR"), "/malstrom_k8s.k8s_operator.rs"));
}
