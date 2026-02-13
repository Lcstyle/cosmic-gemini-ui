use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, Write};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum CertificateError {
    #[error("Certificate is expired")]
    Expired,
    #[error("Certificate is revoked")]
    Revoked,
    #[error("Certificate is not activated yet")]
    NotActivated,
    #[error("Certificate has changed (possible MITM)")]
    BadIdentity,
    #[error("Generic certificate error")]
    GenericError,
}

pub trait KnownHostsRepo: std::fmt::Debug {
    fn get(&self, host: &str) -> Option<&str>;
    fn insert(&mut self, host: &str, sha: &str) -> bool;
    fn remove(&mut self, host: &str) -> bool;
    fn values(&self) -> HashMap<String, String>;
}

/// Validate a certificate against the known hosts store (TOFU model).
///
/// `cert_der` is the raw DER-encoded certificate bytes from rustls.
pub fn validate(
    repo: &mut impl KnownHostsRepo,
    host: &str,
    cert_der: &[u8],
) -> Result<(), CertificateError> {
    let cert_sha = hex::encode(Sha256::digest(cert_der));

    if let Some(known_host_sha) = repo.get(host) {
        if known_host_sha != cert_sha {
            return Err(CertificateError::BadIdentity);
        }
        return Ok(());
    }

    // First time seeing this host — trust on first use
    repo.insert(host, &cert_sha);
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct KnownHostsMap(HashMap<String, String>);
impl KnownHostsMap {
    pub fn new() -> Self {
        Self::default()
    }
}
impl KnownHostsRepo for KnownHostsMap {
    fn get(&self, host: &str) -> Option<&str> {
        self.0.get(host).map(|s| s.as_str())
    }

    fn insert(&mut self, host: &str, sha: &str) -> bool {
        self.0.insert(host.to_string(), sha.to_string()).is_none()
    }

    fn remove(&mut self, host: &str) -> bool {
        self.0.remove(host).is_some()
    }
    fn values(&self) -> HashMap<String, String> {
        self.0.clone()
    }
}

#[derive(Debug)]
pub struct KnownHostsFile {
    file: fs::File,
    known_hosts: KnownHostsMap,
}

impl KnownHostsFile {
    pub fn new(file: fs::File) -> Self {
        let mut known_hosts = KnownHostsMap::new();
        let mut bf = std::io::BufReader::new(file);
        let mut lines = String::new();
        bf.read_to_string(&mut lines).unwrap_or(0);
        lines.split('\n').for_each(|line| {
            let mut parts = line.split(' ');
            if let (Some(host), Some(sha)) = (parts.next(), parts.next()) {
                if !host.is_empty() {
                    known_hosts.insert(host, sha);
                }
            }
        });
        let file = bf.into_inner();
        Self { file, known_hosts }
    }

    /// Open or create the known_hosts file at the standard location.
    pub fn open_default() -> std::io::Result<Self> {
        let path = default_known_hosts_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        Ok(Self::new(file))
    }
}

/// Default path: `~/.local/share/cosmic-gemini/known_hosts`
pub fn default_known_hosts_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("cosmic-gemini/known_hosts")
}

impl KnownHostsRepo for KnownHostsFile {
    fn get(&self, host: &str) -> Option<&str> {
        self.known_hosts.get(host)
    }

    fn insert(&mut self, host: &str, sha: &str) -> bool {
        let new = self.known_hosts.insert(host, sha);
        let _ = self
            .file
            .write_all(format!("{host} {sha}\n").as_bytes());
        new
    }

    fn remove(&mut self, host: &str) -> bool {
        let r = self.known_hosts.remove(host);
        let _ = self.file.set_len(0);
        let _ = self.file.rewind();
        for (host, sha) in self.known_hosts.values() {
            let _ = self
                .file
                .write_all(format!("{host} {sha}\n").as_bytes());
        }
        r
    }

    fn values(&self) -> HashMap<String, String> {
        self.known_hosts.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tofu_first_visit() {
        let mut repo = KnownHostsMap::new();
        let cert = b"fake-cert-bytes";
        assert!(validate(&mut repo, "example.com", cert).is_ok());
        // Should now be stored
        assert!(repo.get("example.com").is_some());
    }

    #[test]
    fn tofu_same_cert() {
        let mut repo = KnownHostsMap::new();
        let cert = b"fake-cert-bytes";
        validate(&mut repo, "example.com", cert).unwrap();
        // Same cert again should be OK
        assert!(validate(&mut repo, "example.com", cert).is_ok());
    }

    #[test]
    fn tofu_changed_cert() {
        let mut repo = KnownHostsMap::new();
        validate(&mut repo, "example.com", b"cert-v1").unwrap();
        // Different cert should fail
        let result = validate(&mut repo, "example.com", b"cert-v2");
        assert_eq!(result, Err(CertificateError::BadIdentity));
    }
}
