//! Shared helpers for `malstrom-core` integration tests: an in-process mock of the
//! communication seams (`OperatorOperatorComm`, `WorkerCoordinatorComm`, `RuntimeFlavor`
//! `Communication`) plus a `RuntimeFlavor` built on it. Each sender/receiver pair is a
//! fresh flume channel, so round-trips are deterministic.

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use malstrom_core::runtime::RuntimeFlavor;
use malstrom_core::runtime::communication::{
    OperatorOperatorComm, ReqResReceiver, ReqResResponder, ReqResSender, StreamReceiver,
    StreamSender, WorkerCoordinatorComm,
};
use malstrom_core::types::{OperatorId, WorkerId};

/// A sender for in-process operator-to-operator streams.
pub struct MemoryStreamSender {
    tx: flume::Sender<Vec<u8>>,
}

#[async_trait]
impl StreamSender for MemoryStreamSender {
    async fn send(&self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.tx.send(msg).map_err(|_| "channel send failed")?;
        Ok(())
    }
}

/// A receiver for in-process operator-to-operator streams.
pub struct MemoryStreamReceiver {
    rx: Arc<flume::Receiver<Vec<u8>>>,
}

#[async_trait]
impl StreamReceiver for MemoryStreamReceiver {
    async fn recv(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(self.rx.recv_async().await?)
    }
}

/// A request-response pair: the request bytes plus a oneshot for the response.
type ReqRes = (Vec<u8>, tokio::sync::oneshot::Sender<Vec<u8>>);

/// In-process request-response sender.
pub struct MemoryReqResSender {
    tx: flume::Sender<ReqRes>,
}

#[async_trait]
impl ReqResSender for MemoryReqResSender {
    async fn send(&self, msg: Vec<u8>) -> Result<Vec<u8>, Box<dyn Error>> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx.send((msg, resp_tx)).map_err(|e| e.to_string())?;
        Ok(resp_rx.await.map_err(|e| e.to_string())?)
    }
}

/// In-process request-response receiver.
pub struct MemoryReqResReceiver {
    rx: flume::Receiver<ReqRes>,
}

#[async_trait]
impl ReqResReceiver for MemoryReqResReceiver {
    async fn recv(&self) -> Result<(Vec<u8>, Box<dyn ReqResResponder>), Box<dyn Error>> {
        let (msg, resp_tx) = self.rx.recv_async().await?;
        Ok((msg, Box::new(MemoryResponder { tx: Some(resp_tx) })))
    }
}

struct MemoryResponder {
    tx: Option<tokio::sync::oneshot::Sender<Vec<u8>>>,
}

#[async_trait]
impl ReqResResponder for MemoryResponder {
    async fn respond(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let tx = self.tx.take().ok_or("responder already used")?;
        tx.send(msg).map_err(|_| "response channel closed")?;
        Ok(())
    }
}

/// An in-process implementation of both comm seams. `new_sender`/`new_receiver` pair up
/// by `(worker, operator)`; the worker↔coordinator req/res channel is created by
/// `worker_to_coordinator` (for this mock's single worker) and consumed by
/// `coordinator_to_worker`.
#[derive(Clone)]
pub struct MemoryComm {
    worker_id: WorkerId,
    receivers: Arc<Mutex<HashMap<(WorkerId, OperatorId), Arc<flume::Receiver<Vec<u8>>>>>>,
    coordinator_senders: Arc<Mutex<HashMap<WorkerId, flume::Sender<ReqRes>>>>,
}

impl MemoryComm {
    /// Create a mock comm for a single worker with the given id.
    pub fn new(worker_id: WorkerId) -> Self {
        Self {
            worker_id,
            receivers: Arc::new(Mutex::new(HashMap::new())),
            coordinator_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl OperatorOperatorComm for MemoryComm {
    async fn new_sender(
        &self,
        to_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamSender>, Box<dyn Error>> {
        let (tx, rx) = flume::unbounded();
        self.receivers
            .lock()
            .unwrap()
            .insert((to_worker, channel_id), Arc::new(rx));
        Ok(Box::new(MemoryStreamSender { tx }))
    }

    async fn new_receiver(
        &self,
        from_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamReceiver>, Box<dyn Error>> {
        let rx = self
            .receivers
            .lock()
            .unwrap()
            .remove(&(from_worker, channel_id))
            .ok_or_else(|| format!("no channel for ({from_worker}, {channel_id})"))?;
        Ok(Box::new(MemoryStreamReceiver { rx }))
    }
}

#[async_trait]
impl WorkerCoordinatorComm for MemoryComm {
    async fn worker_to_coordinator(
        &self,
    ) -> Result<Box<dyn ReqResReceiver>, Box<dyn Error + Send + Sync>> {
        let (tx, rx) = flume::unbounded();
        self.coordinator_senders
            .lock()
            .unwrap()
            .insert(self.worker_id, tx);
        Ok(Box::new(MemoryReqResReceiver { rx }))
    }

    async fn coordinator_to_worker(
        &self,
        to_worker: WorkerId,
    ) -> Result<Box<dyn ReqResSender>, Box<dyn Error + Send + Sync>> {
        let tx = self
            .coordinator_senders
            .lock()
            .unwrap()
            .remove(&to_worker)
            .ok_or_else(|| format!("worker {to_worker} not connected"))?;
        Ok(Box::new(MemoryReqResSender { tx }))
    }
}

/// A `RuntimeFlavor` whose `Communication` is the in-process [MemoryComm].
pub struct MemoryFlavor {
    comm: MemoryComm,
    worker_id: WorkerId,
}

impl MemoryFlavor {
    /// Create a flavor for a single worker with the given id.
    pub fn new(worker_id: WorkerId) -> Self {
        Self {
            comm: MemoryComm::new(worker_id),
            worker_id,
        }
    }
}

impl RuntimeFlavor for MemoryFlavor {
    type Communication = MemoryComm;

    fn communication(&mut self) -> Result<Self::Communication, Box<dyn Error + Send + Sync>> {
        Ok(self.comm.clone())
    }

    fn this_worker_id(&self) -> u64 {
        self.worker_id
    }
}
