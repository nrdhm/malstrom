//! Utilities for testing inter-worker communication

use async_trait::async_trait;

use malstrom_core::runtime::{
        OperatorOperatorComm,
        communication::{StreamReceiver, StreamSender},
    };
use malstrom_core::types::{OperatorId, WorkerId};


/// A CommunicationBackend which will always return an error when trying to create a connection.
/// This is only really useful for unit tests where you know the operator will not attempt
/// to make a connection or want to assert it does not.
#[derive(Debug, Default)]
pub struct NoCommunication;

#[async_trait]
impl OperatorOperatorComm for NoCommunication {
    async fn new_sender(
        &self,
        _to_worker: WorkerId,
        _channel_id: OperatorId,
    ) -> Result<Box<dyn StreamSender>, Box<dyn std::error::Error>> {
        Err("NoCommunication backend cannot create senders".into())
    }

    async fn new_receiver(
        &self,
        _from_worker: WorkerId,
        _channel_id: OperatorId,
    ) -> Result<Box<dyn StreamReceiver>, Box<dyn std::error::Error>> {
        Err("NoCommunication backend cannot create receivers".into())
    }
}
