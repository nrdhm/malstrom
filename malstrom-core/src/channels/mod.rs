//! Channels for exchanging data between stream operators
/// Barrier alignment machinery used by the distributed crate and operator IO.
pub mod alignment;
pub mod operator_io;
/// Low-level receiver abstraction used by operator IO and the distributed crate.
pub mod recv_trait;
pub(crate) mod signal;
/// The bounded/unbounded SPSC channels underlying operator edges.
pub mod spsc;
