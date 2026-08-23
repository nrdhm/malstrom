use std::time::Duration;

use crate::{
    coordinator::{Coordinator, CoordinatorExecutionError},
    runtime::{
        OperatorOperatorComm, RuntimeFlavor,
        communication::{
            ReqResReceiver, ReqResSender, StreamReceiver, StreamSender, WorkerCoordinatorComm,
        },
        threaded::communication::{
            CoordinatorChannels, CoordinatorCommunication, OperatorChannels, OperatorCommunication,
        },
    },
    snapshot::PersistenceBackend,
    types::{OperatorId, WorkerId},
    worker::{StreamProvider, WorkerBuilder, WorkerExecutionError},
};

use async_trait::async_trait;
use bon::Builder;
use thiserror::Error;

/// Runs all dataflows in a single thread on a
/// single machine with no parrallelism.
#[derive(Builder)]
pub struct SingleThreadRuntime<P, F> {
    #[builder(finish_fn)]
    build: F,
    persistence: P,
    snapshots: Option<Duration>,
}

impl<P, F> SingleThreadRuntime<P, F>
where
    P: PersistenceBackend + Clone + Send,
    F: FnOnce(&mut dyn StreamProvider),
{
    /// Start execution on this runtime, returning a build error if building the
    /// JetStream worker fails
    pub fn execute(self) -> Result<(), ExecutionError> {
        let mut flavor = SingleThreadRuntimeFlavor::default();

        let mut worker = WorkerBuilder::new(flavor.clone(), self.persistence.clone());
        (self.build)(&mut worker);

        let (coordinator, _) = Coordinator::new();
        let communication = flavor
            .communication()
            .expect("SingleThread communication is infallible");
        let _coord_thread = std::thread::spawn(move || {
            coordinator.execute(1, self.snapshots, self.persistence, communication)
        });
        worker.execute()?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Error executing worker")]
    Worker(#[from] WorkerExecutionError),
    #[error("Error executing coordinator")]
    Coordinator(#[from] CoordinatorExecutionError),
    #[error("Error joining coordinator thread: {0:?}")]
    CoordinatorJoin(Box<dyn std::any::Any + std::marker::Send>),
}

/// Runtime which only provides a single thread for a single worker.
/// This runtime is usually not very performant, but very simple.
/// Useful for unit-tests.
#[derive(Debug, Default, Clone)]
pub struct SingleThreadRuntimeFlavor {
    operator_channels: OperatorChannels,
    coordinator_channels: CoordinatorChannels,
}

impl RuntimeFlavor for SingleThreadRuntimeFlavor {
    type Communication = InterThreadCommunication;

    fn communication(
        &mut self,
    ) -> Result<Self::Communication, Box<dyn std::error::Error + Send + Sync>> {
        Ok(InterThreadCommunication {
            operator: OperatorCommunication::new(self.operator_channels.clone(), 0),
            coordinator: CoordinatorCommunication::new(self.coordinator_channels.clone(), 0),
        })
    }

    fn this_worker_id(&self) -> u64 {
        0
    }
}

/// In-process communication for the single-thread runtime.
/// Delegates to the shared inter-thread channel infrastructure
/// ([crate::runtime::threaded::communication]).
pub struct InterThreadCommunication {
    operator: OperatorCommunication,
    coordinator: CoordinatorCommunication,
}

#[async_trait]
impl OperatorOperatorComm for InterThreadCommunication {
    async fn new_sender(
        &self,
        to_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamSender>, Box<dyn std::error::Error>> {
        self.operator.new_sender(to_worker, channel_id).await
    }

    async fn new_receiver(
        &self,
        from_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamReceiver>, Box<dyn std::error::Error>> {
        self.operator.new_receiver(from_worker, channel_id).await
    }
}

#[async_trait]
impl WorkerCoordinatorComm for InterThreadCommunication {
    async fn worker_to_coordinator(
        &self,
    ) -> Result<Box<dyn ReqResReceiver>, Box<dyn std::error::Error + Send + Sync>> {
        self.coordinator.worker_to_coordinator().await
    }

    async fn coordinator_to_worker(
        &self,
        to_worker: WorkerId,
    ) -> Result<Box<dyn ReqResSender>, Box<dyn std::error::Error + Send + Sync>> {
        self.coordinator.coordinator_to_worker(to_worker).await
    }
}
