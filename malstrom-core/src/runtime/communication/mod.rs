//! Traits for implementing inter-worker and worker-coordinator communication in different runtimes

use crate::types::distributable::Distributable;

mod operator_operator;
mod reqres;
mod stream;
mod worker_coordinator;

pub use operator_operator::OperatorOperatorComm;
pub use reqres::{ReqResReceiver, ReqResResponder, ReqResSender};
pub use stream::{StreamReceiver, StreamSender};
pub use worker_coordinator::WorkerCoordinatorComm;

/// The receiver side of an operator-to-operator channel, used by the distributed crate.
pub use operator_operator::OperatorCommReceiver;
/// The sender side of an operator-to-operator channel, used by the distributed crate.
pub use operator_operator::OperatorCommSender;
pub(crate) use worker_coordinator::{CoordinatorClient, WorkerClient};

/// A convinience method to broadcast a message to all available clients
pub async fn broadcast<'a, T: Distributable + Clone + 'a>(
    clients: impl Iterator<Item = &'a OperatorCommSender<T>>,
    msg: T,
) {
    futures::future::join_all(clients.map(|c| c.send(msg.clone()))).await;
}
