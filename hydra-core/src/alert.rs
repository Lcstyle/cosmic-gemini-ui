use serde::{Deserialize, Serialize};

use crate::ledger::{AnomalyStatus, CertificateLedger, DomainEntry};
use crate::observation::CertObservation;

/// Alert level from HYDRA spec Section 5.7.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertLevel {
    /// GREEN — Certificate matches network consensus.
    Verified,
    /// GREY — No network data for this domain.
    Unverified,
    /// YELLOW — Certificate differs from consensus, but significant minority also reports it.
    Uncertain,
    /// RED — Very few or no other nodes report your certificate. Likely targeted attack.
    TargetedAttack,
    /// RED — Your fingerprint is brand new, not in consensus or anomalies.
    NovelDivergence,
}

/// Result of checking an observation against the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertResult {
    pub domain: String,
    pub level: AlertLevel,
    pub my_fingerprint: String,
    pub consensus_fingerprint: Option<String>,
    pub confidence: f32,
    pub observer_count: u32,
    pub message: String,
}

/// Check an observation against the certificate ledger.
///
/// Implements the `check_certificate` algorithm from HYDRA spec Section 5.6.
pub fn check_observation(ledger: &CertificateLedger, obs: &CertObservation) -> AlertResult {
    let entry = ledger.lookup(&obs.domain);

    match entry {
        None => {
            // No network data — UNVERIFIED
            AlertResult {
                domain: obs.domain.clone(),
                level: AlertLevel::Unverified,
                my_fingerprint: obs.cert_fingerprint.clone(),
                consensus_fingerprint: None,
                confidence: 0.0,
                observer_count: 0,
                message: format!(
                    "No network data for {}. Standard TLS trust applies.",
                    obs.domain
                ),
            }
        }
        Some(entry) => check_against_entry(entry, obs),
    }
}

/// Check observation against a specific domain entry.
fn check_against_entry(entry: &DomainEntry, obs: &CertObservation) -> AlertResult {
    let consensus_fp = &entry.consensus_cert.fingerprint;

    if obs.cert_fingerprint == *consensus_fp {
        // VERIFIED — matches consensus
        let confidence = if entry.consensus_cert.unique_observers > 0 {
            entry.consensus_cert.unique_observers as f32
        } else {
            1.0
        };

        return AlertResult {
            domain: obs.domain.clone(),
            level: AlertLevel::Verified,
            my_fingerprint: obs.cert_fingerprint.clone(),
            consensus_fingerprint: Some(consensus_fp.clone()),
            confidence,
            observer_count: entry.consensus_cert.unique_observers,
            message: format!(
                "Certificate matches network consensus. {} observer(s) confirm.",
                entry.consensus_cert.unique_observers
            ),
        };
    }

    // Check if fingerprint is a known anomaly
    if let Some(anomaly) = entry
        .anomalies
        .iter()
        .find(|a| a.fingerprint == obs.cert_fingerprint && a.status == AnomalyStatus::Active)
    {
        let total_observers = entry.consensus_cert.unique_observers + anomaly.observer_count;
        let anomaly_fraction = if total_observers > 0 {
            anomaly.observer_count as f32 / total_observers as f32
        } else {
            0.0
        };

        if anomaly_fraction > 0.1 {
            // >10% see this cert — UNCERTAIN (could be CDN variation)
            return AlertResult {
                domain: obs.domain.clone(),
                level: AlertLevel::Uncertain,
                my_fingerprint: obs.cert_fingerprint.clone(),
                consensus_fingerprint: Some(consensus_fp.clone()),
                confidence: anomaly_fraction,
                observer_count: anomaly.observer_count,
                message: format!(
                    "Certificate differs from consensus for {}. {} observer(s) also report your certificate. Could be CDN variation.",
                    obs.domain, anomaly.observer_count
                ),
            };
        } else {
            // <10% see this cert — TARGETED ATTACK
            return AlertResult {
                domain: obs.domain.clone(),
                level: AlertLevel::TargetedAttack,
                my_fingerprint: obs.cert_fingerprint.clone(),
                consensus_fingerprint: Some(consensus_fp.clone()),
                confidence: anomaly_fraction,
                observer_count: anomaly.observer_count,
                message: format!(
                    "WARNING: The certificate for {} does not match what {} other observer(s) are seeing. You may be experiencing a targeted MITM attack.",
                    obs.domain, entry.consensus_cert.unique_observers
                ),
            };
        }
    }

    // Fingerprint not in consensus or anomalies — NOVEL DIVERGENCE
    AlertResult {
        domain: obs.domain.clone(),
        level: AlertLevel::NovelDivergence,
        my_fingerprint: obs.cert_fingerprint.clone(),
        consensus_fingerprint: Some(consensus_fp.clone()),
        confidence: 0.0,
        observer_count: 0,
        message: format!(
            "WARNING: The certificate for {} is completely new — no other observer has reported it. Consensus fingerprint: {}",
            obs.domain,
            &consensus_fp[..consensus_fp.len().min(16)]
        ),
    }
}

/// Check an observation against the local diary (Stage 0 — no peers).
///
/// Simple comparison: does the current cert match the last seen cert?
pub fn check_against_diary(
    obs: &CertObservation,
    diary_history: &[CertObservation],
) -> AlertResult {
    if diary_history.is_empty() {
        return AlertResult {
            domain: obs.domain.clone(),
            level: AlertLevel::Unverified,
            my_fingerprint: obs.cert_fingerprint.clone(),
            consensus_fingerprint: None,
            confidence: 0.0,
            observer_count: 0,
            message: format!("First visit to {} — recorded in personal diary.", obs.domain),
        };
    }

    // Find the most recent previous observation
    let latest = diary_history
        .iter()
        .max_by_key(|o| o.observed_at)
        .unwrap();

    if obs.cert_fingerprint == latest.cert_fingerprint {
        AlertResult {
            domain: obs.domain.clone(),
            level: AlertLevel::Verified,
            my_fingerprint: obs.cert_fingerprint.clone(),
            consensus_fingerprint: Some(latest.cert_fingerprint.clone()),
            confidence: 1.0,
            observer_count: 1,
            message: format!(
                "Certificate matches your previous observation for {}.",
                obs.domain
            ),
        }
    } else {
        AlertResult {
            domain: obs.domain.clone(),
            level: AlertLevel::NovelDivergence,
            my_fingerprint: obs.cert_fingerprint.clone(),
            consensus_fingerprint: Some(latest.cert_fingerprint.clone()),
            confidence: 0.0,
            observer_count: 1,
            message: format!(
                "Certificate for {} has changed since your last visit. Previous: {}...",
                obs.domain,
                &latest.cert_fingerprint[..latest.cert_fingerprint.len().min(16)]
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::CertificateLedger;
    use chrono::Utc;

    fn obs(domain: &str, fp: &str) -> CertObservation {
        CertObservation {
            domain: domain.to_string(),
            port: 1965,
            cert_fingerprint: fp.to_string(),
            chain_fingerprints: vec![fp.to_string()],
            issuer: "CN=CA".to_string(),
            subject: format!("CN={}", domain),
            san: vec![domain.to_string()],
            not_before: 1000,
            not_after: 2000,
            observed_at: Utc::now(),
            observer_id: "me".to_string(),
        }
    }

    #[test]
    fn unverified_when_no_data() {
        let ledger = CertificateLedger::new();
        let result = check_observation(&ledger, &obs("unknown.com", "ff"));
        assert_eq!(result.level, AlertLevel::Unverified);
    }

    #[test]
    fn verified_when_matches_consensus() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "aabb"));
        ledger.process_observation(&obs("test.com", "aabb"));

        let result = check_observation(&ledger, &obs("test.com", "aabb"));
        assert_eq!(result.level, AlertLevel::Verified);
        assert_eq!(result.observer_count, 2);
    }

    #[test]
    fn novel_divergence_when_unknown_fp() {
        let mut ledger = CertificateLedger::new();
        ledger.process_observation(&obs("test.com", "aabb"));

        let result = check_observation(&ledger, &obs("test.com", "xxxx"));
        assert_eq!(result.level, AlertLevel::NovelDivergence);
    }

    #[test]
    fn targeted_attack_when_few_see_anomaly() {
        let mut ledger = CertificateLedger::new();
        // Build consensus with many observers
        for _ in 0..20 {
            ledger.process_observation(&obs("test.com", "good"));
        }
        // One observer sees a different cert
        ledger.process_observation(&obs("test.com", "evil"));

        let result = check_observation(&ledger, &obs("test.com", "evil"));
        assert_eq!(result.level, AlertLevel::TargetedAttack);
    }

    #[test]
    fn uncertain_when_significant_minority() {
        let mut ledger = CertificateLedger::new();
        // 5 see cert A
        for _ in 0..5 {
            ledger.process_observation(&obs("test.com", "certA"));
        }
        // 3 see cert B (>10% = 3/(5+3) = 37.5%)
        for _ in 0..3 {
            ledger.process_observation(&obs("test.com", "certB"));
        }

        let result = check_observation(&ledger, &obs("test.com", "certB"));
        assert_eq!(result.level, AlertLevel::Uncertain);
    }

    #[test]
    fn diary_first_visit() {
        let result = check_against_diary(&obs("new.com", "ff"), &[]);
        assert_eq!(result.level, AlertLevel::Unverified);
    }

    #[test]
    fn diary_same_cert() {
        let history = vec![obs("test.com", "same")];
        let result = check_against_diary(&obs("test.com", "same"), &history);
        assert_eq!(result.level, AlertLevel::Verified);
    }

    #[test]
    fn diary_changed_cert() {
        let history = vec![obs("test.com", "old_cert_hash_1234")];
        let result = check_against_diary(&obs("test.com", "new_cert_hash"), &history);
        assert_eq!(result.level, AlertLevel::NovelDivergence);
    }
}
