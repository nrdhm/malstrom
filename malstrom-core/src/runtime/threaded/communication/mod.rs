use crate::types::{OperatorId, WorkerId};

mod inter_thread;
mod reqres;
mod stream;

pub(super) use inter_thread::{
    CoordinatorChannels, CoordinatorCommunication, OperatorChannels, OperatorCommunication,
};
use reqres::{ReqResReceiver, ReqResSender};
use stream::{OperatorReceiver, OperatorSender};

/// uniquely identifies a connection
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub(super) struct ConnectionKey {
    sending: WorkerId,
    receiving: WorkerId,
    operator: OperatorId,
}
impl ConnectionKey {
    /// generates the same key no matter in which direction the connection is supplied
    fn new(sending: WorkerId, receiving: WorkerId, operator: OperatorId) -> Self {
        Self {
            sending,
            receiving,
            operator,
        }
    }
}
