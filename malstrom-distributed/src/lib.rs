//! The distributed routing protocol for Malstrom: keyed state movement and the
//! distributor/router machinery for cross-worker key placement. The protocol
//! message types ([Acquire]/[Collect]/[Interrogate]) live in the kernel
//! (`malstrom::types::distributed`); this crate implements the machinery that
//! moves them between workers.

pub mod distributor;
pub use distributor::DistributorBuilder;

mod remote_receiver;
mod remote_sender;

mod routers;
mod targeted_message;
mod versioned_message;
mod wire_message;

mod worker_partitioners;
pub use worker_partitioners::{WorkerPartitioner, index_select, rendezvous_select};

pub use malstrom::types::distributed::{Acquire, Collect, Interrogate};

/// Version of the current cluster configuration.
/// TODO: move to global crate scope
type ConfigVersion = u64;
