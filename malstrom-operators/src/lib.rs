//! User-facing operators, sinks and sources for Malstrom jobs, built on the
//! kernel's public operator extension API.

// Operator-layer edge-construction helpers (previously in the kernel, where they leaked into
// the public surface). Not part of the user API.
pub(crate) mod forward_logic;
pub(crate) mod operator_builder;

pub mod keyed;
pub mod operators;
pub mod sinks;
pub mod sources;
