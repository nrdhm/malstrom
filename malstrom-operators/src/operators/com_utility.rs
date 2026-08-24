//! Communciation utility for operators to communicate with other workers.

use std::{collections::HashMap, rc::Rc};

use futures::{StreamExt, stream::FuturesUnordered};
use indexmap::IndexSet;
use thiserror::Error;

use malstrom::channels::recv_trait::Receiver;
use malstrom::runtime::{
        OperatorOperatorComm,
        communication::{OperatorCommReceiver, OperatorCommSender},
    };
use malstrom::stream::{BuildContext, OperatorContext};
use malstrom::types::{OperatorId, ReconfigComplete, RescaleMessage, WorkerId, distributable::Distributable};


struct SenderReceiver<T> {
    sender: OperatorCommSender<T>,
    receiver: OperatorCommReceiver<T>,
}

impl<T> SenderReceiver<T>
where
    T: Distributable,
{
    async fn new(
        comm: &Rc<dyn OperatorOperatorComm>,
        to_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Self {
        let receiver = OperatorCommReceiver::new(to_worker, channel_id, &**comm)
            .await
            .expect("Backend communication failed");

        let sender = OperatorCommSender::new(to_worker, channel_id, &**comm)
            .await
            .expect("Backend communication failed");
        Self { sender, receiver }
    }
}

pub struct CommUtility<T> {
    clients: HashMap<WorkerId, SenderReceiver<T>>,
    /// Communication backend for inter-operator communication
    comm: Rc<dyn OperatorOperatorComm>,
    /// channel id this CommUtility pair is keyed by (used when rescaling adds workers)
    channel_id: OperatorId,
}

impl<T> CommUtility<T>
where
    T: Distributable,
{
    /// Connect to every worker (including ourselves — a CommUtility pair may live on
    /// the same worker, e.g. a source's reader op and its discovery coordinator).
    /// `channel_id` must be shared by the two sides of the pair; give each pair its
    /// own id so concurrent users do not collide.
    pub async fn new(ctx: &BuildContext, channel_id: OperatorId) -> Self {
        let comm = ctx.get_communication();
        let worker_ids = ctx.get_worker_ids().to_owned();

        let mut clients = HashMap::with_capacity(worker_ids.len());
        for wid in worker_ids.iter() {
            let sender_receiver = SenderReceiver::new(&comm, *wid, channel_id).await;
            clients.insert(*wid, sender_receiver);
        }
        let comm = Rc::clone(&comm);

        Self {
            clients,
            comm,
            channel_id,
        }
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
            let sender_receiver = SenderReceiver::new(&self.comm, *wid, self.channel_id).await;
            self.clients.insert(*wid, sender_receiver);
        }
    }
}

#[derive(Debug, Error)]
pub enum CommUtilityError<T> {
    #[error("Worker ID not connected")]
    WorkerIdNotConnected(T),
}
