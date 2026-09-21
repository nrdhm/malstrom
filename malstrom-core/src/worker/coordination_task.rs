use indexmap::IndexSet;
use malstrom_macros::instrument_debug;
use tokio::sync::mpsc;

use crate::{
    coordinator::messages::RuntimeMessage,
    runtime::communication::WorkerClient,
    snapshot::{PersistenceBackend, SnapshotVersion},
    types::WorkerId,
    worker::sys_message::SysMessage,
};

/// Task for interacting with the central job coordinator
pub(super) struct CoordinationTask<P: PersistenceBackend> {
    worker_id: WorkerId,
    persistence_backend: P,
    sys_msg_sender: mpsc::Sender<SysMessage<P::Client>>,
    coordinator_comm: WorkerClient,
    /// Set to `true` by the worker once its dataflow has completed
    completion: tokio::sync::watch::Receiver<bool>,
}

impl<P> CoordinationTask<P>
where
    P: PersistenceBackend,
{
    pub(super) fn new(
        this_worker: WorkerId,
        persistence_backend: P,
        sys_msg_sender: mpsc::Sender<SysMessage<P::Client>>,
        coordinator_comm: WorkerClient,
        completion: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self {
            worker_id: this_worker,
            persistence_backend,
            sys_msg_sender,
            coordinator_comm,
            completion,
        }
    }

    #[instrument_debug(skip_all, fields(worker_id = self.worker_id))]
    pub(super) fn start(self, comm_rt: &tokio::runtime::Runtime) -> tokio::task::JoinHandle<()> {
        comm_rt.spawn(async move {
            loop {
                let (msg, responder) = self.coordinator_comm.recv::<RuntimeMessage, bool>().await;
                match msg {
                    RuntimeMessage::Snapshot(version) => {
                        self.handle_snapshot(version).await;
                        responder.respond(true).await;
                    }
                    RuntimeMessage::Reconfigure((new_set, new_version)) => {
                        self.handle_reconfigure(new_set, new_version).await;
                        responder.respond(true).await;
                    }
                    RuntimeMessage::ExecutionComplete => {
                        let finished = *self.completion.borrow();
                        responder.respond(finished).await;
                        if finished {
                            return;
                        }
                    }
                }
            }
        })
    }

    #[instrument_debug(skip(self))]
    async fn handle_snapshot(&self, version: SnapshotVersion) {
        let persistence_client = self
            .persistence_backend
            .for_version(self.worker_id, &version);

        let (tx, mut rx) = mpsc::channel(1);
        let msg = SysMessage::Snapshot {
            client: persistence_client,
            callback: tx,
        };
        self.sys_msg_sender
            .send(msg)
            .await
            .expect("the msg to be sent");
        let _ = rx.recv().await;
    }

    #[instrument_debug(skip(self))]
    async fn handle_reconfigure(&self, new_set: IndexSet<WorkerId>, new_version: u64) {
        let (tx, mut rx) = mpsc::channel(1);
        let msg = SysMessage::Reconfigure {
            new_set,
            new_version,
            callback: tx,
        };
        self.sys_msg_sender
            .send(msg)
            .await
            .expect("the msg to be sent");
        let _ = rx.recv().await;
    }
}
