use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::alert::{self, AlertResult};
use crate::config::HydraConfig;
use crate::crypto;
use crate::error::HydraError;
use crate::event::{self, EventPayload};
use crate::event_log::EventLog;
use crate::identity::NodeIdentity;
use crate::ledger::CertificateLedger;
use crate::network_identity::NetworkIdentity;
use crate::observation::{CertObservation, ObservationDiary, RawCertCapture};
use crate::peer::PeerList;
use crate::store;

/// Commands sent to the HydraNode.
#[derive(Debug)]
pub enum HydraCommand {
    Shutdown,
    GetStatus,
    AddPeer { onion_address: String },
    RemovePeer { node_id: String },
    ManualSync,
    ToggleEnabled,
}

/// Notifications sent from the HydraNode to the UI.
#[derive(Debug, Clone)]
pub enum HydraNotification {
    Started { node_id: String },
    StatusUpdate(HydraStatus),
    Alert(AlertResult),
    ObservationRecorded { domain: String },
    SyncComplete { peer_id: String, events_exchanged: usize },
    Error(String),
}

/// Current status of the HYDRA node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraStatus {
    pub node_id: String,
    pub enabled: bool,
    pub stage: u8,
    pub observation_count: usize,
    pub event_count: usize,
    pub peer_count: usize,
    pub domain_count: usize,
    pub last_sync: Option<DateTime<Utc>>,
    pub alert_count: usize,
    pub network_id: String,
}

/// Handle to a running HydraNode — used by the app to send commands.
pub struct HydraHandle {
    pub cmd_tx: mpsc::Sender<HydraCommand>,
    pub join_handle: tokio::task::JoinHandle<()>,
}

impl HydraHandle {
    /// Send a command to the node.
    pub async fn send(&self, cmd: HydraCommand) -> Result<(), HydraError> {
        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|e| HydraError::Transport(format!("node channel closed: {}", e)))
    }
}

/// The HYDRA background node.
///
/// Runs as a long-lived tokio task. Processes certificate observations,
/// maintains the event log and ledger, and handles peer sync.
pub struct HydraNode {
    identity: NodeIdentity,
    network_identity: NetworkIdentity,
    config: HydraConfig,
    data_dir: PathBuf,
    event_log: EventLog,
    diary: ObservationDiary,
    ledger: CertificateLedger,
    peers: PeerList,
    alerts: Vec<AlertResult>,
    sequence: u64,
}

impl HydraNode {
    /// Start the HYDRA node as a background task.
    ///
    /// Returns a handle for sending commands and a notification receiver.
    pub async fn start(
        cert_capture_rx: broadcast::Receiver<RawCertCapture>,
        notification_tx: broadcast::Sender<HydraNotification>,
    ) -> Result<HydraHandle, HydraError> {
        store::ensure_dir()?;
        let data_dir = store::hydra_data_dir();

        let identity = NodeIdentity::load_or_generate(&data_dir)?;
        let network_identity = NetworkIdentity::derive();
        let config = HydraConfig::load()?;
        let event_log = EventLog::new(&data_dir)?;
        let diary = ObservationDiary::new(&data_dir);
        let ledger = CertificateLedger::load(&data_dir)?;
        let peers = PeerList::load(&data_dir)?;

        let sequence = event_log
            .events()
            .iter()
            .filter(|e| e.author == identity.node_id)
            .map(|e| e.sequence)
            .max()
            .map(|s| s + 1)
            .unwrap_or(0);

        let node_id = identity.node_id.clone();
        let net_id = network_identity.network_id_hex.clone();

        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let mut node = HydraNode {
            identity,
            network_identity,
            config,
            data_dir,
            event_log,
            diary,
            ledger,
            peers,
            alerts: Vec::new(),
            sequence,
        };

        // Create genesis event if this is a fresh node
        if node.event_log.is_empty() {
            node.create_genesis_event()?;
        }

        let notif_tx = notification_tx.clone();
        let join_handle = tokio::spawn(async move {
            let _ = notif_tx.send(HydraNotification::Started {
                node_id: node.identity.node_id.clone(),
            });
            node.run(cmd_rx, cert_capture_rx, notif_tx).await;
        });

        log::info!(
            "HYDRA node started: id={}, network={}",
            &node_id[..node_id.len().min(8)],
            &net_id[..net_id.len().min(8)]
        );

        Ok(HydraHandle {
            cmd_tx,
            join_handle,
        })
    }

    fn create_genesis_event(&mut self) -> Result<(), HydraError> {
        let payload = EventPayload::Genesis {
            network_id: self.network_identity.network_id_hex.clone(),
            creator_pubkey: self.identity.node_id.clone(),
            creator_onion: String::new(), // Will be set after Tor bootstrap
        };

        let event = event::sign_event(
            &self.identity.signing_key,
            self.sequence,
            payload,
            self.event_log.last_hash(),
        );

        self.event_log.append(event)?;
        self.sequence += 1;
        Ok(())
    }

    async fn run(
        &mut self,
        mut cmd_rx: mpsc::Receiver<HydraCommand>,
        mut cert_rx: broadcast::Receiver<RawCertCapture>,
        notif_tx: broadcast::Sender<HydraNotification>,
    ) {
        let sync_interval = tokio::time::Duration::from_secs(self.config.sync_interval_secs);
        let mut sync_timer = tokio::time::interval(sync_interval);
        sync_timer.tick().await; // Skip first immediate tick

        loop {
            tokio::select! {
                // Handle commands from the UI
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(HydraCommand::Shutdown) | None => {
                            log::info!("HYDRA node shutting down");
                            break;
                        }
                        Some(HydraCommand::GetStatus) => {
                            let status = self.build_status();
                            let _ = notif_tx.send(HydraNotification::StatusUpdate(status));
                        }
                        Some(HydraCommand::AddPeer { onion_address }) => {
                            self.handle_add_peer(&onion_address, &notif_tx);
                        }
                        Some(HydraCommand::RemovePeer { node_id }) => {
                            self.peers.remove(&node_id);
                            let _ = self.peers.save(&self.data_dir);
                            let status = self.build_status();
                            let _ = notif_tx.send(HydraNotification::StatusUpdate(status));
                        }
                        Some(HydraCommand::ManualSync) => {
                            // Sync would go through Tor — placeholder for now
                            log::info!("Manual sync requested (not yet connected to Tor)");
                        }
                        Some(HydraCommand::ToggleEnabled) => {
                            self.config.enabled = !self.config.enabled;
                            let _ = self.config.save();
                            let status = self.build_status();
                            let _ = notif_tx.send(HydraNotification::StatusUpdate(status));
                        }
                    }
                }

                // Handle certificate captures from browsing
                capture = cert_rx.recv() => {
                    match capture {
                        Ok(raw) => {
                            self.handle_cert_capture(raw, &notif_tx);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("HYDRA: dropped {} cert captures (lagging)", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            log::info!("Cert capture channel closed");
                            break;
                        }
                    }
                }

                // Periodic sync timer (Stage 1 only)
                _ = sync_timer.tick() => {
                    if self.config.enabled && self.peers.count() > 0 {
                        log::debug!("Sync timer fired ({} peers)", self.peers.count());
                        // Tor-based sync would happen here
                    }
                }
            }
        }

        // Save state on exit
        let _ = self.ledger.save(&self.data_dir);
        let _ = self.peers.save(&self.data_dir);
        let _ = self.config.save();
    }

    fn handle_cert_capture(
        &mut self,
        raw: RawCertCapture,
        notif_tx: &broadcast::Sender<HydraNotification>,
    ) {
        let domain = raw.host.clone();

        // Convert raw capture to observation
        let obs = match CertObservation::from_raw(&raw, &self.identity.node_id) {
            Ok(obs) => obs,
            Err(e) => {
                log::warn!("Failed to process cert capture for {}: {}", domain, e);
                return;
            }
        };

        // Record in diary
        if let Err(e) = self.diary.append(&obs) {
            log::warn!("Failed to write observation diary: {}", e);
        }

        // Check alert (Stage 0: against diary, Stage 1: against ledger)
        let alert_result = if self.peers.count() > 0 {
            alert::check_observation(&self.ledger, &obs)
        } else {
            let history = self.diary.get_for_domain(&obs.domain).unwrap_or_default();
            alert::check_against_diary(&obs, &history)
        };

        // Store alert if interesting
        if alert_result.level != crate::alert::AlertLevel::Verified
            && alert_result.level != crate::alert::AlertLevel::Unverified
        {
            self.alerts.push(alert_result.clone());
            let _ = notif_tx.send(HydraNotification::Alert(alert_result));
        }

        // Create event
        let payload = EventPayload::CertObservationBatch {
            observations: vec![obs],
        };

        let event = event::sign_event(
            &self.identity.signing_key,
            self.sequence,
            payload,
            self.event_log.last_hash(),
        );

        if let Err(e) = self.event_log.append(event) {
            log::warn!("Failed to append event: {}", e);
            return;
        }
        self.sequence += 1;

        // Recompute ledger
        self.ledger = CertificateLedger::compute_from_events(self.event_log.events());
        let _ = self.ledger.save(&self.data_dir);

        let _ = notif_tx.send(HydraNotification::ObservationRecorded { domain });
    }

    fn handle_add_peer(
        &mut self,
        onion_address: &str,
        notif_tx: &broadcast::Sender<HydraNotification>,
    ) {
        // Use the onion address as a temporary node_id until we learn the real one
        let temp_id = crypto::sha256_hex(onion_address.as_bytes());
        self.peers.upsert(&temp_id, onion_address);
        let _ = self.peers.save(&self.data_dir);

        let status = self.build_status();
        let _ = notif_tx.send(HydraNotification::StatusUpdate(status));
        log::info!("Added peer: {}", onion_address);
    }

    fn build_status(&self) -> HydraStatus {
        let stage = if self.peers.count() == 0 { 0 } else { 1 };

        HydraStatus {
            node_id: self.identity.node_id.clone(),
            enabled: self.config.enabled,
            stage,
            observation_count: self.diary.count().unwrap_or(0),
            event_count: self.event_log.len(),
            peer_count: self.peers.count(),
            domain_count: self.ledger.domain_count(),
            last_sync: None,
            alert_count: self.alerts.len(),
            network_id: self.network_identity.network_id_hex.clone(),
        }
    }
}
