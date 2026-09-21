use std::time::Duration;

use tonic::transport::{Channel, Endpoint};

pub use malstrom_k8s_proto::k8s_operator::coordinator_operator_service_client::CoordinatorOperatorServiceClient;
pub use malstrom_k8s_proto::k8s_operator::RescaleRequest;

pub async fn get_coord_api_client(
    endpoint: Endpoint,
) -> Result<CoordinatorOperatorServiceClient<Channel>, tonic::transport::Error> {
    let endpoint = endpoint.connect_timeout(Duration::from_secs(20));
    CoordinatorOperatorServiceClient::connect(endpoint).await
}
