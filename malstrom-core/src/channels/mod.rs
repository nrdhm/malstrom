//! Channels for exchanging data between stream operators
pub mod operator_io;
pub(crate) mod signal;

// Edge primitives live in `malstrom-core-internal` (a lower layer the published
// `malstrom` facade never re-exports); re-exported crate-internally so the kernel can
// keep using them without exposing them to users.
pub(crate) use malstrom_core_internal::{alignment, recv_trait, spsc};
