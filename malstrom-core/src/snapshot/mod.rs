//! Snapshots are periodically saved state from stateful operations. Regular snapshots allow
//! resuming computation after failures. Snapshots can also be utilized to enable statful job
//! upgrades

use crate::types::{OperatorId, WorkerId};
use futures::{FutureExt, SinkExt};
use serde::{Serialize, de::DeserializeOwned};
use std::{cell::RefCell, fmt::Debug, rc::Rc, sync::Mutex, task::Waker};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

/// Version of a snapshot
pub type SnapshotVersion = u64;

/// Serialize state with the framework's snapshot encoding (MessagePack).
pub fn serialize_state<S: Serialize>(state: &S) -> Vec<u8> {
    rmp_serde::to_vec(state).expect("Error serializing state")
}

/// Deserialize state with the framework's snapshot encoding (MessagePack).
pub fn deserialize_state<S: DeserializeOwned>(state: Vec<u8>) -> S {
    rmp_serde::from_slice(&state).expect("Error deserializing state")
}

/// A persistence backend provides persistent storage for storing snapshots across job restarts.
/// This may be on a local disk, remote storage, a database or anything really which can reliably
/// store data
pub trait PersistenceBackend: Send + Sync + 'static {
    /// Client for this backend. The client is used to store and load state from the backend.
    type Client: PersistenceClient;
    /// Return the version of the last committed snapshot or `None` if no version has not been
    /// committed yet.
    fn last_commited(&self) -> Option<SnapshotVersion>;
    /// Create a client for a loading/saving state for a specific snapshot version
    fn for_version(&self, worker_id: WorkerId, snapshot_version: &SnapshotVersion) -> Self::Client;
    /// mark a specific snapshot version as finished
    fn commit_version(&self, snapshot_version: &SnapshotVersion);
}

/// A client for saving snapshot data to and loading that data from a persistent storage
pub trait PersistenceClient: Send + 'static {
    /// Load the state for the given operator, returning `None` if no state exists for this
    /// operator in persistent storage
    fn load(&self, operator_id: &OperatorId) -> Option<Vec<u8>>;
    /// Retain the given state for the given operator.
    fn persist(&mut self, state: &[u8], operator_id: &OperatorId);
}

/// A snapshotting barrier for use with the
/// [ABS snapshotting algorithm](https://arxiv.org/abs/1506.08603)
pub struct SnapshotBarrier {
    backend: Rc<RefCell<Box<dyn PersistenceClient>>>,
    /// sends when the last barrier is dropped
    callback: Rc<RefCell<mpsc::Sender<()>>>,
}
impl Clone for SnapshotBarrier {
    fn clone(&self) -> Self {
        Self {
            backend: Rc::clone(&self.backend),
            callback: Rc::clone(&self.callback),
        }
    }
}
impl Debug for SnapshotBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Barrier").finish()
    }
}

impl SnapshotBarrier {
    /// Create a snapshot barrier over the given persistence client. The callback
    /// is signalled when the last clone of the barrier is dropped.
    pub fn new(backend: Box<dyn PersistenceClient>, callback: mpsc::Sender<()>) -> Self {
        Self {
            backend: Rc::new(RefCell::new(backend)),
            callback: Rc::new(RefCell::new(callback)),
        }
    }

    /// Persist the given state for the given operator.
    pub fn persist<S: Serialize + DeserializeOwned>(
        &mut self,
        state: &S,
        operator_id: &OperatorId,
    ) {
        let encoded = serialize_state(state);
        self.backend.borrow_mut().persist(&encoded, operator_id)
    }
}

impl Drop for SnapshotBarrier {
    fn drop(&mut self) {
        // kinda ugly, but works
        if Rc::strong_count(&self.callback) == 1 {
            self.callback.borrow_mut().send(()).now_or_never().unwrap();
        }
    }
}

/// A persistence backend which does not retain any data. This is mostly useful for testing or
/// situations where you always want to restart the job statelessly
#[derive(Clone, Debug)]
pub struct NoPersistence;
impl PersistenceBackend for NoPersistence {
    type Client = NoPersistence;
    fn last_commited(&self) -> Option<SnapshotVersion> {
        None
    }

    fn for_version(&self, _worker_id: WorkerId, _snapshot_version: &SnapshotVersion) -> Self {
        NoPersistence {}
    }

    fn commit_version(&self, _snapshot_version: &SnapshotVersion) {}
}

impl PersistenceClient for NoPersistence {
    fn load(&self, _operator_id: &OperatorId) -> Option<Vec<u8>> {
        None
    }

    fn persist(&mut self, _state: &[u8], _operator_id: &OperatorId) {}

    // fn commit(&mut self, _snapshot_epoch: &SnapshotVersion) -> () {
    //     ()
    // }

    // fn get_last_committed(&self) -> Option<SnapshotVersion> {
    //     None
    // }
}

#[cfg(test)]
mod test {
    use super::PersistenceClient;

    /// This test won't compile if PersistenceBackend is not object safe
    #[test]
    fn is_object_safe() {
        struct _Foo {
            _bar: Box<dyn PersistenceClient>,
        }
    }
}

#[cfg(test)]
mod serialization_tests {
    use super::{deserialize_state, serialize_state};

    /// The coordinator's cluster-state serialization must round-trip.
    #[test]
    fn serialize_state_round_trips() {
        let state = vec![(1u64, "one".to_string()), (2, "two".to_string())];
        let bytes = serialize_state(&state);
        assert_eq!(deserialize_state::<Vec<(u64, String)>>(bytes), state);
    }

    #[test]
    fn serialize_state_round_trips_primitives() {
        let bytes = serialize_state(&7u64);
        assert_eq!(deserialize_state::<u64>(bytes), 7);
    }
}
