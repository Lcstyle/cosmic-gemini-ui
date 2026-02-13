use crate::crypto;
use crate::error::HydraError;
use crate::event::{self, SignedEvent};

/// Merge remote events into the local event set.
///
/// At Stage 1 (bilateral sync), events are ordered by timestamp
/// with deterministic tie-breaking: if timestamps are equal,
/// the event with the lexicographically lower `sha256(event_signature)`
/// is ordered first (HYDRA spec Section 6.1).
///
/// Returns the merged, ordered list of all events (local + remote).
pub fn merge_events(
    local_events: &[SignedEvent],
    remote_events: &[SignedEvent],
) -> Result<Vec<SignedEvent>, HydraError> {
    // Validate all remote events
    for evt in remote_events {
        event::verify_event(evt).map_err(|e| {
            HydraError::LogIntegrity(format!(
                "invalid remote event (seq={}, author={}): {}",
                evt.sequence,
                &evt.author[..evt.author.len().min(8)],
                e
            ))
        })?;
    }

    // Collect all events, dedup by hash
    let mut all_events: Vec<SignedEvent> = Vec::new();
    let mut seen_hashes = std::collections::HashSet::new();

    for evt in local_events.iter().chain(remote_events.iter()) {
        let hash = event::event_hash(evt);
        if seen_hashes.insert(hash) {
            all_events.push(evt.clone());
        }
    }

    // Sort by timestamp, with deterministic tie-breaking
    all_events.sort_by(|a, b| {
        let time_cmp = a.timestamp.cmp(&b.timestamp);
        if time_cmp == std::cmp::Ordering::Equal {
            // Tie-break: lower sha256(signature) wins
            let hash_a = crypto::sha256_hex(a.signature.as_bytes());
            let hash_b = crypto::sha256_hex(b.signature.as_bytes());
            hash_a.cmp(&hash_b)
        } else {
            time_cmp
        }
    });

    Ok(all_events)
}

/// Find events that the remote is missing.
///
/// Returns local events that are not in the remote set
/// (determined by event hash comparison).
pub fn events_to_send(
    local_events: &[SignedEvent],
    remote_events: &[SignedEvent],
) -> Vec<SignedEvent> {
    let remote_hashes: std::collections::HashSet<String> = remote_events
        .iter()
        .map(|e| event::event_hash(e))
        .collect();

    local_events
        .iter()
        .filter(|e| !remote_hashes.contains(&event::event_hash(e)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_keypair;
    use crate::event::{EventPayload, genesis_prev_hash, sign_event};

    #[test]
    fn merge_deduplicates() {
        let key = generate_keypair();
        let evt = sign_event(
            &key,
            0,
            EventPayload::Genesis {
                network_id: "net".to_string(),
                creator_pubkey: "pk".to_string(),
                creator_onion: "o.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        // Same event in both local and remote
        let merged = merge_events(&[evt.clone()], &[evt]).unwrap();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_combines_different_events() {
        let key1 = generate_keypair();
        let key2 = generate_keypair();

        let evt1 = sign_event(
            &key1,
            0,
            EventPayload::Genesis {
                network_id: "net".to_string(),
                creator_pubkey: "pk1".to_string(),
                creator_onion: "o1.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        let evt2 = sign_event(
            &key2,
            0,
            EventPayload::MemberJoin {
                node_id: "peer".to_string(),
                onion_address: "p.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        let merged = merge_events(&[evt1], &[evt2]).unwrap();
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_sorts_by_timestamp() {
        let key = generate_keypair();

        // Create events with different timestamps (they'll be slightly different due to Utc::now())
        let evt1 = sign_event(
            &key,
            0,
            EventPayload::Genesis {
                network_id: "net".to_string(),
                creator_pubkey: "pk".to_string(),
                creator_onion: "o.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        let evt2 = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "p".to_string(),
                onion_address: "p.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        let merged = merge_events(&[evt2.clone()], &[evt1.clone()]).unwrap();
        assert_eq!(merged.len(), 2);
        // Earlier timestamp should be first
        assert!(merged[0].timestamp <= merged[1].timestamp);
    }

    #[test]
    fn merge_rejects_invalid_signature() {
        let key = generate_keypair();
        let mut evt = sign_event(
            &key,
            0,
            EventPayload::Genesis {
                network_id: "net".to_string(),
                creator_pubkey: "pk".to_string(),
                creator_onion: "o.onion".to_string(),
            },
            &genesis_prev_hash(),
        );
        // Tamper
        evt.sequence = 99;

        let result = merge_events(&[], &[evt]);
        assert!(result.is_err());
    }

    #[test]
    fn events_to_send_finds_missing() {
        let key = generate_keypair();

        let evt1 = sign_event(
            &key,
            0,
            EventPayload::Genesis {
                network_id: "n".to_string(),
                creator_pubkey: "p".to_string(),
                creator_onion: "o.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        let evt2 = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "x".to_string(),
                onion_address: "x.onion".to_string(),
            },
            &genesis_prev_hash(),
        );

        // Remote only has evt1, so we should send evt2
        let to_send = events_to_send(&[evt1.clone(), evt2.clone()], &[evt1]);
        assert_eq!(to_send.len(), 1);
        assert_eq!(to_send[0].sequence, 1);
    }
}
