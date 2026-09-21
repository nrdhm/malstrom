//! Internal edge primitives for the Malstrom kernel.
//!
//! This crate holds the same-worker operator-edge implementation that the kernel and the
//! sibling layer crates need, but that is not part of the public extension API: the SPSC
//! edge channel ([`spsc`]), the receiver abstraction it and the alignment combinator share
//! ([`recv_trait`]), and the barrier-alignment combinator ([`alignment`]).
//!
//! It is a **lower layer**: it does not depend on `malstrom-core`, and the published
//! `malstrom` facade never re-exports it, so these items are not reachable by users. See
//! `.agents/notes/proposed/architecture/2026-09-21-malstrom-core-internal-crate.md`.

pub mod alignment;
pub mod recv_trait;
pub mod spsc;
