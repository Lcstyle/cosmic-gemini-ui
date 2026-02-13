use serde::{Deserialize, Serialize};

use crate::error::HydraError;
use crate::store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub node_id: String,
    pub onion_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraConfig {
    pub enabled: bool,
    pub bootstrap_port: u16,
    pub sync_interval_secs: u64,
    pub max_observation_age_days: u64,
    pub peers: Vec<PeerConfig>,
}

impl Default for HydraConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bootstrap_port: 19650,
            sync_interval_secs: 300,
            max_observation_age_days: 90,
            peers: Vec::new(),
        }
    }
}

impl HydraConfig {
    pub fn load() -> Result<Self, HydraError> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&data)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), HydraError> {
        let path = config_path();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| HydraError::Config(e.to_string()))?;
        store::atomic_write(&path, json.as_bytes())
    }
}

fn config_path() -> std::path::PathBuf {
    store::hydra_data_dir().join("config.json")
}
