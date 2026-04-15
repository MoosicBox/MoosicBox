use async_trait::async_trait;
use flume::{Receiver, Sender};
use hyperchad_shared_state_models::{TransportInbound, TransportOutbound};

use crate::{SharedStateTransportClient, TransportError};

#[derive(Debug, Clone)]
pub struct InMemoryTransportClient {
    outbound: Sender<TransportOutbound>,
    inbound: Receiver<TransportInbound>,
}

impl InMemoryTransportClient {
    #[must_use]
    pub const fn new(
        outbound: Sender<TransportOutbound>,
        inbound: Receiver<TransportInbound>,
    ) -> Self {
        Self { outbound, inbound }
    }
}

#[derive(Debug)]
pub struct InMemoryTransportPair {
    pub client: InMemoryTransportClient,
    pub server_inbound: Receiver<TransportOutbound>,
    pub server_outbound: Sender<TransportInbound>,
}

impl InMemoryTransportPair {
    #[must_use]
    pub fn new() -> Self {
        let (to_server_tx, to_server_rx) = flume::unbounded();
        let (to_client_tx, to_client_rx) = flume::unbounded();

        Self {
            client: InMemoryTransportClient::new(to_server_tx, to_client_rx),
            server_inbound: to_server_rx,
            server_outbound: to_client_tx,
        }
    }
}

impl Default for InMemoryTransportPair {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SharedStateTransportClient for InMemoryTransportClient {
    async fn connect(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send(&self, message: TransportOutbound) -> Result<(), TransportError> {
        self.outbound
            .send(message)
            .map_err(|_| TransportError::Disconnected)
    }

    fn inbound(&self) -> Receiver<TransportInbound> {
        self.inbound.clone()
    }
}
