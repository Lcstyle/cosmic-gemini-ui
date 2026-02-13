use std::fs;
use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::HydraError;
use crate::store;

/// Persistent node identity for the HYDRA network.
///
/// Each node has a unique Ed25519 keypair generated on first run.
/// The `node_id` is the hex-encoded public key.
pub struct NodeIdentity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    /// Hex-encoded Ed25519 public key — the node's unique identifier.
    pub node_id: String,
}

/// Serialized form stored on disk.
#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    /// Ed25519 secret key bytes, base64-encoded.
    secret_key_b64: String,
    /// Hex-encoded public key (the node_id).
    node_id: String,
}

const IDENTITY_FILENAME: &str = "node_identity.json";

impl NodeIdentity {
    /// Create a NodeIdentity from an existing signing key.
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        let node_id = hex::encode(verifying_key.as_bytes());
        Self {
            signing_key,
            verifying_key,
            node_id,
        }
    }

    /// Load the node identity from disk, or generate a new one if none exists.
    pub fn load_or_generate(data_dir: &Path) -> Result<Self, HydraError> {
        let path = data_dir.join(IDENTITY_FILENAME);

        if path.exists() {
            return Self::load_from_file(&path);
        }

        let identity = Self::generate_new();
        identity.save_to_file(&path)?;
        Ok(identity)
    }

    /// Load from the default HYDRA data directory.
    pub fn load_or_generate_default() -> Result<Self, HydraError> {
        let dir = store::hydra_data_dir();
        store::ensure_dir()?;
        Self::load_or_generate(&dir)
    }

    /// Generate a fresh identity (does not persist).
    pub fn generate_new() -> Self {
        Self::from_signing_key(crypto::generate_keypair())
    }

    /// Sign a message with this node's key.
    pub fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature {
        crypto::sign(&self.signing_key, message)
    }

    /// Verify a signature from this node.
    pub fn verify(&self, message: &[u8], signature: &ed25519_dalek::Signature) -> Result<(), HydraError> {
        crypto::verify(&self.verifying_key, message, signature)
    }

    fn load_from_file(path: &Path) -> Result<Self, HydraError> {
        let data = fs::read_to_string(path)?;
        let stored: StoredIdentity = serde_json::from_str(&data)?;

        let secret_bytes = base64_decode(&stored.secret_key_b64)?;
        if secret_bytes.len() != 32 {
            return Err(HydraError::Crypto(format!(
                "invalid secret key length: expected 32, got {}",
                secret_bytes.len()
            )));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&secret_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();
        let node_id = hex::encode(verifying_key.as_bytes());

        // Sanity check: stored node_id should match derived one
        if node_id != stored.node_id {
            return Err(HydraError::Crypto(
                "stored node_id does not match derived public key".to_string(),
            ));
        }

        Ok(Self {
            signing_key,
            verifying_key,
            node_id,
        })
    }

    fn save_to_file(&self, path: &Path) -> Result<(), HydraError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let stored = StoredIdentity {
            secret_key_b64: base64_encode(self.signing_key.as_bytes()),
            node_id: self.node_id.clone(),
        };

        let json = serde_json::to_string_pretty(&stored)
            .map_err(|e| HydraError::Config(e.to_string()))?;
        store::atomic_write(path, json.as_bytes())
    }
}

// Simple base64 helpers (avoiding an extra dependency)
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let _ = result.write_char(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        let _ = result.write_char(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            let _ = result.write_char(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            let _ = result.write_char(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, HydraError> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in input.chars() {
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(HydraError::Crypto(format!("invalid base64 character: {}", c))),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_has_valid_node_id() {
        let identity = NodeIdentity::generate_new();
        assert_eq!(identity.node_id.len(), 64); // 32 bytes hex
        // node_id should match the verifying key
        assert_eq!(identity.node_id, hex::encode(identity.verifying_key.as_bytes()));
    }

    #[test]
    fn sign_verify_roundtrip() {
        let identity = NodeIdentity::generate_new();
        let msg = b"test message";
        let sig = identity.sign(msg);
        assert!(identity.verify(msg, &sig).is_ok());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let identity = NodeIdentity::load_or_generate(tmp.path()).unwrap();
        let node_id = identity.node_id.clone();
        let key_bytes = identity.signing_key.to_bytes();

        // Load again — should get the same identity
        let loaded = NodeIdentity::load_or_generate(tmp.path()).unwrap();
        assert_eq!(loaded.node_id, node_id);
        assert_eq!(loaded.signing_key.to_bytes(), key_bytes);
    }

    #[test]
    fn different_generates_are_different() {
        let id1 = NodeIdentity::generate_new();
        let id2 = NodeIdentity::generate_new();
        assert_ne!(id1.node_id, id2.node_id);
    }

    #[test]
    fn base64_roundtrip() {
        let data = b"Hello, HYDRA!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_roundtrip_32_bytes() {
        let data: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let encoded = base64_encode(&data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}
