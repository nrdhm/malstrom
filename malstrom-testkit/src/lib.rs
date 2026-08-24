//! Test utilities for Malstrom: an operator tester, in-memory comm backends and
//! capture persistence, used by downstream crates' unit tests.

use std::sync::Arc;
use std::{collections::HashMap, rc::Rc, sync::Mutex};

use malstrom::runtime::SingleThreadRuntime;
use malstrom::snapshot::{NoPersistence, PersistenceBackend, PersistenceClient, SnapshotBarrier, SnapshotVersion};
use malstrom::stream::Logic;
use malstrom::types::{
    Barrier, Key, Kvt, MaybeData, MaybeKey, Message, OperatorId, WorkerId,
    RescaleMessage, distributable::Distributable,
};
use malstrom::worker::StreamProvider;
use indexmap::{IndexMap, IndexSet};

pub mod communication;
pub mod operator_tester;

pub use operator_tester::{FakeCommunication, OperatorTester, SentMessage};

/// Creates a JetStream worker with no persistence and
/// a JetStream stream, which does not produce any messages
pub fn get_test_rt<F>(stream: F) -> SingleThreadRuntime<NoPersistence, F>
where
    F: FnMut(&mut dyn StreamProvider) -> (),
{
    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(stream)
}

#[derive(Default, Clone, Debug)]
/// A backend which simply captures any state it is given into a shared
/// HashMap.
/// If you have a clone of this backend you can retrieve the state using
/// the corresponding operator_id
pub struct CapturingPersistenceBackend {
    capture: Arc<Mutex<HashMap<OperatorId, Vec<u8>>>>,
}
impl PersistenceBackend for CapturingPersistenceBackend {
    type Client = CapturingPersistenceBackend;

    fn last_commited(&self) -> Option<SnapshotVersion> {
        Some(SnapshotVersion::default())
    }

    fn for_version(
        &self,
        _worker_id: WorkerId,
        _snapshot_epoch: &SnapshotVersion,
    ) -> Self::Client {
        self.clone()
    }

    fn commit_version(&self, _snapshot_version: &SnapshotVersion) {
        // nothing happening here
    }
}

impl PersistenceClient for CapturingPersistenceBackend {
    fn load(&self, operator_id: &OperatorId) -> Option<Vec<u8>> {
        self.capture.lock().unwrap().remove(operator_id)
    }

    fn persist(&mut self, state: &[u8], operator_id: &OperatorId) {
        self.capture
            .lock()
            .unwrap()
            .insert(*operator_id, state.into());
    }
}

/// A test which panics if the given operator does not forward a system message from local upstream
pub fn test_forward_system_messages<
    In: Kvt,
    Out: Kvt,
    L: Logic<In, Out>,
    R: Distributable + Send + Sync + 'static,
>(
    tester: &mut OperatorTester<In, Out, L, R>,
) where
    In::Key: Key + Default,
{
    let (cb_tx, _cb_rx) = tokio::sync::mpsc::channel(1);
    let msg = Message::AbsBarrier(Barrier::Snapshot(SnapshotBarrier::new(
        Box::new(NoPersistence),
        cb_tx,
    )));
    tester.send_local(msg);
    tester.step();
    assert!(matches!(
        tester.recv_local().unwrap(),
        Message::AbsBarrier(_)
    ));

    let (cb_tx, _cb_rx) = tokio::sync::mpsc::channel(1);
    let msg = Message::Rescale(RescaleMessage::new(IndexSet::new(), 0, cb_tx));
    tester.send_local(msg);
    tester.step();
    assert!(matches!(tester.recv_local().unwrap(), Message::Rescale(_)));
}

#[cfg(test)]
mod tests {

    use malstrom::snapshot::{deserialize_state, serialize_state};

    use super::*;

    #[test]
    fn capturing_persistence_backend() {
        let backend = CapturingPersistenceBackend::default();
        let a = backend.for_version(0, &0);
        let mut b = backend.for_version(0, &0);

        let val = "hello world".to_string();
        let ser = serialize_state(&val);
        b.persist(&ser, &42);

        let deser: String = a.load(&42).map(deserialize_state).unwrap();
        assert_eq!(deser, val);
    }
}
