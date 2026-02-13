use std::convert::TryFrom;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use log::debug;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::Url;

use tokio::sync::broadcast;

use crate::known_hosts::{self, CertificateError, KnownHostsFile};

const MAX_REDIRECT: u8 = 5;
const MAX_TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("Invalid status")]
    InvalidStatus(#[from] InvalidStatus),
    #[error("Meta not found (no <CR><LF>)")]
    MetaNotFound,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid status")]
pub struct InvalidStatus;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("The server doesn't follow the Gemini protocol: {0}")]
    InvalidProtocolData(#[from] ProtoError),
    #[error("The server sent invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("TLS error: {0}")]
    Tls(#[from] CertificateError),
    #[error("Invalid host")]
    InvalidHost,
    #[error("Too many redirections. Last redirect was to {0}")]
    TooManyRedirects(String),
    #[error("Only the gemini:// URL scheme is supported")]
    SchemeNotSupported,
    #[error("Connection timed out")]
    Timeout,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Eq)]
pub enum Status {
    Input(u8),
    Success(u8),
    Redirect(u8),
    TempFail(u8),
    PermFail(u8),
    CertRequired(u8),
}

impl TryFrom<u8> for Status {
    type Error = InvalidStatus;
    fn try_from(s: u8) -> Result<Self, Self::Error> {
        match s / 10 {
            1 => Ok(Status::Input(s)),
            2 => Ok(Status::Success(s)),
            3 => Ok(Status::Redirect(s)),
            4 => Ok(Status::TempFail(s)),
            5 => Ok(Status::PermFail(s)),
            6 => Ok(Status::CertRequired(s)),
            _ => Err(InvalidStatus),
        }
    }
}

pub struct Response {
    status: Status,
    meta: String,
    body: Pin<Box<dyn AsyncRead + Send>>,
}

impl Response {
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn meta(&self) -> &str {
        &self.meta
    }
    pub fn into_body(self) -> Option<Pin<Box<dyn AsyncRead + Send>>> {
        match self.status {
            Status::Success(_) => Some(self.body),
            _ => None,
        }
    }

    /// Read the entire body into a Vec<u8>.
    pub async fn body_bytes(self) -> Option<Vec<u8>> {
        let mut body = self.into_body()?;
        let mut buf = Vec::new();
        let _ = body.read_to_end(&mut buf).await;
        Some(buf)
    }

    /// Read the entire body as a UTF-8 string.
    pub async fn body_text(self) -> Option<String> {
        let bytes = self.body_bytes().await?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Parse the response header from raw bytes (status + meta).
    pub async fn from_async_read(
        mut reader: impl AsyncRead + Unpin + Send + 'static,
    ) -> Result<Self, Error> {
        let mut buffer = Vec::with_capacity(2048);
        // 3 bytes for status + space, 1024 max bytes for meta
        let mut limited = (&mut reader).take(3 + 1024);
        limited.read_to_end(&mut buffer).await?;

        let meta_end = buffer[3..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .map(|i| i + 3)
            .ok_or(Error::InvalidProtocolData(ProtoError::MetaNotFound))?;

        let status: u8 = std::str::from_utf8(buffer.get(0..2).unwrap_or(&[]))?
            .parse()
            .map_err(|_| Error::InvalidProtocolData(InvalidStatus.into()))?;

        let status = Status::try_from(status)
            .map_err(|_| Error::InvalidProtocolData(InvalidStatus.into()))?;

        let meta_buffer = buffer.get(3..meta_end).unwrap_or(&[]);
        let meta = String::from_utf8_lossy(meta_buffer).to_string();

        // 2 byte offset for '\r\n'
        let split_at = meta_end + 2;
        let remaining = buffer.split_off(split_at);
        let cursor = Cursor::new(remaining);

        // Chain leftover buffer bytes with the rest of the reader
        let chained = AsyncReadExt::chain(cursor, reader);

        Ok(Response {
            status,
            meta,
            body: Box::pin(chained),
        })
    }
}

/// Custom server certificate verifier for TOFU (Trust On First Use).
///
/// This verifier accepts all certificates during the TLS handshake and stores
/// the presented certificate for post-handshake TOFU validation.
#[derive(Debug)]
struct TofuVerifier;

impl rustls::client::danger::ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Accept all certs during handshake; TOFU validation happens post-connect
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

/// Raw TLS certificate capture for HYDRA protocol observation.
///
/// Sent on the broadcast channel after each successful TLS handshake.
#[derive(Debug, Clone)]
pub struct TlsCertCapture {
    pub host: String,
    pub port: u16,
    pub certs_der: Vec<Vec<u8>>,
    pub timestamp: std::time::SystemTime,
}

pub struct Client {
    tls_config: Arc<rustls::ClientConfig>,
    cert_observer: Option<broadcast::Sender<TlsCertCapture>>,
}

/// Build a rustls ClientConfig with TOFU verification and no client auth.
pub fn build_tls_config() -> rustls::ClientConfig {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TofuVerifier))
        .with_no_client_auth();
    config.alpn_protocols = Vec::new();
    config
}

/// Build a rustls ClientConfig with TOFU verification and client certificate auth.
pub fn build_tls_config_with_client_auth(
    cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
    private_key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<rustls::ClientConfig, Error> {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TofuVerifier))
        .with_client_auth_cert(cert_chain, private_key)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    config.alpn_protocols = Vec::new();
    Ok(config)
}

impl Client {
    pub fn new() -> Self {
        // Ensure the ring crypto provider is installed
        let _ = rustls::crypto::ring::default_provider().install_default();

        Self {
            tls_config: Arc::new(build_tls_config()),
            cert_observer: None,
        }
    }

    /// Attach a HYDRA certificate observer to this client.
    ///
    /// After each successful TLS handshake, the server's certificate chain
    /// will be sent on this channel for HYDRA observation processing.
    pub fn with_cert_observer(mut self, tx: broadcast::Sender<TlsCertCapture>) -> Self {
        self.cert_observer = Some(tx);
        self
    }

    pub async fn fetch(&self, url_str: &str) -> Result<Response, Error> {
        self.fetch_with_redirect(url_str, true).await
    }

    /// Fetch a URL presenting a client certificate for authentication.
    /// Follows redirects internally, preserving the client certificate across hops.
    pub async fn fetch_with_identity(
        &self,
        url_str: &str,
        cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
        private_key: rustls::pki_types::PrivateKeyDer<'static>,
    ) -> Result<Response, Error> {
        let config = Arc::new(build_tls_config_with_client_auth(cert_chain, private_key)?);
        let mut url_str = url_str.to_string();

        for i in 0..=MAX_REDIRECT {
            let url = Url::parse(&url_str)?;
            let res = self.fetch_internal_with_config(&url, config.clone()).await?;

            match res.status() {
                Status::Redirect(_) if i < MAX_REDIRECT => {
                    let redirect_target = res.meta().trim().to_string();
                    match Url::parse(&redirect_target) {
                        Ok(_) => {
                            url_str = redirect_target;
                        }
                        Err(_) => {
                            let base_url = Url::parse(&url_str)?;
                            let new_url = base_url.join(&redirect_target)?;
                            url_str = new_url.to_string();
                        }
                    }
                }
                Status::Redirect(_) => {
                    return Err(Error::TooManyRedirects(res.meta().to_string()));
                }
                _ => return Ok(res),
            }
        }

        unreachable!()
    }

    pub async fn fetch_with_redirect(
        &self,
        url_str: &str,
        follow_redirects: bool,
    ) -> Result<Response, Error> {
        let mut url_str = url_str.to_string();
        let max_redirect = if follow_redirects { MAX_REDIRECT } else { 1 };

        for i in 0..=max_redirect {
            let url = Url::parse(&url_str)?;
            let res = self.fetch_internal_with_config(&url, self.tls_config.clone()).await?;

            match res.status() {
                Status::Redirect(_) if i < max_redirect => {
                    // Resolve redirect URL (may be relative)
                    let redirect_target = res.meta().trim().to_string();
                    match Url::parse(&redirect_target) {
                        Ok(_) => {
                            url_str = redirect_target;
                        }
                        Err(_) => {
                            // Any parse error — treat as relative URL
                            let base_url = Url::parse(&url_str)?;
                            let new_url = base_url.join(&redirect_target)?;
                            url_str = new_url.to_string();
                        }
                    }
                }
                Status::Redirect(_) => {
                    return Err(Error::TooManyRedirects(res.meta().to_string()));
                }
                _ => return Ok(res),
            }
        }

        unreachable!()
    }

    async fn fetch_internal_with_config(
        &self,
        url: &Url,
        tls_config: Arc<rustls::ClientConfig>,
    ) -> Result<Response, Error> {
        if url.scheme() != "gemini" {
            return Err(Error::SchemeNotSupported);
        }

        let host = url.host_str().ok_or(Error::InvalidHost)?.to_owned();
        let port = url.port().unwrap_or(1965);

        // Connect with timeout
        let tcp_stream = tokio::time::timeout(
            Duration::from_secs(MAX_TIMEOUT_SECONDS),
            TcpStream::connect(format!("{}:{}", host, port)),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(Error::Io)?;

        let server_name = ServerName::try_from(host.clone())
            .map_err(|_| Error::InvalidHost)?;

        let connector = TlsConnector::from(tls_config);
        let mut tls_stream = tokio::time::timeout(
            Duration::from_secs(MAX_TIMEOUT_SECONDS),
            connector.connect(server_name, tcp_stream),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(Error::Io)?;

        // Post-handshake TOFU validation
        {
            let (_, server_conn) = tls_stream.get_ref();
            if let Some(certs) = server_conn.peer_certificates() {
                if let Some(cert) = certs.first() {
                    let mut known_hosts = KnownHostsFile::open_default()
                        .unwrap_or_else(|_| {
                            // Fall back to in-memory store
                            let temp = tempfile::tempfile().expect("Cannot create temp file");
                            KnownHostsFile::new(temp)
                        });
                    known_hosts::validate(&mut known_hosts, &host, cert.as_ref())?;
                }

                // Send certificate capture to HYDRA observer (if attached)
                if let Some(ref observer) = self.cert_observer {
                    let capture = TlsCertCapture {
                        host: host.clone(),
                        port,
                        certs_der: certs.iter().map(|c| c.as_ref().to_vec()).collect(),
                        timestamp: std::time::SystemTime::now(),
                    };
                    let _ = observer.send(capture);
                }
            }
        }

        // Send request: URL\r\n
        let request = format!("{}\r\n", url);
        tls_stream.write_all(request.as_bytes()).await?;
        debug!("Request sent to {}", url);

        Response::from_async_read(tls_stream).await
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_from_bytes(bytes: &[u8]) -> Result<Response, Error> {
        let cursor = Cursor::new(bytes.to_vec());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(Response::from_async_read(cursor))
    }

    #[test]
    fn basic_response() {
        let res = response_from_bytes(
            b"20 text/gemini\r\nBasic example response from a dummy server",
        )
        .unwrap();
        assert_eq!(res.status(), Status::Success(20));
        assert_eq!(res.meta(), "text/gemini");
    }

    #[test]
    fn redirect_response() {
        let res = response_from_bytes(
            b"31 gemini://gemini.circumlunar.space/\r\nUnexpected body",
        )
        .unwrap();
        assert_eq!(res.status(), Status::Redirect(31));
        assert_eq!(res.meta(), "gemini://gemini.circumlunar.space/");
        assert!(res.into_body().is_none());
    }

    #[test]
    fn no_meta_crlf() {
        let res = response_from_bytes(b"20 no crlf in this response at all");
        assert!(res.is_err());
    }

    #[test]
    fn input_status() {
        let res =
            response_from_bytes(b"10 What is your name?\r\n").unwrap();
        assert_eq!(res.status(), Status::Input(10));
        assert_eq!(res.meta(), "What is your name?");
    }

    #[test]
    fn status_try_from() {
        assert_eq!(Status::try_from(10).unwrap(), Status::Input(10));
        assert_eq!(Status::try_from(20).unwrap(), Status::Success(20));
        assert_eq!(Status::try_from(31).unwrap(), Status::Redirect(31));
        assert_eq!(Status::try_from(40).unwrap(), Status::TempFail(40));
        assert_eq!(Status::try_from(51).unwrap(), Status::PermFail(51));
        assert_eq!(Status::try_from(60).unwrap(), Status::CertRequired(60));
        assert!(Status::try_from(0).is_err());
        assert!(Status::try_from(70).is_err());
    }

    #[test]
    fn invalid_scheme() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = Client::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let res = rt.block_on(client.fetch("http://example.com"));
        assert!(matches!(res, Err(Error::SchemeNotSupported)));
    }
}
