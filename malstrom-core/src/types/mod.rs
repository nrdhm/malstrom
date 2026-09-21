//! Types and traits used accross JetStream
mod data;
mod key;
mod message;
mod operator_partitioner;
mod sealed;
mod time;

pub use data::{Data, MaybeData, NoData};
pub use key::{Key, MaybeKey, NoKey};
pub use message::{
    Barrier, DataMessage, Kvt, Message, ReconfigComplete, RescaleMessage, SuspendMarker,
};
pub use operator_partitioner::{OperatorId, OperatorPartitioner};
pub use sealed::sealed::Sealed;
pub use time::{MaybeTime, NoTime, OnceTime, Timestamp};
/// Uniquely identifies a worker in a JetStream cluster
pub type WorkerId = u64;
/// The [Distributable] wire-encoding trait. Public so operators and connectors
/// can bound their APIs on it.
pub mod distributable;
/// The keyed state-movement protocol messages ([Acquire]/[Collect]/[Interrogate]).
pub mod distributed;
