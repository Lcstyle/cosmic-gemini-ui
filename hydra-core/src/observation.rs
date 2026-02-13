use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::HydraError;
use crate::store;

/// Raw certificate capture from a TLS handshake.
///
/// This is the bridging type from gemini-core — minimal data
/// that gets enriched into a full `CertObservation`.
#[derive(Debug, Clone)]
pub struct RawCertCapture {
    pub host: String,
    pub port: u16,
    pub certs_der: Vec<Vec<u8>>,
    pub timestamp: std::time::SystemTime,
}

/// Full certificate observation with extracted metadata.
///
/// Matches the `LocalObservation` structure from the HYDRA spec (Section 5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertObservation {
    pub domain: String,
    pub port: u16,
    /// SHA-256 of the DER-encoded leaf certificate.
    pub cert_fingerprint: String,
    /// SHA-256 fingerprints of the full certificate chain.
    pub chain_fingerprints: Vec<String>,
    pub issuer: String,
    pub subject: String,
    pub san: Vec<String>,
    pub not_before: i64,
    pub not_after: i64,
    pub observed_at: DateTime<Utc>,
    pub observer_id: String,
}

impl CertObservation {
    /// Create from a raw capture by parsing the DER certificates.
    pub fn from_raw(capture: &RawCertCapture, observer_id: &str) -> Result<Self, HydraError> {
        if capture.certs_der.is_empty() {
            return Err(HydraError::Crypto("no certificates in capture".to_string()));
        }

        let leaf_der = &capture.certs_der[0];
        let cert_fingerprint = crypto::sha256_hex(leaf_der);

        let chain_fingerprints: Vec<String> = capture
            .certs_der
            .iter()
            .map(|der| crypto::sha256_hex(der))
            .collect();

        // Parse the leaf certificate for metadata using x509-parser
        let (issuer, subject, san, not_before, not_after) = match x509_parser::parse_x509_certificate(leaf_der) {
            Ok((_, cert)) => {
                let issuer = cert.issuer().to_string();
                let subject = cert.subject().to_string();

                let san = cert
                    .subject_alternative_name()
                    .ok()
                    .flatten()
                    .map(|ext| {
                        ext.value
                            .general_names
                            .iter()
                            .filter_map(|name| match name {
                                x509_parser::extensions::GeneralName::DNSName(dns) => {
                                    Some(dns.to_string())
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let not_before = cert.validity().not_before.timestamp();
                let not_after = cert.validity().not_after.timestamp();

                (issuer, subject, san, not_before, not_after)
            }
            Err(_) => {
                // If we can't parse, use fallback values
                (
                    String::new(),
                    String::new(),
                    Vec::new(),
                    0i64,
                    0i64,
                )
            }
        };

        let observed_at: DateTime<Utc> = capture.timestamp.into();

        Ok(Self {
            domain: capture.host.clone(),
            port: capture.port,
            cert_fingerprint,
            chain_fingerprints,
            issuer,
            subject,
            san,
            not_before,
            not_after,
            observed_at,
            observer_id: observer_id.to_string(),
        })
    }
}

/// Append-only observation diary stored as JSONL.
///
/// At Stage 0, this is the only record of certificate observations.
/// At Stage 1+, observations are also shared via events, but the
/// diary provides a fast local lookup.
pub struct ObservationDiary {
    path: PathBuf,
}

const DIARY_FILENAME: &str = "observations.jsonl";

impl ObservationDiary {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(DIARY_FILENAME),
        }
    }

    pub fn default_path() -> Self {
        Self::new(&store::hydra_data_dir())
    }

    /// Append an observation to the diary.
    pub fn append(&self, obs: &CertObservation) -> Result<(), HydraError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let json = serde_json::to_string(obs)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }

    /// Load all observations from the diary.
    pub fn load_all(&self) -> Result<Vec<CertObservation>, HydraError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut observations = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let obs: CertObservation = serde_json::from_str(&line)?;
            observations.push(obs);
        }
        Ok(observations)
    }

    /// Get all observations for a specific domain.
    pub fn get_for_domain(&self, domain: &str) -> Result<Vec<CertObservation>, HydraError> {
        Ok(self
            .load_all()?
            .into_iter()
            .filter(|o| o.domain == domain)
            .collect())
    }

    /// Get the most recent observation for a domain.
    pub fn latest_for_domain(&self, domain: &str) -> Result<Option<CertObservation>, HydraError> {
        let mut observations = self.get_for_domain(domain)?;
        observations.sort_by(|a, b| b.observed_at.cmp(&a.observed_at));
        Ok(observations.into_iter().next())
    }

    /// Get the total number of observations.
    pub fn count(&self) -> Result<usize, HydraError> {
        Ok(self.load_all()?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_observation(domain: &str, fingerprint: &str) -> CertObservation {
        CertObservation {
            domain: domain.to_string(),
            port: 1965,
            cert_fingerprint: fingerprint.to_string(),
            chain_fingerprints: vec![fingerprint.to_string()],
            issuer: "CN=Test CA".to_string(),
            subject: format!("CN={}", domain),
            san: vec![domain.to_string()],
            not_before: 1000000,
            not_after: 2000000,
            observed_at: Utc::now(),
            observer_id: "test-node-id".to_string(),
        }
    }

    #[test]
    fn diary_append_and_load() {
        let tmp = TempDir::new().unwrap();
        let diary = ObservationDiary::new(tmp.path());

        let obs1 = make_observation("example.com", "aabb");
        let obs2 = make_observation("other.com", "ccdd");

        diary.append(&obs1).unwrap();
        diary.append(&obs2).unwrap();

        let all = diary.load_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].domain, "example.com");
        assert_eq!(all[1].domain, "other.com");
    }

    #[test]
    fn diary_get_for_domain() {
        let tmp = TempDir::new().unwrap();
        let diary = ObservationDiary::new(tmp.path());

        diary.append(&make_observation("a.com", "11")).unwrap();
        diary.append(&make_observation("b.com", "22")).unwrap();
        diary.append(&make_observation("a.com", "33")).unwrap();

        let a_obs = diary.get_for_domain("a.com").unwrap();
        assert_eq!(a_obs.len(), 2);

        let b_obs = diary.get_for_domain("b.com").unwrap();
        assert_eq!(b_obs.len(), 1);

        let c_obs = diary.get_for_domain("c.com").unwrap();
        assert_eq!(c_obs.len(), 0);
    }

    #[test]
    fn diary_latest_for_domain() {
        let tmp = TempDir::new().unwrap();
        let diary = ObservationDiary::new(tmp.path());

        diary.append(&make_observation("a.com", "old")).unwrap();
        diary.append(&make_observation("a.com", "new")).unwrap();

        let latest = diary.latest_for_domain("a.com").unwrap().unwrap();
        assert_eq!(latest.cert_fingerprint, "new");
    }

    #[test]
    fn diary_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let diary = ObservationDiary::new(tmp.path());
        assert_eq!(diary.load_all().unwrap().len(), 0);
        assert_eq!(diary.count().unwrap(), 0);
    }

    #[test]
    fn observation_serialization_roundtrip() {
        let obs = make_observation("test.com", "ff00");
        let json = serde_json::to_string(&obs).unwrap();
        let decoded: CertObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.domain, "test.com");
        assert_eq!(decoded.cert_fingerprint, "ff00");
    }
}
