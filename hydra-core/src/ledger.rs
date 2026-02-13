use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::HydraError;
use crate::event::{EventPayload, SignedEvent};
use crate::observation::CertObservation;
use crate::store;

/// An anomalous certificate observation for a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub fingerprint: String,
    pub issuer: String,
    pub observer_count: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    pub status: AnomalyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnomalyStatus {
    Active,
    Resolved,
    ConfirmedAttack,
}

/// Historical certificate entry for rotation tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub fingerprint: String,
    pub issuer: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub rotation_type: RotationType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RotationType {
    Normal,
    Forced,
    Suspicious,
}

/// The consensus certificate info for a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusCert {
    pub fingerprint: String,
    pub chain_fps: Vec<String>,
    pub issuer: String,
    pub not_before: i64,
    pub not_after: i64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub total_obs: u32,
    pub unique_observers: u32,
}

/// A ledger entry for a single domain.
///
/// Matches the `DomainEntry` structure from HYDRA spec Section 5.4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntry {
    pub domain: String,
    pub consensus_cert: ConsensusCert,
    pub history: Vec<HistoryEntry>,
    pub anomalies: Vec<Anomaly>,
}

/// The certificate ledger — the core application-layer data structure.
///
/// Deterministically computed from ordered events. At Stages 0-1,
/// this is recomputed from the full event log on each update.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificateLedger {
    pub entries: HashMap<String, DomainEntry>,
}

impl CertificateLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute the ledger from a sequence of ordered events.
    ///
    /// Implements the consensus rules from HYDRA spec Section 5.5.
    pub fn compute_from_events(events: &[SignedEvent]) -> Self {
        let mut ledger = Self::new();

        for event in events {
            if let EventPayload::CertObservationBatch { observations } = &event.payload {
                for obs in observations {
                    ledger.process_observation(obs);
                }
            }
        }

        ledger
    }

    /// Process a single observation, applying consensus rules.
    ///
    /// Implements Section 5.5 Cases 1-4.
    pub fn process_observation(&mut self, obs: &CertObservation) {
        let key = obs.domain.clone();

        if let Some(entry) = self.entries.get_mut(&key) {
            if obs.cert_fingerprint == entry.consensus_cert.fingerprint {
                // Case 2: matches consensus — increment counts
                entry.consensus_cert.total_obs += 1;
                entry.consensus_cert.last_seen = obs.observed_at.timestamp();
                // Track unique observers
                entry.consensus_cert.unique_observers += 1;
            } else {
                // Case 3: differs from consensus
                // Check if this fingerprint is already a known anomaly
                if let Some(anomaly) = entry
                    .anomalies
                    .iter_mut()
                    .find(|a| a.fingerprint == obs.cert_fingerprint)
                {
                    anomaly.observer_count += 1;
                    anomaly.last_seen = obs.observed_at.timestamp();

                    // Check if anomaly should become new consensus (>50% of observers)
                    let total_observers =
                        entry.consensus_cert.unique_observers + anomaly.observer_count;
                    if anomaly.observer_count > total_observers / 2 {
                        // Anomaly becomes new consensus — old consensus moves to history
                        let old_consensus = entry.consensus_cert.clone();
                        entry.history.push(HistoryEntry {
                            fingerprint: old_consensus.fingerprint,
                            issuer: old_consensus.issuer,
                            first_seen: old_consensus.first_seen,
                            last_seen: old_consensus.last_seen,
                            rotation_type: RotationType::Normal,
                        });

                        entry.consensus_cert = ConsensusCert {
                            fingerprint: obs.cert_fingerprint.clone(),
                            chain_fps: obs.chain_fingerprints.clone(),
                            issuer: obs.issuer.clone(),
                            not_before: obs.not_before,
                            not_after: obs.not_after,
                            first_seen: anomaly.first_seen,
                            last_seen: obs.observed_at.timestamp(),
                            total_obs: anomaly.observer_count,
                            unique_observers: anomaly.observer_count,
                        };

                        // Remove this fingerprint from anomalies
                        let fp = obs.cert_fingerprint.clone();
                        entry.anomalies.retain(|a| a.fingerprint != fp);
                    }
                } else {
                    // New anomaly
                    entry.anomalies.push(Anomaly {
                        fingerprint: obs.cert_fingerprint.clone(),
                        issuer: obs.issuer.clone(),
                        observer_count: 1,
                        first_seen: obs.observed_at.timestamp(),
                        last_seen: obs.observed_at.timestamp(),
                        status: AnomalyStatus::Active,
                    });
                }
            }
        } else {
            // Case 1: first observation for this domain
            self.entries.insert(
                key,
                DomainEntry {
                    domain: obs.domain.clone(),
                    consensus_cert: ConsensusCert {
                        fingerprint: obs.cert_fingerprint.clone(),
                        chain_fps: obs.chain_fingerprints.clone(),
                        issuer: obs.issuer.clone(),
                        not_before: obs.not_before,
                        not_after: obs.not_after,
                        first_seen: obs.observed_at.timestamp(),
                        last_seen: obs.observed_at.timestamp(),
                        total_obs: 1,
                        unique_observers: 1,
                    },
                    history: Vec::new(),
                    anomalies: Vec::new(),
                },
            );
        }
    }

    /// Look up a domain in the ledger.
    pub fn lookup(&self, domain: &str) -> Option<&DomainEntry> {
        self.entries.get(domain)
    }

    /// Get the total number of tracked domains.
    pub fn domain_count(&self) -> usize {
        self.entries.len()
    }

    /// Save the ledger to a JSON file (atomic write).
    pub fn save(&self, data_dir: &Path) -> Result<(), HydraError> {
        let path = data_dir.join("ledger.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| HydraError::Config(e.to_string()))?;
        store::atomic_write(&path, json.as_bytes())
    }

    /// Load the ledger from a JSON file.
    pub fn load(data_dir: &Path) -> Result<Self, HydraError> {
        let path = data_dir.join("ledger.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let ledger: Self = serde_json::from_str(&data)?;
        Ok(ledger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn obs(domain: &str, fingerprint: &str, observer: &str) -> CertObservation {
        CertObservation {
            domain: domain.to_string(),
            port: 1965,
            cert_fingerprint: fingerprint.to_string(),
            chain_fingerprints: vec![fingerprint.to_string()],
            issuer: "CN=CA".to_string(),
            subject: format!("CN={}", domain),
            san: vec![domain.to_string()],
            not_before: 1000,
            not_after: 2000,
            observed_at: Utc::now(),
            observer_id: observer.to_string(),
        }
    }

    #[test]
    fn case1_first_observation() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "aabb", "node1"));

        let entry = ledger.lookup("test.com").unwrap();
        assert_eq!(entry.consensus_cert.fingerprint, "aabb");
        assert_eq!(entry.consensus_cert.total_obs, 1);
        assert_eq!(entry.consensus_cert.unique_observers, 1);
        assert!(entry.anomalies.is_empty());
    }

    #[test]
    fn case2_matching_consensus() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "aabb", "node1"));
        ledger.process_observation(&obs("test.com", "aabb", "node2"));

        let entry = ledger.lookup("test.com").unwrap();
        assert_eq!(entry.consensus_cert.fingerprint, "aabb");
        assert_eq!(entry.consensus_cert.total_obs, 2);
        assert_eq!(entry.consensus_cert.unique_observers, 2);
    }

    #[test]
    fn case3_anomaly_recorded() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "aabb", "node1"));
        ledger.process_observation(&obs("test.com", "ccdd", "node2"));

        let entry = ledger.lookup("test.com").unwrap();
        assert_eq!(entry.consensus_cert.fingerprint, "aabb");
        assert_eq!(entry.anomalies.len(), 1);
        assert_eq!(entry.anomalies[0].fingerprint, "ccdd");
        assert_eq!(entry.anomalies[0].observer_count, 1);
    }

    #[test]
    fn case3_anomaly_becomes_consensus() {
        let mut ledger = CertificateLedger::new();
        // Original consensus
        ledger.process_observation(&obs("test.com", "old", "node1"));
        // New cert seen by node2 — creates anomaly
        ledger.process_observation(&obs("test.com", "new", "node2"));
        // New cert seen by node3 — now 2 vs 1 (>50%), becomes consensus
        ledger.process_observation(&obs("test.com", "new", "node3"));

        let entry = ledger.lookup("test.com").unwrap();
        assert_eq!(entry.consensus_cert.fingerprint, "new");
        assert_eq!(entry.history.len(), 1);
        assert_eq!(entry.history[0].fingerprint, "old");
        assert_eq!(entry.history[0].rotation_type, RotationType::Normal);
        // The old anomaly for "new" should be gone
        assert!(entry
            .anomalies
            .iter()
            .all(|a| a.fingerprint != "new"));
    }

    #[test]
    fn multiple_domains_independent() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("a.com", "aa", "node1"));
        ledger.process_observation(&obs("b.com", "bb", "node1"));

        assert_eq!(ledger.domain_count(), 2);
        assert_eq!(ledger.lookup("a.com").unwrap().consensus_cert.fingerprint, "aa");
        assert_eq!(ledger.lookup("b.com").unwrap().consensus_cert.fingerprint, "bb");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "ff", "node1"));
        ledger.save(tmp.path()).unwrap();

        let loaded = CertificateLedger::load(tmp.path()).unwrap();
        assert_eq!(loaded.domain_count(), 1);
        assert_eq!(
            loaded.lookup("test.com").unwrap().consensus_cert.fingerprint,
            "ff"
        );
    }

    #[test]
    fn load_returns_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let ledger = CertificateLedger::load(tmp.path()).unwrap();
        assert_eq!(ledger.domain_count(), 0);
    }
}
