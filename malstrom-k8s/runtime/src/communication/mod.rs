mod discovery;
mod worker_backend;
/// Intra-job communication (generated once in `malstrom-k8s-proto`).
use malstrom_k8s_proto::{exchange, k8s_operator};

mod coordinator_backend;
pub(crate) mod transport;
mod util;

pub(crate) use coordinator_backend::CoordinatorGrpcBackend;
pub(crate) use worker_backend::WorkerGrpcBackend;

pub enum APICommand {
    Rescale(RescaleCommand),
}
pub struct RescaleCommand {
    pub desired: u64,
    pub on_finish: tokio::sync::oneshot::Sender<()>,
}
