use std::sync::Arc;
use std::time::Duration;

use log::debug;
use rustls::pki_types::ServerName;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::Url;

use crate::client::{Error, Response};
use crate::known_hosts::{self, KnownHostsFile};

const MAX_TIMEOUT_SECONDS: u64 = 30;

pub struct TitanRequest {
    pub url: String,
    pub mime: String,
    pub token: Option<String>,
    pub payload: Vec<u8>,
}

/// Upload content via the Titan protocol.
///
/// Titan uses the same TLS connection as Gemini (port 1965) but sends
/// payload data after the request line.
pub async fn upload(request: &TitanRequest) -> Result<Response, Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let parsed = Url::parse(&request.url).map_err(Error::InvalidUrl)?;
    let scheme = parsed.scheme();
    if scheme != "titan" {
        return Err(Error::SchemeNotSupported);
    }

    let host = parsed.host_str().ok_or(Error::InvalidHost)?.to_owned();
    let port = parsed.port().unwrap_or(1965);

    // Build request line: titan://host/path;mime=<mime>;size=<len>[;token=<tok>]\r\n
    let mut params = format!(
        ";mime={};size={}",
        request.mime,
        request.payload.len()
    );
    if let Some(ref token) = request.token {
        if !token.is_empty() {
            params.push_str(&format!(";token={}", token));
        }
    }

    let request_line = format!(
        "titan://{}{}{}{}",
        host,
        if port != 1965 {
            format!(":{}", port)
        } else {
            String::new()
        },
        parsed.path(),
        params,
    );

    // TLS setup with TOFU (reuse the same verifier pattern as Client)
    let config = crate::client::build_tls_config();
    let tls_config = Arc::new(config);

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

    // TOFU validation
    {
        let (_, server_conn) = tls_stream.get_ref();
        if let Some(certs) = server_conn.peer_certificates() {
            if let Some(cert) = certs.first() {
                let mut known_hosts = KnownHostsFile::open_default().unwrap_or_else(|_| {
                    let temp = tempfile::tempfile().expect("Cannot create temp file");
                    KnownHostsFile::new(temp)
                });
                known_hosts::validate(&mut known_hosts, &host, cert.as_ref())?;
            }
        }
    }

    // Send request line + payload
    let full_request = format!("{}\r\n", request_line);
    tls_stream.write_all(full_request.as_bytes()).await?;
    tls_stream.write_all(&request.payload).await?;
    debug!("Titan upload sent to {}: {} bytes", request_line, request.payload.len());

    Response::from_async_read(tls_stream).await
}

/// Upload with client certificate authentication.
pub async fn upload_with_identity(
    request: &TitanRequest,
    cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
    private_key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<Response, Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let parsed = Url::parse(&request.url).map_err(Error::InvalidUrl)?;
    if parsed.scheme() != "titan" {
        return Err(Error::SchemeNotSupported);
    }

    let host = parsed.host_str().ok_or(Error::InvalidHost)?.to_owned();
    let port = parsed.port().unwrap_or(1965);

    let mut params = format!(
        ";mime={};size={}",
        request.mime,
        request.payload.len()
    );
    if let Some(ref token) = request.token {
        if !token.is_empty() {
            params.push_str(&format!(";token={}", token));
        }
    }

    let request_line = format!(
        "titan://{}{}{}{}",
        host,
        if port != 1965 {
            format!(":{}", port)
        } else {
            String::new()
        },
        parsed.path(),
        params,
    );

    let config =
        crate::client::build_tls_config_with_client_auth(cert_chain, private_key)?;
    let tls_config = Arc::new(config);

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

    {
        let (_, server_conn) = tls_stream.get_ref();
        if let Some(certs) = server_conn.peer_certificates() {
            if let Some(cert) = certs.first() {
                let mut known_hosts = KnownHostsFile::open_default().unwrap_or_else(|_| {
                    let temp = tempfile::tempfile().expect("Cannot create temp file");
                    KnownHostsFile::new(temp)
                });
                known_hosts::validate(&mut known_hosts, &host, cert.as_ref())?;
            }
        }
    }

    let full_request = format!("{}\r\n", request_line);
    tls_stream.write_all(full_request.as_bytes()).await?;
    tls_stream.write_all(&request.payload).await?;
    debug!("Titan upload (with identity) sent to {}: {} bytes", request_line, request.payload.len());

    Response::from_async_read(tls_stream).await
}
