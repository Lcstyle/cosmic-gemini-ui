use ed25519_dalek::{SigningKey, VerifyingKey};

use crate::crypto;

/// The HYDRA network identity.
///
/// Derived deterministically from protocol constants via HKDF-SHA256.
/// Every node computes the same values — this is not a secret.
///
/// ```text
/// HKDF-SHA256(
///     ikm  = "HYDRA-cert-verify-v1",
///     salt = "HYDRA-network-identity",
///     info = "ed25519-network-key",
///     len  = 32
/// )
/// ```
pub struct NetworkIdentity {
    /// The shared Ed25519 signing key (used for bootstrap .onion service).
    pub signing_key: SigningKey,
    /// The shared Ed25519 public key.
    pub verifying_key: VerifyingKey,
    /// SHA-256 of the network public key — the `network_id` in all events.
    pub network_id: [u8; 32],
    /// Hex-encoded network ID.
    pub network_id_hex: String,
}

impl NetworkIdentity {
    /// Derive the network identity from protocol constants.
    ///
    /// This is deterministic — every node produces the same result.
    pub fn derive() -> Self {
        let signing_key = crypto::derive_network_signing_key();
        let verifying_key = signing_key.verifying_key();
        let network_id = crypto::sha256(verifying_key.as_bytes());
        let network_id_hex = hex::encode(network_id);

        Self {
            signing_key,
            verifying_key,
            network_id,
            network_id_hex,
        }
    }

    /// Get the theoretical bootstrap .onion address.
    ///
    /// The actual .onion address is derived by Tor from the Ed25519 key
    /// using the v3 onion service key expansion. This method computes
    /// the public key bytes that Tor would use.
    ///
    /// The full .onion address (with checksum and version byte) is computed
    /// by the Tor subsystem when launching the hidden service.
    pub fn bootstrap_onion_pubkey_bytes(&self) -> &[u8; 32] {
        self.verifying_key.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_deterministic() {
        let id1 = NetworkIdentity::derive();
        let id2 = NetworkIdentity::derive();
        assert_eq!(id1.signing_key.to_bytes(), id2.signing_key.to_bytes());
        assert_eq!(id1.verifying_key.as_bytes(), id2.verifying_key.as_bytes());
        assert_eq!(id1.network_id, id2.network_id);
        assert_eq!(id1.network_id_hex, id2.network_id_hex);
    }

    #[test]
    fn network_id_is_hash_of_pubkey() {
        let id = NetworkIdentity::derive();
        let expected = crypto::sha256(id.verifying_key.as_bytes());
        assert_eq!(id.network_id, expected);
    }

    #[test]
    fn network_id_hex_is_64_chars() {
        let id = NetworkIdentity::derive();
        assert_eq!(id.network_id_hex.len(), 64);
    }

    #[test]
    fn signing_key_can_sign_and_verify() {
        let id = NetworkIdentity::derive();
        let msg = b"bootstrap peer list";
        let sig = crypto::sign(&id.signing_key, msg);
        assert!(crypto::verify(&id.verifying_key, msg, &sig).is_ok());
    }

    #[test]
    fn bootstrap_onion_pubkey_is_32_bytes() {
        let id = NetworkIdentity::derive();
        assert_eq!(id.bootstrap_onion_pubkey_bytes().len(), 32);
    }
}
