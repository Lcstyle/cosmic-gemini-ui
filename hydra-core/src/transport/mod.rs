pub mod tor;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::HydraError;

/// A bidirectional stream (read + write).
pub trait HydraStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

/// Blanket implementation for any type that satisfies the bounds.
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> HydraStream for T {}

/// Transport trait — abstracts how we connect to peers.
///
/// The production implementation uses Tor (.onion addresses).
/// Tests can use in-memory channels.
#[async_trait]
pub trait Transport: Send + Sync {
    type Stream: HydraStream;

    /// Connect to a peer at the given address.
    async fn connect(&self, address: &str) -> Result<Self::Stream, HydraError>;
}

/// Listener trait — abstracts how we accept incoming connections.
#[async_trait]
pub trait TransportListener: Send {
    type Stream: HydraStream;

    /// Accept the next incoming connection.
    /// Returns the stream and the peer's address (if known).
    async fn accept(&mut self) -> Result<(Self::Stream, String), HydraError>;
}
