use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub expires_at: String,
    pub common_name: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdentityConfig {
    pub identities: Vec<Identity>,
    pub host_bindings: HashMap<String, String>,
}

fn identities_dir() -> PathBuf {
    store::data_dir().join("identities")
}

fn config_path() -> PathBuf {
    identities_dir().join("config.json")
}

fn identity_dir(id: &str) -> PathBuf {
    identities_dir().join(id)
}

fn load_config() -> IdentityConfig {
    let path = config_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&data) {
                return config;
            }
        }
    }
    IdentityConfig::default()
}

fn save_config(config: &IdentityConfig) -> Result<(), String> {
    let dir = identities_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create identities dir: {}", e))?;

    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("Failed to serialize: {}", e))?;

    let tmp_path = config_path().with_extension("tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Failed to write config: {}", e))?;
    fs::rename(&tmp_path, config_path()).map_err(|e| format!("Failed to rename config: {}", e))?;

    Ok(())
}

/// Generate a new self-signed client certificate identity.
pub fn generate_identity(name: &str, duration_days: u64) -> Result<Identity, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::days(duration_days as i64);

    let key_pair = KeyPair::generate().map_err(|e| format!("Key generation failed: {}", e))?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, name);
    params.distinguished_name = dn;
    params.not_before = rcgen::date_time_ymd(now.year() as i32, now.month() as u8, now.day() as u8);
    params.not_after = rcgen::date_time_ymd(
        expires.year() as i32,
        expires.month() as u8,
        expires.day() as u8,
    );

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("Certificate generation failed: {}", e))?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    // Compute fingerprint from DER
    let cert_der = cert.der();
    let fingerprint = hex::encode(Sha256::digest(cert_der.as_ref()));

    // Store files
    let dir = identity_dir(&id);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create identity dir: {}", e))?;
    fs::write(dir.join("cert.pem"), &cert_pem)
        .map_err(|e| format!("Failed to write cert: {}", e))?;
    fs::write(dir.join("key.pem"), &key_pem)
        .map_err(|e| format!("Failed to write key: {}", e))?;

    let identity = Identity {
        id: id.clone(),
        name: name.to_string(),
        created_at: now.to_rfc3339(),
        expires_at: expires.to_rfc3339(),
        common_name: name.to_string(),
        fingerprint,
    };

    // Update config
    let mut config = load_config();
    config.identities.push(identity.clone());
    save_config(&config)?;

    Ok(identity)
}

/// Import an existing PEM certificate and key.
pub fn import_identity(name: &str, cert_pem: &str, key_pem: &str) -> Result<Identity, String> {
    // Validate the PEM data by parsing
    let cert_der = parse_cert_pem(cert_pem)?;
    let _ = parse_key_pem(key_pem)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let fingerprint = hex::encode(Sha256::digest(cert_der.first().unwrap().as_ref()));

    // Store files
    let dir = identity_dir(&id);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create identity dir: {}", e))?;
    fs::write(dir.join("cert.pem"), cert_pem)
        .map_err(|e| format!("Failed to write cert: {}", e))?;
    fs::write(dir.join("key.pem"), key_pem)
        .map_err(|e| format!("Failed to write key: {}", e))?;

    let identity = Identity {
        id: id.clone(),
        name: name.to_string(),
        created_at: now.to_rfc3339(),
        expires_at: String::new(), // Unknown for imported certs
        common_name: name.to_string(),
        fingerprint,
    };

    let mut config = load_config();
    config.identities.push(identity.clone());
    save_config(&config)?;

    Ok(identity)
}

/// Load a stored identity's cert chain and private key for TLS use.
pub fn load_identity(
    id: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
    let dir = identity_dir(id);
    let cert_pem =
        fs::read_to_string(dir.join("cert.pem")).map_err(|e| format!("Read cert: {}", e))?;
    let key_pem =
        fs::read_to_string(dir.join("key.pem")).map_err(|e| format!("Read key: {}", e))?;

    let certs = parse_cert_pem(&cert_pem)?;
    let key = parse_key_pem(&key_pem)?;

    Ok((certs, key))
}

/// List all known identities.
pub fn list_identities() -> Vec<Identity> {
    load_config().identities
}

/// Delete an identity and its files.
pub fn delete_identity(id: &str) -> Result<(), String> {
    let mut config = load_config();
    config.identities.retain(|i| i.id != id);
    // Remove any host bindings pointing to this identity
    config.host_bindings.retain(|_, v| v != id);
    save_config(&config)?;

    let dir = identity_dir(id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }

    Ok(())
}

/// Bind an identity to a hostname for automatic presentation.
pub fn bind_host(hostname: &str, identity_id: &str) -> Result<(), String> {
    let mut config = load_config();
    config
        .host_bindings
        .insert(hostname.to_string(), identity_id.to_string());
    save_config(&config)
}

/// Remove a host binding.
pub fn unbind_host(hostname: &str) -> Result<(), String> {
    let mut config = load_config();
    config.host_bindings.remove(hostname);
    save_config(&config)
}

/// Get the identity ID bound to a hostname, if any.
pub fn get_host_binding(hostname: &str) -> Option<String> {
    let config = load_config();
    config.host_bindings.get(hostname).cloned()
}

/// Get all host bindings for a given identity.
pub fn get_bindings_for_identity(identity_id: &str) -> Vec<String> {
    let config = load_config();
    config
        .host_bindings
        .iter()
        .filter(|(_, v)| v.as_str() == identity_id)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Export a certificate PEM for a given identity.
pub fn export_cert_pem(id: &str) -> Result<String, String> {
    let dir = identity_dir(id);
    fs::read_to_string(dir.join("cert.pem")).map_err(|e| format!("Read cert: {}", e))
}

// -- PEM parsing helpers --

fn parse_cert_pem(pem: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Invalid certificate PEM: {}", e))?;
    if certs.is_empty() {
        return Err("No certificates found in PEM data".to_string());
    }
    Ok(certs)
}

fn parse_key_pem(pem: &str) -> Result<PrivateKeyDer<'static>, String> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("Invalid key PEM: {}", e))?
        .ok_or_else(|| "No private key found in PEM data".to_string())
}

// -- Trait imports for chrono --
use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_load_identity() {
        let id = generate_identity("Test User", 365).unwrap();
        assert!(!id.id.is_empty());
        assert_eq!(id.name, "Test User");
        assert!(!id.fingerprint.is_empty());

        let (certs, _key) = load_identity(&id.id).unwrap();
        assert!(!certs.is_empty());

        // Cleanup
        delete_identity(&id.id).unwrap();
    }

    #[test]
    fn host_binding() {
        let id = generate_identity("Binding Test", 365).unwrap();
        bind_host("example.com", &id.id).unwrap();
        assert_eq!(get_host_binding("example.com"), Some(id.id.clone()));

        unbind_host("example.com").unwrap();
        assert_eq!(get_host_binding("example.com"), None);

        delete_identity(&id.id).unwrap();
    }
}
