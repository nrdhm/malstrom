use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bon::Builder;
use thiserror::Error;

use crate::{
    coordinator::{ApiRequestError, Coordinator, CoordinatorApi, CoordinatorExecutionError},
    runtime::{
        OperatorOperatorComm, RuntimeFlavor,
        communication::{
            ReqResReceiver, ReqResSender, StreamReceiver, StreamSender, WorkerCoordinatorComm,
        },
    },
    snapshot::PersistenceBackend,
    types::{OperatorId, WorkerId},
    worker::{StreamProvider, WorkerBuilder, WorkerExecutionError},
};

use super::communication::{
    CoordinatorChannels, CoordinatorCommunication, OperatorChannels, OperatorCommunication,
};

/// Runs all dataflows on multiple threads within one machine
///
/// See the `multi_thread_runtime_runs_dataflow_on_all_workers` test for a runnable
/// example that uses only the kernel's public extension API — plain
/// [`Logic`](crate::stream::Logic) operators wired via
/// [`Operator::built_by`](crate::stream::Operator), no `malstrom-operators` needed.
#[derive(Builder)]
pub struct MultiThreadRuntime<P> {
    #[builder(finish_fn)]
    build: fn(&mut dyn StreamProvider) -> (),
    persistence: P,
    snapshots: Option<Duration>,
    parrallelism: u64,
    #[builder(default = tokio::sync::watch::Sender::new(None))]
    api_handles: tokio::sync::watch::Sender<Option<CoordinatorApi>>,
    #[builder(default = std::sync::mpsc::channel())]
    rescale_req: (std::sync::mpsc::Sender<u64>, std::sync::mpsc::Receiver<u64>),
}

impl<P> MultiThreadRuntime<P>
where
    P: PersistenceBackend + Clone + Send + Sync,
{
    /// Start job execution an all workers in this runtime.
    pub fn execute(self) -> Result<(), WorkerExecutionError> {
        let mut threads = Vec::with_capacity(self.parrallelism as usize);

        let coord_channels = CoordinatorChannels::default();
        let operator_channels = OperatorChannels::default();

        let (coordinator, coordinator_api) = Coordinator::new();

        let coordinator_thread = {
            let persistence = self.persistence.clone();
            let comm = CoordinatorCommunication::new(Arc::clone(&coord_channels), WorkerId::MAX);
            let shared = Arc::clone(&coord_channels);
            std::thread::spawn(move || {
                coordinator
                    .execute(self.parrallelism, self.snapshots, persistence, comm)
                    .map_err(ExecutionError::Coordinator)
            })
        };
        threads.push(coordinator_thread);

        // fails if there are no API handles
        let _ = self.api_handles.send(Some(coordinator_api));

        for i in 0..self.parrallelism {
            let thread = Self::spawn_worker(
                self.build,
                self.persistence.clone(),
                Arc::clone(&operator_channels),
                Arc::clone(&coord_channels),
                i,
            );
            threads.push(thread);
        }

        loop {
            if let Ok(desired) = self.rescale_req.1.try_recv() {
                let actual = threads.len() as u64;
                if desired > actual {
                    for i in actual..desired {
                        let thread = Self::spawn_worker(
                            self.build,
                            self.persistence.clone(),
                            Arc::clone(&operator_channels),
                            Arc::clone(&coord_channels),
                            i,
                        );
                        threads.push(thread);
                    }
                }
            }
            threads.retain(|x| !x.is_finished());
            if threads.is_empty() {
                return Ok(());
            }
        }
    }

    fn spawn_worker(
        build_fn: fn(&mut dyn StreamProvider) -> (),
        persistence: P,
        operator_channels: OperatorChannels,
        coordinator_channels: CoordinatorChannels,
        thread_id: u64,
    ) -> std::thread::JoinHandle<Result<(), ExecutionError>> {
        std::thread::Builder::new()
            .name(format!("worker-{thread_id}"))
            .spawn(move || {
                let flavor = MultiThreadRuntimeFlavor::new(
                    operator_channels,
                    coordinator_channels,
                    thread_id,
                );
                let mut worker_builder = WorkerBuilder::new(flavor, persistence);
                build_fn(&mut worker_builder);
                worker_builder.execute().map_err(ExecutionError::Worker)
            })
            .expect("IO error spawning worker")
    }

    /// Get an API handle for interacting with the Malstrom job, e.g. for triggering rescales.
    pub fn api_handle(&self) -> MultiThreadRuntimeApiHandle {
        MultiThreadRuntimeApiHandle {
            coord_channel: self.api_handles.subscribe(),
            rescale_req: self.rescale_req.0.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Error executing worker")]
    Worker(#[from] WorkerExecutionError),
    #[error("Error executing coordinator")]
    Coordinator(#[from] CoordinatorExecutionError),
}

/// This is passed to the worker
/// You can not construct this directly use [MultiThreadRuntime] instead
pub struct MultiThreadRuntimeFlavor {
    operator_channels: OperatorChannels,
    coordinator_channels: CoordinatorChannels,
    worker_id: u64,
}
impl MultiThreadRuntimeFlavor {
    fn new(
        operator_channels: OperatorChannels,
        coordinator_channels: CoordinatorChannels,
        worker_id: WorkerId,
    ) -> Self {
        MultiThreadRuntimeFlavor {
            operator_channels,
            coordinator_channels,
            worker_id,
        }
    }
}

impl RuntimeFlavor for MultiThreadRuntimeFlavor {
    type Communication = InterThreadCommunication;

    fn communication(
        &mut self,
    ) -> Result<Self::Communication, Box<dyn std::error::Error + Send + Sync>> {
        Ok(InterThreadCommunication {
            operator: OperatorCommunication::new(self.operator_channels.clone(), self.worker_id),
            coordinator: CoordinatorCommunication::new(
                self.coordinator_channels.clone(),
                self.worker_id,
            ),
        })
    }

    fn this_worker_id(&self) -> u64 {
        self.worker_id
    }
}

/// In-process communication for the multi-thread runtime (one worker per thread).
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

pub struct MultiThreadRuntimeApiHandle {
    coord_channel: tokio::sync::watch::Receiver<Option<CoordinatorApi>>,
    rescale_req: std::sync::mpsc::Sender<u64>,
}

impl MultiThreadRuntimeApiHandle {
    pub async fn rescale(&self, desired: u64) -> Result<(), ApiRequestError> {
        // instruct the runtime to spawn another thread if needed
        self.rescale_req
            .send(desired)
            .map_err(|_| ApiRequestError::NotRunning)?;
        // instruct the coordinator to re-distribute computation
        self.coord_channel
            .borrow()
            .as_ref()
            .ok_or(ApiRequestError::NotRunning)?
            .rescale(desired)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        channels::operator_io::{Input, Output},
        runtime::MultiThreadRuntime,
        snapshot::NoPersistence,
        stream::{BuildContext, Logic, LogicBuilder, Malstrom as _, Operator, OperatorContext},
        types::{DataMessage, Message},
        worker::StreamProvider,
    };

    /// A minimal source: emits `0..10` once, then finishes the stream.
    struct Numbers(usize);
    impl Logic<(), (usize, usize, usize)> for Numbers {
        async fn apply(
            &mut self,
            _input: &mut Input<()>,
            output: &mut Output<(usize, usize, usize)>,
            _ctx: &mut OperatorContext,
        ) {
            if self.0 == 0 {
                for i in 0..10 {
                    output
                        .send(Message::Data(DataMessage::new(i, i, i)))
                        .await;
                }
                output.send(Message::Epoch(usize::MAX)).await;
            }
            self.0 += 1;
        }
    }

    /// A pass-through operator that prints which worker handled each record.
    struct PrintWorker;
    impl Logic<(usize, usize, usize), (usize, usize, usize)> for PrintWorker {
        async fn apply(
            &mut self,
            input: &mut Input<(usize, usize, usize)>,
            output: &mut Output<(usize, usize, usize)>,
            ctx: &mut OperatorContext,
        ) {
            let msg = input.recv().await;
            if let Message::Data(d) = &msg {
                println!("{} @ Worker {}", d.value, ctx.worker_id);
            }
            output.send(msg).await;
        }
    }

    /// Every worker runs the same dataflow, so the four workers each emit `0..10`.
    /// Exercises the kernel's public extension API end-to-end (no `malstrom-operators`).
    #[test]
    fn multi_thread_runtime_runs_dataflow_on_all_workers() {
        MultiThreadRuntime::builder()
            .parrallelism(4)
            .persistence(NoPersistence)
            .build(|provider: &mut dyn StreamProvider| {
                provider
                    .new_stream()
                    .then(Operator::built_by(
                        "numbers".to_string(),
                        |_ctx: &mut BuildContext| async { Numbers(0) },
                    ))
                    .then(Operator::built_by(
                        "print-worker".to_string(),
                        |_ctx: &mut BuildContext| async { PrintWorker },
                    ));
            })
            .execute()
            .unwrap();
    }
}
