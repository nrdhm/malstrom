//! Communciation utility for operators to communicate with other workers.

use std::{collections::HashMap, rc::Rc};

use futures::{StreamExt, stream::FuturesUnordered};
use indexmap::IndexSet;
use thiserror::Error;

use crate::{
    channels::recv_trait::Receiver,
    runtime::{
        OperatorOperatorComm,
        communication::{OperatorCommReceiver, OperatorCommSender},
    },
    stream::{BuildContext, OperatorContext},
    types::{OperatorId, ReconfigComplete, RescaleMessage, WorkerId, distributable::Distributable},
};

struct SenderReceiver<T> {
    sender: OperatorCommSender<T>,
    receiver: OperatorCommReceiver<T>,
}

/// Channel id shared by the two sides of a CommUtility connection.
/// NOTE: this means all CommUtility pairs on a worker share one channel; fine while only
/// sources use it (one source per stream). A per-source id would be needed for multiple
/// concurrent CommUtility users.
const COMM_CHANNEL_ID: OperatorId = u64::MAX;

impl<T> SenderReceiver<T>
where
    T: Distributable,
{
    async fn new(
        comm: &Rc<dyn OperatorOperatorComm>,
        to_worker: WorkerId,
    ) -> Self {
        let receiver = OperatorCommReceiver::new(to_worker, COMM_CHANNEL_ID, &**comm)
            .await
            .expect("Backend communication failed");

        let sender = OperatorCommSender::new(to_worker, COMM_CHANNEL_ID, &**comm)
            .await
            .expect("Backend communication failed");
        Self { sender, receiver }
    }
}

pub struct CommUtility<T> {
    clients: HashMap<WorkerId, SenderReceiver<T>>,
    /// Communication backend for inter-operator communication
    comm: Rc<dyn OperatorOperatorComm>,
}

impl<T> CommUtility<T>
where
    T: Distributable,
{
    pub async fn new(ctx: &BuildContext) -> Self {
        let comm = ctx.get_communication();
        // connect to every worker, including ourselves — a CommUtility pair may live
        // on the same worker (e.g. source partition-op and part-lister)
        let worker_ids = ctx.get_worker_ids().to_owned();

        let mut clients = HashMap::with_capacity(worker_ids.len());
        for wid in worker_ids.iter() {
            let sender_receiver = SenderReceiver::new(&comm, *wid).await;
            clients.insert(*wid, sender_receiver);
        }
        let comm = Rc::clone(&comm);

        Self { clients, comm }
    }

    pub async fn recv(&mut self) -> T {
        if self.clients.is_empty() {
            return std::future::pending().await;
        }
        let mut unordered: FuturesUnordered<_> = self
            .clients
            .values_mut()
            .map(|x| x.receiver.recv())
            .collect();
        unordered.next().await.expect("Clients must not be empty")
    }

    pub async fn send(&self, wid: WorkerId, msg: T) -> Result<(), CommUtilityError<T>> {
        match &self.clients.get(&wid) {
            Some(sr) => {
                sr.sender.send(msg).await;
                Ok(())
            }
            None => Err(CommUtilityError::WorkerIdNotConnected(msg)),
        }
    }

    /// Handles reconfiguration complete messages
    ///
    /// Updates both remote receivers and senders to match the new worker set
    /// from the reconfiguration event, removing connections to workers that are no
    /// longer part of the system.
    ///
    /// # Arguments
    /// * `reconfig` - The reconfiguration complete message containing the new worker set
    /// * `output` - The output stream to forward the reconfiguration message to
    async fn handle_reconfig_complete(&mut self, reconfig: &ReconfigComplete) {
        let workers = reconfig.get_new_worker_set();
        self.clients.retain(|wid, _| workers.contains(wid));
    }

    /// Handles rescale messages by establishing connections to new workers
    ///
    /// When the system scales up, this method creates new receiver connections and
    /// sender connections to any workers that have been added to the system
    /// but don't yet have established connections.
    ///
    /// # Arguments
    /// * `rescale` - The rescale message containing the complete set of workers
    /// * `ctx` - The operator context needed to create new receiver and sender connections
    async fn handle_rescale(&mut self, rescale: &RescaleMessage, ctx: &OperatorContext) {
        let all_workers = rescale.get_all_workers();
        let existing_workers: IndexSet<WorkerId> = self.clients.keys().map(|x| *x).collect();
        let new_workers = all_workers.difference(&existing_workers);
        for wid in new_workers.into_iter() {
            let sender_receiver = SenderReceiver::new(&self.comm, *wid).await;
            self.clients.insert(*wid, sender_receiver);
        }
    }
}

#[derive(Debug, Error)]
pub enum CommUtilityError<T> {
    #[error("Worker ID not connected")]
    WorkerIdNotConnected(T),
}
