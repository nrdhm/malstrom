pub(crate) mod distributor;

mod remote_receiver;
mod remote_sender;

mod routers;
mod targeted_message;
mod versioned_message;
mod wire_message;

pub use crate::types::distributed::{Acquire, Collect, Interrogate};

/// Version of the current cluster configuration.
/// TODO: move to global crate scope
type ConfigVersion = u64;
