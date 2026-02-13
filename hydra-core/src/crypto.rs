use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

use crate::error::HydraError;

/// Generate a new Ed25519 keypair from OS randomness.
pub fn generate_keypair() -> SigningKey {
    let mut csprng = rand::thread_rng();
    SigningKey::generate(&mut csprng)
}

/// Sign a message with an Ed25519 signing key.
pub fn sign(key: &SigningKey, message: &[u8]) -> Signature {
    key.sign(message)
}

/// Verify an Ed25519 signature.
pub fn verify(pubkey: &VerifyingKey, message: &[u8], signature: &Signature) -> Result<(), HydraError> {
    pubkey
        .verify(message, signature)
        .map_err(|_| HydraError::InvalidSignature)
}

/// Compute SHA-256 of data and return as hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Compute SHA-256 of data and return raw bytes.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// HYDRA network identity constants.
const NETWORK_IKM: &[u8] = b"HYDRA-cert-verify-v1";
const NETWORK_SALT: &[u8] = b"HYDRA-network-identity";
const NETWORK_INFO: &[u8] = b"ed25519-network-key";

/// Derive the HYDRA network identity key using HKDF-SHA256.
///
/// This is a protocol constant — every node derives the same key.
/// The result is a 32-byte seed that can be used to create an Ed25519 keypair.
///
/// ```text
/// HKDF-SHA256(
///     ikm  = "HYDRA-cert-verify-v1",
///     salt = "HYDRA-network-identity",
///     info = "ed25519-network-key",
///     len  = 32
/// )
/// ```
pub fn derive_network_seed() -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(NETWORK_SALT), NETWORK_IKM);
    let mut okm = [0u8; 32];
    hk.expand(NETWORK_INFO, &mut okm)
        .expect("HKDF expand should never fail for 32 bytes");
    okm
}

/// Derive the network Ed25519 signing key from the HKDF seed.
///
/// Every HYDRA node derives the same key — this is the shared
/// network identity used for the bootstrap .onion service.
pub fn derive_network_signing_key() -> SigningKey {
    let seed = derive_network_seed();
    SigningKey::from_bytes(&seed)
}

/// Derive the network Ed25519 verifying (public) key.
pub fn derive_network_verifying_key() -> VerifyingKey {
    derive_network_signing_key().verifying_key()
}

/// Compute the network ID: SHA-256 of the network public key.
///
/// This is the `network_id` field in all events: `H(network_pubkey)`.
pub fn network_id() -> [u8; 32] {
    let pubkey = derive_network_verifying_key();
    sha256(pubkey.as_bytes())
}

/// Compute the network ID as hex string.
pub fn network_id_hex() -> String {
    hex::encode(network_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_sign_verify_roundtrip() {
        let key = generate_keypair();
        let msg = b"hello hydra";
        let sig = sign(&key, msg);
        assert!(verify(&key.verifying_key(), msg, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_wrong_message() {
        let key = generate_keypair();
        let sig = sign(&key, b"correct message");
        assert!(verify(&key.verifying_key(), b"wrong message", &sig).is_err());
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let key1 = generate_keypair();
        let key2 = generate_keypair();
        let sig = sign(&key1, b"message");
        assert!(verify(&key2.verifying_key(), b"message", &sig).is_err());
    }

    #[test]
    fn sha256_hex_deterministic() {
        let hash1 = sha256_hex(b"test data");
        let hash2 = sha256_hex(b"test data");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn network_seed_deterministic() {
        let seed1 = derive_network_seed();
        let seed2 = derive_network_seed();
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn network_signing_key_deterministic() {
        let key1 = derive_network_signing_key();
        let key2 = derive_network_signing_key();
        assert_eq!(key1.to_bytes(), key2.to_bytes());
    }

    #[test]
    fn network_verifying_key_deterministic() {
        let pk1 = derive_network_verifying_key();
        let pk2 = derive_network_verifying_key();
        assert_eq!(pk1.as_bytes(), pk2.as_bytes());
    }

    #[test]
    fn network_id_deterministic() {
        let id1 = network_id_hex();
        let id2 = network_id_hex();
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 64);
    }

    #[test]
    fn network_key_can_sign_and_verify() {
        let sk = derive_network_signing_key();
        let pk = derive_network_verifying_key();
        let msg = b"bootstrap test";
        let sig = sign(&sk, msg);
        assert!(verify(&pk, msg, &sig).is_ok());
    }
}
