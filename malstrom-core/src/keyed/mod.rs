//! Keyed streams for logical and physical partitioning of data
mod key_local;
pub use key_local::KeyLocal;
mod key_distribute;
pub(crate) use key_distribute::Distribute;
pub use key_distribute::KeyDistribute;
mod broadcast;
pub use broadcast::WorkerBroadcast;
mod worker_partitioners;
pub use worker_partitioners::{WorkerPartitioner, index_select, rendezvous_select};

/// The distributed routing protocol: keyed state movement
/// (`Acquire`/`Collect`/`Interrogate`) and the distributor/router machinery for
/// cross-worker key placement.
pub mod distributed;
