use serde::{Deserialize, Serialize};

use crate::event::SignedEvent;
use crate::peer::PeerInfo;

/// Bootstrap protocol messages.
///
/// Wire format: newline-delimited JSON (each message is one JSON line).
/// This keeps parsing trivial and debugging easy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BootstrapMsg {
    /// Request the peer list from the bootstrap node.
    GetPeers,

    /// Response to GetPeers — the current known peer list.
    Peers {
        peers: Vec<PeerInfo>,
        stage: u8,
    },

    /// Announce this node's presence on the network.
    Announce {
        node_id: String,
        onion_address: String,
    },

    /// Acknowledgement of an Announce.
    AnnounceAck {
        accepted: bool,
    },

    /// Request events since a given sequence number.
    SyncRequest {
        since_sequence: u64,
    },

    /// Response to SyncRequest — events the requester is missing.
    SyncResponse {
        events: Vec<SignedEvent>,
    },

    /// Push events to a peer (bilateral sync: both sides push).
    SyncPush {
        events: Vec<SignedEvent>,
    },

    /// Acknowledgement of a SyncPush.
    SyncPushAck {
        received_count: usize,
    },
}

impl BootstrapMsg {
    /// Serialize to a single JSON line (with trailing newline).
    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Deserialize from a JSON line.
    pub fn from_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_peers_roundtrip() {
        let msg = BootstrapMsg::GetPeers;
        let line = msg.to_line().unwrap();
        let decoded = BootstrapMsg::from_line(&line).unwrap();
        assert!(matches!(decoded, BootstrapMsg::GetPeers));
    }

    #[test]
    fn announce_roundtrip() {
        let msg = BootstrapMsg::Announce {
            node_id: "abc".to_string(),
            onion_address: "test.onion".to_string(),
        };
        let line = msg.to_line().unwrap();
        let decoded = BootstrapMsg::from_line(&line).unwrap();
        match decoded {
            BootstrapMsg::Announce { node_id, onion_address } => {
                assert_eq!(node_id, "abc");
                assert_eq!(onion_address, "test.onion");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn sync_request_roundtrip() {
        let msg = BootstrapMsg::SyncRequest { since_sequence: 42 };
        let line = msg.to_line().unwrap();
        let decoded = BootstrapMsg::from_line(&line).unwrap();
        match decoded {
            BootstrapMsg::SyncRequest { since_sequence } => assert_eq!(since_sequence, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn peers_response_roundtrip() {
        let msg = BootstrapMsg::Peers {
            peers: vec![],
            stage: 1,
        };
        let line = msg.to_line().unwrap();
        let decoded = BootstrapMsg::from_line(&line).unwrap();
        match decoded {
            BootstrapMsg::Peers { peers, stage } => {
                assert!(peers.is_empty());
                assert_eq!(stage, 1);
            }
            _ => panic!("wrong variant"),
        }
    }
}
