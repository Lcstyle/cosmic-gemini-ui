use async_trait::async_trait;

use crate::error::HydraError;
use crate::store;
use crate::transport::Transport;

/// Tor-based transport using Arti.
///
/// Provides .onion connections for peer-to-peer communication.
/// All HYDRA traffic goes through Tor — no clearnet connections.
pub struct TorTransport {
    /// The bootstrapped Arti Tor client.
    client: arti_client::TorClient<tor_rtcompat::PreferredRuntime>,
}

impl TorTransport {
    /// Bootstrap a new Tor client and create the transport.
    ///
    /// This may take 10-30 seconds as Tor downloads consensus and builds circuits.
    pub async fn bootstrap() -> Result<Self, HydraError> {
        let tor_dir = store::tor_data_dir();
        std::fs::create_dir_all(&tor_dir).map_err(|e| {
            HydraError::Tor(format!("failed to create tor data dir: {}", e))
        })?;

        let state_dir = tor_dir.join("state");
        let cache_dir = tor_dir.join("cache");

        let config = arti_client::config::TorClientConfigBuilder::from_directories(state_dir, cache_dir)
            .build()
            .map_err(|e| HydraError::Tor(format!("tor config error: {}", e)))?;

        let client = arti_client::TorClient::create_bootstrapped(config)
            .await
            .map_err(|e| HydraError::Tor(format!("tor bootstrap failed: {}", e)))?;

        log::info!("Tor client bootstrapped successfully");
        Ok(Self { client })
    }

    /// Get a reference to the underlying Arti TorClient.
    pub fn client(&self) -> &arti_client::TorClient<tor_rtcompat::PreferredRuntime> {
        &self.client
    }
}

#[async_trait]
impl Transport for TorTransport {
    type Stream = arti_client::DataStream;

    async fn connect(&self, address: &str) -> Result<Self::Stream, HydraError> {
        // Arti's connect expects (host, port) or "host:port" format
        let stream: arti_client::DataStream = self
            .client
            .connect(address)
            .await
            .map_err(|e| HydraError::Transport(format!("tor connect to {}: {}", address, e)))?;
        Ok(stream)
    }
}
