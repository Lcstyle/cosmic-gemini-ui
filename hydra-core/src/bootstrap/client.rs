use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::bootstrap::protocol::BootstrapMsg;
use crate::error::HydraError;
use crate::event::SignedEvent;
use crate::peer::PeerInfo;
use crate::transport::{HydraStream, Transport};

/// Send a message and read the response on a stream.
async fn request_response<S: HydraStream>(
    stream: S,
    msg: &BootstrapMsg,
) -> Result<BootstrapMsg, HydraError> {
    let (reader, mut writer) = tokio::io::split(stream);

    let line = msg
        .to_line()
        .map_err(|e| HydraError::Transport(format!("serialize error: {}", e)))?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| HydraError::Transport(format!("write error: {}", e)))?;
    writer
        .flush()
        .await
        .map_err(|e| HydraError::Transport(format!("flush error: {}", e)))?;

    let mut buf_reader = BufReader::new(reader);
    let mut response_line = String::new();
    buf_reader
        .read_line(&mut response_line)
        .await
        .map_err(|e| HydraError::Transport(format!("read error: {}", e)))?;

    BootstrapMsg::from_line(&response_line)
        .map_err(|e| HydraError::Transport(format!("parse response error: {}", e)))
}

/// Discover peers from a bootstrap node.
pub async fn discover_peers<T: Transport>(
    transport: &T,
    bootstrap_address: &str,
) -> Result<Vec<PeerInfo>, HydraError> {
    let stream = transport.connect(bootstrap_address).await?;
    let response = request_response(stream, &BootstrapMsg::GetPeers).await?;

    match response {
        BootstrapMsg::Peers { peers, .. } => Ok(peers),
        other => Err(HydraError::Transport(format!(
            "unexpected response to GetPeers: {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

/// Announce this node to a bootstrap peer.
pub async fn announce<T: Transport>(
    transport: &T,
    peer_address: &str,
    node_id: &str,
    onion_address: &str,
) -> Result<bool, HydraError> {
    let stream = transport.connect(peer_address).await?;
    let msg = BootstrapMsg::Announce {
        node_id: node_id.to_string(),
        onion_address: onion_address.to_string(),
    };
    let response = request_response(stream, &msg).await?;

    match response {
        BootstrapMsg::AnnounceAck { accepted } => Ok(accepted),
        other => Err(HydraError::Transport(format!(
            "unexpected response to Announce: {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

/// Request events from a peer since a given sequence number.
pub async fn request_sync<T: Transport>(
    transport: &T,
    peer_address: &str,
    since_sequence: u64,
) -> Result<Vec<SignedEvent>, HydraError> {
    let stream = transport.connect(peer_address).await?;
    let msg = BootstrapMsg::SyncRequest { since_sequence };
    let response = request_response(stream, &msg).await?;

    match response {
        BootstrapMsg::SyncResponse { events } => Ok(events),
        other => Err(HydraError::Transport(format!(
            "unexpected response to SyncRequest: {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

/// Push events to a peer (bilateral sync).
pub async fn push_events<T: Transport>(
    transport: &T,
    peer_address: &str,
    events: Vec<SignedEvent>,
) -> Result<usize, HydraError> {
    let stream = transport.connect(peer_address).await?;
    let msg = BootstrapMsg::SyncPush { events };
    let response = request_response(stream, &msg).await?;

    match response {
        BootstrapMsg::SyncPushAck { received_count } => Ok(received_count),
        other => Err(HydraError::Transport(format!(
            "unexpected response to SyncPush: {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}
