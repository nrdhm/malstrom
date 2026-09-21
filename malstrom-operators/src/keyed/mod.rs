//! Keyed streams for logical and physical partitioning of data
mod key_local;
pub use key_local::KeyLocal;
mod key_distribute;
pub(crate) use key_distribute::Distribute;
pub use key_distribute::KeyDistribute;
mod broadcast;
pub use broadcast::WorkerBroadcast;

pub use malstrom_distributed::{WorkerPartitioner, index_select, rendezvous_select};

/// The distributed routing protocol, re-exported at the historical
/// `keyed::distributed` path for continuity.
pub mod distributed;
