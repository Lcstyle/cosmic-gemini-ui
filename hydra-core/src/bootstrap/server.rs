use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::bootstrap::protocol::BootstrapMsg;
use crate::error::HydraError;
use crate::event::SignedEvent;
use crate::peer::PeerList;
use crate::transport::HydraStream;

/// Handle a single incoming bootstrap connection.
///
/// Reads one message, processes it, sends a response.
/// Used by both the bootstrap .onion service and the node's personal .onion.
pub async fn handle_connection<S: HydraStream>(
    stream: S,
    peers: &PeerList,
    events: &[SignedEvent],
    our_node_id: &str,
    stage: u8,
) -> Result<(), HydraError> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one message
    buf_reader
        .read_line(&mut line)
        .await
        .map_err(|e| HydraError::Transport(format!("read error: {}", e)))?;

    if line.trim().is_empty() {
        return Ok(());
    }

    let msg = BootstrapMsg::from_line(&line)
        .map_err(|e| HydraError::Transport(format!("parse error: {}", e)))?;

    let response = match msg {
        BootstrapMsg::GetPeers => BootstrapMsg::Peers {
            peers: peers.peers.clone(),
            stage,
        },

        BootstrapMsg::Announce { node_id, onion_address } => {
            log::info!(
                "Peer announced: {} at {}",
                &node_id[..node_id.len().min(8)],
                onion_address
            );
            // The caller (node.rs) will handle actually adding the peer
            // after verifying the announcement. For now, always accept.
            BootstrapMsg::AnnounceAck { accepted: true }
        }

        BootstrapMsg::SyncRequest { since_sequence } => {
            let matching_events: Vec<SignedEvent> = events
                .iter()
                .filter(|e| e.sequence > since_sequence)
                .cloned()
                .collect();
            BootstrapMsg::SyncResponse {
                events: matching_events,
            }
        }

        BootstrapMsg::SyncPush { events: pushed } => {
            let count = pushed.len();
            log::info!("Received {} events via sync push", count);
            // The caller will handle merging these events
            BootstrapMsg::SyncPushAck {
                received_count: count,
            }
        }

        // These are responses, not requests — ignore
        BootstrapMsg::Peers { .. }
        | BootstrapMsg::AnnounceAck { .. }
        | BootstrapMsg::SyncResponse { .. }
        | BootstrapMsg::SyncPushAck { .. } => {
            return Ok(());
        }
    };

    let response_line = response
        .to_line()
        .map_err(|e| HydraError::Transport(format!("serialize error: {}", e)))?;
    writer
        .write_all(response_line.as_bytes())
        .await
        .map_err(|e| HydraError::Transport(format!("write error: {}", e)))?;
    writer
        .flush()
        .await
        .map_err(|e| HydraError::Transport(format!("flush error: {}", e)))?;

    let _ = our_node_id; // Will be used for filtering self from peer lists
    Ok(())
}
