//! Contract test for the runtime seam: a local `RuntimeFlavor` with an in-process
//! `OperatorOperatorComm` + `WorkerCoordinatorComm` implementation works — the same
//! seam `malstrom-k8s`/`malstrom-distributed` rely on.

mod common;

use common::{init_logs, MemoryComm, MemoryFlavor};
use malstrom_core::runtime::RuntimeFlavor;
use malstrom_core::runtime::communication::{OperatorOperatorComm, WorkerCoordinatorComm};

#[tokio::test]
async fn operator_stream_round_trip() {
    init_logs();
    let comm = MemoryComm::new(0);
    let sender = comm.new_sender(1, 7).await.unwrap();
    let receiver = comm.new_receiver(1, 7).await.unwrap();

    sender.send(b"hello".to_vec()).await.unwrap();
    assert_eq!(receiver.recv().await.unwrap(), b"hello");
}

#[tokio::test]
async fn worker_coordinator_req_res_round_trip() {
    init_logs();
    let comm = MemoryComm::new(0);
    let receiver = comm.worker_to_coordinator().await.unwrap();
    let sender = comm.coordinator_to_worker(0).await.unwrap();

    // the send must run concurrently with the receive (it blocks awaiting the response)
    let send_task = tokio::spawn(async move { sender.send(b"request".to_vec()).await.unwrap() });
    let (msg, mut responder) = receiver.recv().await.unwrap();
    assert_eq!(msg, b"request");
    responder.respond(b"response".to_vec()).await.unwrap();
    assert_eq!(send_task.await.unwrap(), b"response");
}

#[tokio::test]
async fn flavor_communication_and_worker_id() {
    init_logs();
    let mut flavor = MemoryFlavor::new(3);
    assert_eq!(flavor.this_worker_id(), 3);
    let comm = flavor.communication().unwrap();

    // the flavor's communication backend is usable: a full operator round-trip
    let tx = comm.new_sender(0, 1).await.unwrap();
    let rx = comm.new_receiver(0, 1).await.unwrap();
    tx.send(vec![1, 2, 3]).await.unwrap();
    assert_eq!(rx.recv().await.unwrap(), vec![1, 2, 3]);
}
