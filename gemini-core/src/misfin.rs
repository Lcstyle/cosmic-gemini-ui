use std::sync::Arc;
use std::time::Duration;

use log::debug;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::client::Error;

const DEFAULT_PORT: u16 = 1958;
const MAX_TIMEOUT_SECONDS: u64 = 15;

pub struct MisfinMessage {
    pub recipient: String, // user@host
    pub body: String,      // UTF-8 text, max ~2000 chars
    pub sender_cert: Vec<CertificateDer<'static>>,
    pub sender_key: PrivateKeyDer<'static>,
}

pub struct MisfinResponse {
    pub status: u8,
    pub meta: String,
}

/// Send a message via the Misfin protocol.
///
/// Misfin uses TLS on port 1958 and requires a client certificate.
/// Request format: `misfin://user@host <body>\r\n`
/// Response: `<2-digit status> <meta>\r\n`
pub async fn send_message(msg: &MisfinMessage) -> Result<MisfinResponse, Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Parse recipient: user@host[:port]
    let (host, port) = parse_recipient_host(&msg.recipient)?;

    // Build TLS config with client cert (required for Misfin)
    let config = crate::client::build_tls_config_with_client_auth(
        msg.sender_cert.clone(),
        msg.sender_key.clone_key(),
    )?;
    let tls_config = Arc::new(config);

    // Connect
    let tcp_stream = tokio::time::timeout(
        Duration::from_secs(MAX_TIMEOUT_SECONDS),
        TcpStream::connect(format!("{}:{}", host, port)),
    )
    .await
    .map_err(|_| Error::Timeout)?
    .map_err(Error::Io)?;

    let server_name =
        ServerName::try_from(host.clone()).map_err(|_| Error::InvalidHost)?;

    let connector = TlsConnector::from(tls_config);
    let mut tls_stream = tokio::time::timeout(
        Duration::from_secs(MAX_TIMEOUT_SECONDS),
        connector.connect(server_name, tcp_stream),
    )
    .await
    .map_err(|_| Error::Timeout)?
    .map_err(Error::Io)?;

    // Send request: misfin://user@host <body>\r\n
    let request = format!("misfin://{} {}\r\n", msg.recipient, msg.body);
    tls_stream.write_all(request.as_bytes()).await?;
    debug!("Misfin message sent to {}", msg.recipient);

    // Read response status line
    let mut buf = Vec::with_capacity(256);
    let mut limited = (&mut tls_stream).take(1024);
    limited.read_to_end(&mut buf).await?;

    parse_response(&buf)
}

fn parse_recipient_host(recipient: &str) -> Result<(String, u16), Error> {
    // recipient is "user@host" or "user@host:port"
    let at_pos = recipient.find('@').ok_or(Error::InvalidHost)?;
    let host_part = &recipient[at_pos + 1..];

    if let Some(colon_pos) = host_part.rfind(':') {
        let host = host_part[..colon_pos].to_string();
        let port: u16 = host_part[colon_pos + 1..]
            .parse()
            .map_err(|_| Error::InvalidHost)?;
        Ok((host, port))
    } else {
        Ok((host_part.to_string(), DEFAULT_PORT))
    }
}

fn parse_response(buf: &[u8]) -> Result<MisfinResponse, Error> {
    let text = std::str::from_utf8(buf).map_err(Error::InvalidUtf8)?;
    let line = text.lines().next().unwrap_or("");

    if line.len() < 2 {
        return Err(Error::InvalidProtocolData(
            crate::client::ProtoError::MetaNotFound,
        ));
    }

    let status: u8 = line[..2]
        .parse()
        .map_err(|_| Error::InvalidProtocolData(crate::client::InvalidStatus.into()))?;

    let meta = if line.len() > 3 { &line[3..] } else { "" };

    Ok(MisfinResponse {
        status,
        meta: meta.trim_end().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recipient_simple() {
        let (host, port) = parse_recipient_host("user@example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, DEFAULT_PORT);
    }

    #[test]
    fn parse_recipient_with_port() {
        let (host, port) = parse_recipient_host("user@example.com:2000").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 2000);
    }

    #[test]
    fn parse_response_success() {
        let resp = parse_response(b"20 Message delivered\r\n").unwrap();
        assert_eq!(resp.status, 20);
        assert_eq!(resp.meta, "Message delivered");
    }

    #[test]
    fn parse_response_error() {
        let resp = parse_response(b"50 User not found\r\n").unwrap();
        assert_eq!(resp.status, 50);
        assert_eq!(resp.meta, "User not found");
    }
}
