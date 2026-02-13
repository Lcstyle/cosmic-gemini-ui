use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::HydraError;
use crate::observation::CertObservation;

/// Event types from the HYDRA spec (Section 11.2).
/// Only the types relevant to Stages 0-1 are implemented.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    Genesis,
    MemberJoin,
    MemberLeave,
    CertObservationBatch,
}

/// The payload of an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    /// Genesis event — first event in the log.
    Genesis {
        network_id: String,
        creator_pubkey: String,
        creator_onion: String,
    },
    /// A new node joins the network.
    MemberJoin {
        node_id: String,
        onion_address: String,
    },
    /// A node leaves the network.
    MemberLeave {
        node_id: String,
    },
    /// A batch of certificate observations.
    CertObservationBatch {
        observations: Vec<CertObservation>,
    },
}

impl EventPayload {
    pub fn event_type(&self) -> EventType {
        match self {
            Self::Genesis { .. } => EventType::Genesis,
            Self::MemberJoin { .. } => EventType::MemberJoin,
            Self::MemberLeave { .. } => EventType::MemberLeave,
            Self::CertObservationBatch { .. } => EventType::CertObservationBatch,
        }
    }
}

/// A signed event in the HYDRA event log.
///
/// At Stages 0-1, events have a single parent (previous event hash).
/// The hash chain provides tamper detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEvent {
    /// Monotonically increasing sequence number for this author.
    pub sequence: u64,
    /// The event type.
    pub event_type: EventType,
    /// The event payload.
    pub payload: EventPayload,
    /// Hex-encoded Ed25519 public key of the author.
    pub author: String,
    /// Timestamp of creation.
    pub timestamp: DateTime<Utc>,
    /// Ed25519 signature over the canonical event data (hex-encoded).
    pub signature: String,
    /// SHA-256 hash of the previous event (hex), or "0" * 64 for genesis.
    pub prev_hash: String,
}

/// Compute the canonical bytes of an event for signing/hashing.
///
/// The canonical form is: `sequence || event_type || payload_json || author || timestamp || prev_hash`
fn canonical_bytes(
    sequence: u64,
    event_type: &EventType,
    payload: &EventPayload,
    author: &str,
    timestamp: &DateTime<Utc>,
    prev_hash: &str,
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&sequence.to_be_bytes());
    buf.extend_from_slice(serde_json::to_string(event_type).unwrap().as_bytes());
    buf.extend_from_slice(serde_json::to_string(payload).unwrap().as_bytes());
    buf.extend_from_slice(author.as_bytes());
    buf.extend_from_slice(timestamp.to_rfc3339().as_bytes());
    buf.extend_from_slice(prev_hash.as_bytes());
    buf
}

/// Create and sign a new event.
pub fn sign_event(
    signing_key: &SigningKey,
    sequence: u64,
    payload: EventPayload,
    prev_hash: &str,
) -> SignedEvent {
    let author = hex::encode(signing_key.verifying_key().as_bytes());
    let timestamp = Utc::now();
    let event_type = payload.event_type();

    let bytes = canonical_bytes(sequence, &event_type, &payload, &author, &timestamp, prev_hash);
    let sig = crypto::sign(signing_key, &bytes);

    SignedEvent {
        sequence,
        event_type,
        payload,
        author,
        timestamp,
        signature: hex::encode(sig.to_bytes()),
        prev_hash: prev_hash.to_string(),
    }
}

/// Verify the signature on an event.
pub fn verify_event(event: &SignedEvent) -> Result<(), HydraError> {
    let pubkey_bytes = hex::decode(&event.author)
        .map_err(|e| HydraError::Crypto(format!("invalid author pubkey hex: {}", e)))?;
    if pubkey_bytes.len() != 32 {
        return Err(HydraError::Crypto(format!(
            "invalid pubkey length: expected 32, got {}",
            pubkey_bytes.len()
        )));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pubkey_bytes);
    let verifying_key = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|e| HydraError::Crypto(format!("invalid pubkey: {}", e)))?;

    let sig_bytes = hex::decode(&event.signature)
        .map_err(|e| HydraError::Crypto(format!("invalid signature hex: {}", e)))?;
    if sig_bytes.len() != 64 {
        return Err(HydraError::Crypto(format!(
            "invalid signature length: expected 64, got {}",
            sig_bytes.len()
        )));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);

    let bytes = canonical_bytes(
        event.sequence,
        &event.event_type,
        &event.payload,
        &event.author,
        &event.timestamp,
        &event.prev_hash,
    );

    crypto::verify(&verifying_key, &bytes, &signature)
}

/// Compute the SHA-256 hash of an event (for hash chain linking).
pub fn event_hash(event: &SignedEvent) -> String {
    let json = serde_json::to_string(event).expect("event serialization should not fail");
    crypto::sha256_hex(json.as_bytes())
}

/// The zero hash used as prev_hash for genesis events.
pub fn genesis_prev_hash() -> String {
    "0".repeat(64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_keypair;

    #[test]
    fn sign_and_verify_genesis() {
        let key = generate_keypair();
        let payload = EventPayload::Genesis {
            network_id: crypto::network_id_hex(),
            creator_pubkey: hex::encode(key.verifying_key().as_bytes()),
            creator_onion: "test.onion".to_string(),
        };

        let event = sign_event(&key, 0, payload, &genesis_prev_hash());
        assert_eq!(event.sequence, 0);
        assert_eq!(event.event_type, EventType::Genesis);
        assert_eq!(event.prev_hash, genesis_prev_hash());
        assert!(verify_event(&event).is_ok());
    }

    #[test]
    fn sign_and_verify_member_join() {
        let key = generate_keypair();
        let payload = EventPayload::MemberJoin {
            node_id: "abcd1234".to_string(),
            onion_address: "peer.onion".to_string(),
        };

        let event = sign_event(&key, 1, payload, "prev_hash_here");
        assert!(verify_event(&event).is_ok());
    }

    #[test]
    fn sign_and_verify_cert_batch() {
        let key = generate_keypair();
        let obs = CertObservation {
            domain: "test.com".to_string(),
            port: 1965,
            cert_fingerprint: "ff00".to_string(),
            chain_fingerprints: vec!["ff00".to_string()],
            issuer: "CN=CA".to_string(),
            subject: "CN=test.com".to_string(),
            san: vec!["test.com".to_string()],
            not_before: 1000,
            not_after: 2000,
            observed_at: Utc::now(),
            observer_id: hex::encode(key.verifying_key().as_bytes()),
        };

        let payload = EventPayload::CertObservationBatch {
            observations: vec![obs],
        };

        let event = sign_event(&key, 2, payload, "abc123");
        assert!(verify_event(&event).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_event() {
        let key = generate_keypair();
        let payload = EventPayload::Genesis {
            network_id: "test".to_string(),
            creator_pubkey: "pk".to_string(),
            creator_onion: "addr.onion".to_string(),
        };

        let mut event = sign_event(&key, 0, payload, &genesis_prev_hash());
        // Tamper with the sequence number
        event.sequence = 99;
        assert!(verify_event(&event).is_err());
    }

    #[test]
    fn verify_rejects_wrong_author() {
        let key1 = generate_keypair();
        let key2 = generate_keypair();
        let payload = EventPayload::MemberLeave {
            node_id: "test".to_string(),
        };

        let mut event = sign_event(&key1, 0, payload, &genesis_prev_hash());
        // Replace author with a different key
        event.author = hex::encode(key2.verifying_key().as_bytes());
        assert!(verify_event(&event).is_err());
    }

    #[test]
    fn event_hash_deterministic() {
        let key = generate_keypair();
        let payload = EventPayload::Genesis {
            network_id: "net".to_string(),
            creator_pubkey: "pk".to_string(),
            creator_onion: "o.onion".to_string(),
        };

        let event = sign_event(&key, 0, payload, &genesis_prev_hash());
        let h1 = event_hash(&event);
        let h2 = event_hash(&event);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_chain_linking() {
        let key = generate_keypair();

        let genesis = sign_event(
            &key,
            0,
            EventPayload::Genesis {
                network_id: "net".to_string(),
                creator_pubkey: "pk".to_string(),
                creator_onion: "o.onion".to_string(),
            },
            &genesis_prev_hash(),
        );
        let genesis_hash = event_hash(&genesis);

        let event1 = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "peer1".to_string(),
                onion_address: "p1.onion".to_string(),
            },
            &genesis_hash,
        );
        assert_eq!(event1.prev_hash, genesis_hash);
        assert!(verify_event(&event1).is_ok());
    }
}
