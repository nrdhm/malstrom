//! Malstrom is a framework for building distributed, stateful stream processes.
//!
//! This is the **facade** crate: it re-exports the layer crates under the
//! historical single-crate module tree, so a program imports `malstrom::…` and
//! never has to know which layer crate owns which module. The facade is a thin
//! re-export with no logic of its own; the layers stay first-class crates.
//!
//! - `malstrom_core` (kernel): `channels`, `coordinator`, `runtime`, `snapshot`,
//!   `stream`, `types`, `worker`
//! - `malstrom_operators`: `operators`, `sinks`, `sources`, `keyed` (which
//!   re-exports `malstrom_distributed` at `keyed::distributed`)
//! - `malstrom_snapshot_slatedb` (feature `slatedb`): the SlateDB backend at
//!   `malstrom::slatedb`

pub use malstrom_core::{channels, coordinator, runtime, snapshot, stream, types, worker};

#[cfg(feature = "operators")]
pub use malstrom_operators::{keyed, operators, sinks, sources};

/// The SlateDB/object-store snapshot backend (feature `slatedb`).
///
/// Note: the historical `malstrom::snapshot::slatedb` path is gone — `snapshot`
/// is a kernel module and cannot host the connector. The backend lives here
/// instead.
#[cfg(feature = "slatedb")]
pub mod slatedb {
    pub use malstrom_snapshot_slatedb::*;
}
