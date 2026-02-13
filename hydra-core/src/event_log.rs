use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::error::HydraError;
use crate::event::{self, SignedEvent};
use crate::store;

/// Append-only signed event log stored as JSONL.
///
/// Each line is a JSON-serialized `SignedEvent`. The log maintains
/// a hash chain: each event's `prev_hash` must equal the hash of
/// the preceding event.
pub struct EventLog {
    path: PathBuf,
    /// Cached events (loaded on init).
    events: Vec<SignedEvent>,
    /// Hash of the last event (for chain linking).
    last_hash: String,
}

const LOG_FILENAME: &str = "events.jsonl";

impl EventLog {
    /// Create a new event log at the given directory.
    pub fn new(data_dir: &Path) -> Result<Self, HydraError> {
        let path = data_dir.join(LOG_FILENAME);
        let mut log = Self {
            path,
            events: Vec::new(),
            last_hash: event::genesis_prev_hash(),
        };
        log.load()?;
        Ok(log)
    }

    /// Create at the default HYDRA data directory.
    pub fn default_path() -> Result<Self, HydraError> {
        store::ensure_dir()?;
        Self::new(&store::hydra_data_dir())
    }

    /// Load events from disk into memory.
    fn load(&mut self) -> Result<(), HydraError> {
        if !self.path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let evt: SignedEvent = serde_json::from_str(&line).map_err(|e| {
                HydraError::LogIntegrity(format!("failed to parse event at line {}: {}", i + 1, e))
            })?;
            events.push(evt);
        }

        // Update last_hash from loaded events
        if let Some(last) = events.last() {
            self.last_hash = event::event_hash(last);
        }

        self.events = events;
        Ok(())
    }

    /// Append a signed event to the log.
    ///
    /// The event's `prev_hash` must match the hash of the last event
    /// in the log (or the genesis zero hash if the log is empty).
    pub fn append(&mut self, evt: SignedEvent) -> Result<(), HydraError> {
        // Verify prev_hash chain integrity
        if evt.prev_hash != self.last_hash {
            return Err(HydraError::LogIntegrity(format!(
                "prev_hash mismatch: expected {}, got {}",
                self.last_hash, evt.prev_hash
            )));
        }

        // Verify the event signature
        event::verify_event(&evt)?;

        // Persist to disk
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let json = serde_json::to_string(&evt)?;
        writeln!(file, "{}", json)?;

        // Update in-memory state
        self.last_hash = event::event_hash(&evt);
        self.events.push(evt);

        Ok(())
    }

    /// Get all events.
    pub fn events(&self) -> &[SignedEvent] {
        &self.events
    }

    /// Get the number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the hash of the last event (for chain linking).
    pub fn last_hash(&self) -> &str {
        &self.last_hash
    }

    /// Get events since a given sequence number (exclusive).
    ///
    /// Returns all events with sequence > `since_sequence`.
    pub fn events_since(&self, since_sequence: u64) -> Vec<&SignedEvent> {
        self.events
            .iter()
            .filter(|e| e.sequence > since_sequence)
            .collect()
    }

    /// Verify the entire hash chain integrity.
    ///
    /// Checks that:
    /// 1. Each event's signature is valid
    /// 2. Each event's prev_hash matches the hash of the previous event
    /// 3. Sequence numbers are monotonically increasing per author
    pub fn verify_chain(&self) -> Result<(), HydraError> {
        let mut expected_hash = event::genesis_prev_hash();

        for (i, evt) in self.events.iter().enumerate() {
            // Verify signature
            event::verify_event(evt).map_err(|e| {
                HydraError::LogIntegrity(format!("invalid signature at event {}: {}", i, e))
            })?;

            // Verify hash chain
            if evt.prev_hash != expected_hash {
                return Err(HydraError::LogIntegrity(format!(
                    "hash chain broken at event {}: expected {}, got {}",
                    i, expected_hash, evt.prev_hash
                )));
            }

            expected_hash = event::event_hash(evt);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::event::{EventPayload, genesis_prev_hash, sign_event};
    use tempfile::TempDir;

    fn make_genesis(key: &ed25519_dalek::SigningKey, prev: &str) -> SignedEvent {
        sign_event(
            key,
            0,
            EventPayload::Genesis {
                network_id: crypto::network_id_hex(),
                creator_pubkey: hex::encode(key.verifying_key().as_bytes()),
                creator_onion: "test.onion".to_string(),
            },
            prev,
        )
    }

    #[test]
    fn append_and_load() {
        let tmp = TempDir::new().unwrap();
        let key = crypto::generate_keypair();

        {
            let mut log = EventLog::new(tmp.path()).unwrap();
            assert!(log.is_empty());

            let genesis = make_genesis(&key, &genesis_prev_hash());
            log.append(genesis).unwrap();
            assert_eq!(log.len(), 1);

            let evt1 = sign_event(
                &key,
                1,
                EventPayload::MemberJoin {
                    node_id: "peer1".to_string(),
                    onion_address: "p1.onion".to_string(),
                },
                log.last_hash(),
            );
            log.append(evt1).unwrap();
            assert_eq!(log.len(), 2);
        }

        // Reload from disk
        let log2 = EventLog::new(tmp.path()).unwrap();
        assert_eq!(log2.len(), 2);
        assert_eq!(log2.events()[0].sequence, 0);
        assert_eq!(log2.events()[1].sequence, 1);
    }

    #[test]
    fn rejects_broken_hash_chain() {
        let tmp = TempDir::new().unwrap();
        let key = crypto::generate_keypair();

        let mut log = EventLog::new(tmp.path()).unwrap();
        let genesis = make_genesis(&key, &genesis_prev_hash());
        log.append(genesis).unwrap();

        // Try to append an event with wrong prev_hash
        let bad_event = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "peer".to_string(),
                onion_address: "p.onion".to_string(),
            },
            "wrong_hash",
        );

        assert!(log.append(bad_event).is_err());
    }

    #[test]
    fn verify_chain_passes_for_valid_log() {
        let tmp = TempDir::new().unwrap();
        let key = crypto::generate_keypair();

        let mut log = EventLog::new(tmp.path()).unwrap();
        let genesis = make_genesis(&key, &genesis_prev_hash());
        log.append(genesis).unwrap();

        let evt1 = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "p".to_string(),
                onion_address: "p.onion".to_string(),
            },
            log.last_hash(),
        );
        log.append(evt1).unwrap();

        assert!(log.verify_chain().is_ok());
    }

    #[test]
    fn events_since_filters_correctly() {
        let tmp = TempDir::new().unwrap();
        let key = crypto::generate_keypair();

        let mut log = EventLog::new(tmp.path()).unwrap();

        let genesis = make_genesis(&key, &genesis_prev_hash());
        log.append(genesis).unwrap();

        let evt1 = sign_event(
            &key,
            1,
            EventPayload::MemberJoin {
                node_id: "a".to_string(),
                onion_address: "a.onion".to_string(),
            },
            log.last_hash(),
        );
        log.append(evt1).unwrap();

        let evt2 = sign_event(
            &key,
            2,
            EventPayload::MemberJoin {
                node_id: "b".to_string(),
                onion_address: "b.onion".to_string(),
            },
            log.last_hash(),
        );
        log.append(evt2).unwrap();

        let since_0 = log.events_since(0);
        assert_eq!(since_0.len(), 2); // events 1 and 2

        let since_1 = log.events_since(1);
        assert_eq!(since_1.len(), 1); // event 2 only

        let since_2 = log.events_since(2);
        assert_eq!(since_2.len(), 0);
    }

    #[test]
    fn empty_log_verify_chain() {
        let tmp = TempDir::new().unwrap();
        let log = EventLog::new(tmp.path()).unwrap();
        assert!(log.verify_chain().is_ok());
    }
}
