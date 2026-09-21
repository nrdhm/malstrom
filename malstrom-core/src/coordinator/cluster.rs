use futures::future::join_all;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use crate::{
    coordinator::{COORDINATOR_ID, messages::*},
    runtime::communication::{CoordinatorClient, WorkerCoordinatorComm},
    snapshot::{PersistenceBackend, PersistenceClient, SnapshotVersion, deserialize_state},
    types::WorkerId,
};

/// Current state of a worker
#[derive(Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub(super) struct WorkerState {
    /// What it's doing
    pub phase: WorkerPhase,
    /// Last snapshot the worker has completed or None if not yet any
    pub snapshot_version: Option<SnapshotVersion>,
}

pub(crate) struct ClusterHandle {
    /// Contains state of all known workers, active or not
    pub(super) workers: IndexMap<WorkerId, (WorkerState, CoordinatorClient)>,
    /// Last reconfiguration the cluster has completed
    /// or None if no reconfiguration yet completed
    pub(super) config_version: Option<u64>,
    /// Last snapshot the coordinator has completed or None if not yet any
    pub snapshot_version: Option<SnapshotVersion>,
}

impl ClusterHandle {
    /// Create a serializable version of the state by cloning.
    /// The serializable version, once created, is completely decoupled from the [CoordinatorState]
    /// i.e. updates are not reflected
    // pub(crate) fn get_serializable(&self) -> SerializableClusterHandle {
    //     let worker_states = self
    //         .workers
    //         .iter()
    //         .map(|(id, (state, _))| (id, state))
    //         .cloned()
    //         .collect();
    //     SerializableClusterHandle {
    //         worker_states,
    //         config_version: self.config_version,
    //         snapshot_version: self.snapshot_version,
    //     }
    // }

    /// Start execution graph build on the given workers
    /// Completes when all of them have finished building
    pub async fn start_build(&self, targets: &[WorkerId]) -> () {
        let build_info = BuildInformation {
            worker_set: self.workers.keys().map(|x| *x).collect(),
            resume_snapshot: self.snapshot_version,
            config_version: self.config_version.unwrap_or_default(),
        };
        let msg = StartBuild(build_info);
        let responses = self
            .workers
            .iter()
            .filter(|(wid, _)| targets.contains(wid))
            .map(|(_, (_, client))| client.send::<_, ()>(msg.clone()));
        join_all(responses).await;
    }

    /// Start job execution on the given workers
    pub async fn start_execution(&self, targets: &[WorkerId]) -> () {
        let msg = StartExecution;
        let responses = self
            .workers
            .iter()
            .filter(|(wid, _)| targets.contains(wid))
            .map(|(_, (_, client))| client.send::<_, ()>(msg.clone()));
        join_all(responses).await;
    }

    /// check if all workers have completed execution
    pub async fn check_execution_complete(&self) -> bool {
        let msg = RuntimeMessage::ExecutionComplete;
        let responses = self
            .workers
            .values()
            .map(|(_, client)| client.send(msg.clone()));
        join_all(responses).await.into_iter().all(|x| x)
    }

    /// Suspend execution on all workers.
    /// NOTE: currently a stub — the suspend feature is unimplemented
    /// (see ApiRequestOperation::Suspend).
    pub async fn suspend(&self) {
        tracing::warn!("Coordinator suspend is not yet implemented");
    }

    pub async fn take_snapshot(&self, version: SnapshotVersion) {
        let msg = RuntimeMessage::Snapshot(version);
        let responses = self
            .workers
            .values()
            .map(|(_, client)| client.send::<_, bool>(msg.clone()));
        join_all(responses).await.into_iter();
    }

    pub async fn reconfigure<C>(
        &mut self,
        new_set: IndexSet<WorkerId>,
        comm: &C,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        C: Sync + WorkerCoordinatorComm,
    {
        let mut new_workers = Vec::new();
        for wid in new_set.iter() {
            if !self.workers.contains_key(wid) {
                self.add_worker(*wid, comm).await?;
                new_workers.push(*wid);
            }
        }
        // Bootstrap only the newly-added workers. Existing workers keep running and
        // learn about the new scale from `RuntimeMessage::Reconfigure` below — their
        // coordination tasks only decode `RuntimeMessage`, never the startup protocol,
        // so sending them `StartBuild`/`StartExecution` would be mis-decoded.
        if !new_workers.is_empty() {
            self.start_build(&new_workers).await;
            self.start_execution(&new_workers).await;
        }
        let next_version = self.config_version.map(|x| x + 1).unwrap_or_default();
        let msg = RuntimeMessage::Reconfigure((new_set.clone(), next_version));
        let responses = self
            .workers
            .iter()
            .map(|(_wid, (_, client))| client.send::<_, bool>(msg.clone()));
        join_all(responses).await;
        self.workers.retain(|wid, _| new_set.contains(wid));
        self.config_version = Some(next_version);
        Ok(())
    }

    /// Add, build and start a new worker
    async fn add_worker<C>(
        &mut self,
        id: WorkerId,
        comm: &C,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        C: Sync + WorkerCoordinatorComm,
    {
        let client = CoordinatorClient::new(id, comm).await?;
        self.workers.insert(id, (WorkerState::default(), client));
        Ok(())
    }
}

/// A serializable version of the [CoordinatorState]
#[derive(Serialize, Deserialize)]
pub(crate) struct SerializableClusterHandle {
    pub(super) worker_states: IndexMap<WorkerId, WorkerState>,
    pub(super) config_version: Option<u64>,
    /// Last snapshot the coordinator has completed or None if not yet any
    pub snapshot_version: Option<SnapshotVersion>,
}

impl SerializableClusterHandle {
    fn new(scale: u64) -> Self {
        let worker_states = (0..scale).map(|id| (id, WorkerState::default())).collect();
        Self {
            worker_states,
            config_version: None,
            snapshot_version: None,
        }
    }

    /// Load state from its serializable representation
    pub(crate) async fn setup_communication<C>(
        self,
        comm: &C,
    ) -> Result<ClusterHandle, Box<dyn std::error::Error + Send + Sync>>
    where
        C: Sync + WorkerCoordinatorComm,
    {
        let worker_states = IndexMap::from(self.worker_states);
        let workers = IndexMap::with_capacity(worker_states.len());
        let mut cluster = ClusterHandle {
            workers,
            config_version: self.config_version,
            snapshot_version: self.snapshot_version,
        };
        for (id, state) in worker_states.into_iter() {
            let client = CoordinatorClient::new(id, comm).await?;
            cluster.workers.insert(id, (state, client));
        }
        Ok(cluster)
    }
}

impl From<&ClusterHandle> for SerializableClusterHandle {
    fn from(value: &ClusterHandle) -> Self {
        let worker_states = value
            .workers
            .iter()
            .map(|(wid, x)| (*wid, x.0.clone()))
            .collect();
        Self {
            worker_states,
            config_version: value.config_version,
            snapshot_version: value.snapshot_version,
        }
    }
}

/// What the worker is currently doing
#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
pub(super) enum WorkerPhase {
    /// Not yet reported
    Unknown,
    /// Build completed, but execution not yet started
    BuildComplete,
    /// Execution started and running
    Running,
    /// Performing a snapshot
    Snapshotting,
    /// Performing a reconfiguration
    Reconfiguring,
    /// Suspend (not currently running)
    Suspended,
    /// Execution completed
    Completed,
}
impl Default for WorkerPhase {
    fn default() -> Self {
        Self::Unknown
    }
}

pub(super) fn load_or_create_cluster_handle<P>(
    backend: P,
    default_scale: u64,
) -> SerializableClusterHandle
where
    P: PersistenceBackend,
{
    let persisted = backend.last_commited();
    match persisted {
        Some(version) => {
            let persistence_client = backend.for_version(COORDINATOR_ID, &version);
            persistence_client
                .load(&0)
                .map(deserialize_state)
                .unwrap_or_else(|| SerializableClusterHandle::new(default_scale))
        }
        None => SerializableClusterHandle::new(default_scale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::communication::{ReqResReceiver, ReqResSender};
    use crate::types::distributable::Distributable;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    type ReqRes = (Vec<u8>, tokio::sync::oneshot::Sender<Vec<u8>>);

    /// Coordinator-side mock `WorkerCoordinatorComm`: `coordinator_to_worker` creates a
    /// fresh channel per worker and records it so the test can act as that worker.
    #[derive(Clone, Default)]
    struct MockComm {
        channels: Arc<Mutex<HashMap<WorkerId, (flume::Sender<ReqRes>, flume::Receiver<ReqRes>)>>>,
    }

    impl MockComm {
        fn take_receiver(&self, worker: WorkerId) -> flume::Receiver<ReqRes> {
            loop {
                if let Some((_tx, rx)) = self.channels.lock().unwrap().remove(&worker) {
                    return rx;
                }
                std::thread::yield_now();
            }
        }
    }

    struct MockSender {
        tx: flume::Sender<ReqRes>,
    }

    #[async_trait]
    impl ReqResSender for MockSender {
        async fn send(&self, msg: Vec<u8>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            self.tx.send((msg, resp_tx)).map_err(|e| e.to_string())?;
            Ok(resp_rx.await.map_err(|e| e.to_string())?)
        }
    }

    #[async_trait]
    impl WorkerCoordinatorComm for MockComm {
        async fn worker_to_coordinator(
            &self,
        ) -> Result<Box<dyn ReqResReceiver>, Box<dyn std::error::Error + Send + Sync>> {
            unreachable!("unit test only drives the coordinator side")
        }

        async fn coordinator_to_worker(
            &self,
            to_worker: WorkerId,
        ) -> Result<Box<dyn ReqResSender>, Box<dyn std::error::Error + Send + Sync>> {
            let (tx, rx) = flume::unbounded();
            self.channels
                .lock()
                .unwrap()
                .insert(to_worker, (tx.clone(), rx));
            Ok(Box::new(MockSender { tx }))
        }
    }

    /// Acts as a worker: consumes the startup protocol (`StartBuild`, `StartExecution`),
    /// then `RuntimeMessage`s until a `Reconfigure`, recording every message kind.
    async fn fake_worker(
        worker_id: WorkerId,
        rx: flume::Receiver<ReqRes>,
        log: Arc<Mutex<Vec<(WorkerId, &'static str)>>>,
    ) {
        fn respond(resp: Vec<u8>, resp_tx: tokio::sync::oneshot::Sender<Vec<u8>>) {
            let _ = resp_tx.send(resp);
        }
        let (b1, r1) = rx.recv_async().await.expect("worker startup message");
        let _ = StartBuild::decode(&b1);
        log.lock().unwrap().push((worker_id, "StartBuild"));
        respond(().encode(), r1);
        let (b2, r2) = rx.recv_async().await.expect("worker startup message");
        let _ = StartExecution::decode(&b2);
        log.lock().unwrap().push((worker_id, "StartExecution"));
        respond(().encode(), r2);
        loop {
            let (bytes, resp_tx) = rx.recv_async().await.expect("worker runtime message");
            // decoding a `StartBuild` here (a tuple struct) as the `RuntimeMessage`
            // enum panics — which is exactly the regression this test guards against
            match RuntimeMessage::decode(&bytes) {
                RuntimeMessage::Reconfigure(_) => {
                    log.lock().unwrap().push((worker_id, "Reconfigure"));
                    respond(true.encode(), resp_tx);
                    return;
                }
                RuntimeMessage::Snapshot(_) => {
                    log.lock().unwrap().push((worker_id, "Snapshot"));
                    respond(true.encode(), resp_tx);
                }
                RuntimeMessage::ExecutionComplete => {
                    log.lock().unwrap().push((worker_id, "ExecutionComplete"));
                    respond(false.encode(), resp_tx);
                }
            }
        }
    }

    /// Regression: a rescale must send the startup protocol only to the *newly added*
    /// worker. Existing workers must see only `RuntimeMessage::Reconfigure` — before
    /// the fix they were sent `StartBuild` again, which their coordination tasks
    /// mis-decoded as the enum and panicked.
    #[tokio::test]
    async fn reconfigure_bootstraps_only_new_workers() {
        let comm = MockComm::default();
        let mut state = SerializableClusterHandle::new(1)
            .setup_communication(&comm)
            .await
            .unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));

        // worker 0: initial startup protocol, then runtime messages
        let comm_w0 = comm.clone();
        let log_w0 = Arc::clone(&log);
        let w0 =
            tokio::spawn(async move { fake_worker(0, comm_w0.take_receiver(0), log_w0).await });
        state.start_build(&[0]).await;
        state.start_execution(&[0]).await;

        // rescale 1 -> 2: worker 1's channel materializes inside `reconfigure`
        let comm_w1 = comm.clone();
        let log_w1 = Arc::clone(&log);
        let w1 =
            tokio::spawn(async move { fake_worker(1, comm_w1.take_receiver(1), log_w1).await });
        state
            .reconfigure(IndexSet::from([0, 1]), &comm)
            .await
            .unwrap();

        w0.await.unwrap();
        w1.await.unwrap();

        let log = log.lock().unwrap().clone();
        let kinds = |wid| {
            log.iter()
                .filter(|(w, _)| *w == wid)
                .map(|(_, m)| *m)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            kinds(0),
            vec!["StartBuild", "StartExecution", "Reconfigure"]
        );
        assert_eq!(
            kinds(1),
            vec!["StartBuild", "StartExecution", "Reconfigure"]
        );
        assert_eq!(state.workers.len(), 2);
        assert_eq!(state.config_version, Some(0));
    }
}
