use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::HydraError;
use crate::store;

/// Information about a known HYDRA peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub onion_address: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_sync: Option<DateTime<Utc>>,
}

/// Persistent list of known peers (JSON file).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeerList {
    pub peers: Vec<PeerInfo>,
}

const PEERS_FILENAME: &str = "peers.json";

impl PeerList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from a directory.
    pub fn load(data_dir: &Path) -> Result<Self, HydraError> {
        let path = data_dir.join(PEERS_FILENAME);
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let list: Self = serde_json::from_str(&data)?;
        Ok(list)
    }

    /// Save to a directory (atomic write).
    pub fn save(&self, data_dir: &Path) -> Result<(), HydraError> {
        let path = data_dir.join(PEERS_FILENAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| HydraError::Config(e.to_string()))?;
        store::atomic_write(&path, json.as_bytes())
    }

    /// Add or update a peer.
    pub fn upsert(&mut self, node_id: &str, onion_address: &str) {
        let now = Utc::now();
        if let Some(peer) = self.peers.iter_mut().find(|p| p.node_id == node_id) {
            peer.onion_address = onion_address.to_string();
            peer.last_seen = now;
        } else {
            self.peers.push(PeerInfo {
                node_id: node_id.to_string(),
                onion_address: onion_address.to_string(),
                first_seen: now,
                last_seen: now,
                last_sync: None,
            });
        }
    }

    /// Remove a peer by node_id.
    pub fn remove(&mut self, node_id: &str) -> bool {
        let len_before = self.peers.len();
        self.peers.retain(|p| p.node_id != node_id);
        self.peers.len() < len_before
    }

    /// Get a peer by node_id.
    pub fn get(&self, node_id: &str) -> Option<&PeerInfo> {
        self.peers.iter().find(|p| p.node_id == node_id)
    }

    /// Get a mutable reference to a peer by node_id.
    pub fn get_mut(&mut self, node_id: &str) -> Option<&mut PeerInfo> {
        self.peers.iter_mut().find(|p| p.node_id == node_id)
    }

    /// Record a successful sync with a peer.
    pub fn record_sync(&mut self, node_id: &str) {
        if let Some(peer) = self.get_mut(node_id) {
            peer.last_sync = Some(Utc::now());
            peer.last_seen = Utc::now();
        }
    }

    /// Get the number of known peers.
    pub fn count(&self) -> usize {
        self.peers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn upsert_and_get() {
        let mut list = PeerList::new();
        list.upsert("node1", "addr1.onion");
        assert_eq!(list.count(), 1);
        assert_eq!(list.get("node1").unwrap().onion_address, "addr1.onion");

        // Update existing
        list.upsert("node1", "addr1_new.onion");
        assert_eq!(list.count(), 1);
        assert_eq!(list.get("node1").unwrap().onion_address, "addr1_new.onion");
    }

    #[test]
    fn remove_peer() {
        let mut list = PeerList::new();
        list.upsert("node1", "a.onion");
        list.upsert("node2", "b.onion");
        assert_eq!(list.count(), 2);

        assert!(list.remove("node1"));
        assert_eq!(list.count(), 1);
        assert!(list.get("node1").is_none());

        assert!(!list.remove("nonexistent"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut list = PeerList::new();
        list.upsert("n1", "a.onion");
        list.upsert("n2", "b.onion");
        list.save(tmp.path()).unwrap();

        let loaded = PeerList::load(tmp.path()).unwrap();
        assert_eq!(loaded.count(), 2);
        assert_eq!(loaded.get("n1").unwrap().onion_address, "a.onion");
    }

    #[test]
    fn load_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let list = PeerList::load(tmp.path()).unwrap();
        assert_eq!(list.count(), 0);
    }

    #[test]
    fn record_sync() {
        let mut list = PeerList::new();
        list.upsert("node1", "a.onion");
        assert!(list.get("node1").unwrap().last_sync.is_none());

        list.record_sync("node1");
        assert!(list.get("node1").unwrap().last_sync.is_some());
    }
}
