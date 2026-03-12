//! TLS support for PostgreSQL connections.
//!
//! Implements the PostgreSQL SSLRequest protocol and wraps the TCP stream
//! with rustls for encrypted communication. Enabled via the `tls` feature.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};

use crate::error::{PgError, PgResult};

// ─── SSL Mode ─────────────────────────────────────────────────

/// SSL/TLS mode for PostgreSQL connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    /// Never use TLS. Fail if the server requires it.
    Disable,
    /// Try TLS first; fall back to plaintext if the server doesn't support it.
    Prefer,
    /// Require TLS. Fail if the server doesn't support it.
    Require,
}

impl SslMode {
    /// Parse from a string (e.g., URL query parameter `?sslmode=prefer`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "disable" => Some(SslMode::Disable),
            "prefer" => Some(SslMode::Prefer),
            "require" => Some(SslMode::Require),
            _ => None,
        }
    }
}

impl Default for SslMode {
    fn default() -> Self {
        SslMode::Prefer
    }
}

// ─── PostgreSQL SSLRequest ────────────────────────────────────

/// PostgreSQL SSLRequest message:
/// Int32(8) — message length including self,
/// Int32(80877103) — the SSL request code.
const SSL_REQUEST: [u8; 8] = [0x00, 0x00, 0x00, 0x08, 0x04, 0xd2, 0x16, 0x2f];

// ─── TLS Negotiation ─────────────────────────────────────────

/// Result of attempting TLS negotiation with the server.
pub(crate) enum TlsNegotiateResult {
    /// Server accepted TLS — stream is encrypted.
    Tls(TlsStream),
    /// Server rejected TLS — TCP stream returned for plain-text use.
    Rejected(TcpStream),
}

/// Attempt TLS negotiation on an existing TCP connection.
///
/// Sends the PostgreSQL SSLRequest message, reads the server's single-byte
/// response (`S` = proceed, `N` = refused), and either completes the TLS
/// handshake or returns the TCP stream for plain-text use.
///
/// The TCP stream **must** be in blocking mode when this is called.
pub(crate) fn negotiate(mut tcp: TcpStream, host: &str) -> PgResult<TlsNegotiateResult> {
    // Send SSLRequest
    tcp.write_all(&SSL_REQUEST).map_err(PgError::Io)?;

    // Read single-byte response
    let mut response = [0u8; 1];
    tcp.read_exact(&mut response).map_err(PgError::Io)?;

    if response[0] != b'S' {
        return Ok(TlsNegotiateResult::Rejected(tcp));
    }

    // Build rustls client config with Mozilla root certificates
    let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|e| PgError::Protocol(format!("Invalid TLS server name '{}': {}", host, e)))?;

    let tls_conn = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| PgError::Protocol(format!("TLS connection init failed: {}", e)))?;

    let mut stream = TlsStream {
        tcp,
        tls: tls_conn,
    };

    // Complete the TLS handshake (blocking)
    stream.complete_handshake()?;

    Ok(TlsNegotiateResult::Tls(stream))
}

// ─── TLS Stream ───────────────────────────────────────────────

/// A TLS-wrapped TCP stream using the lower-level rustls API.
///
/// Handles non-blocking I/O correctly by using `read_tls()`/`write_tls()`
/// and `process_new_packets()` instead of `StreamOwned` (which does not
/// support non-blocking sockets).
pub(crate) struct TlsStream {
    tcp: TcpStream,
    tls: ClientConnection,
}

impl TlsStream {
    /// Drive the TLS handshake to completion (blocking).
    fn complete_handshake(&mut self) -> PgResult<()> {
        while self.tls.is_handshaking() {
            // Write pending TLS data to socket
            while self.tls.wants_write() {
                self.tls
                    .write_tls(&mut self.tcp)
                    .map_err(PgError::Io)?;
            }

            // Read more TLS data if handshake needs it
            if self.tls.is_handshaking() {
                let n = self
                    .tls
                    .read_tls(&mut self.tcp)
                    .map_err(PgError::Io)?;
                if n == 0 {
                    return Err(PgError::ConnectionClosed);
                }
                self.tls
                    .process_new_packets()
                    .map_err(|e| PgError::Protocol(format!("TLS handshake failed: {}", e)))?;
            }
        }

        // Flush any remaining TLS data
        while self.tls.wants_write() {
            self.tls
                .write_tls(&mut self.tcp)
                .map_err(PgError::Io)?;
        }

        Ok(())
    }

    /// Set the underlying TCP stream to non-blocking mode.
    pub(crate) fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.tcp.set_nonblocking(nonblocking)
    }

    /// Get the raw file descriptor of the underlying TCP stream.
    #[cfg(unix)]
    pub(crate) fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        use std::os::unix::io::AsRawFd;
        self.tcp.as_raw_fd()
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            // Try to read plaintext data that rustls has already decrypted
            match self.tls.reader().read(buf) {
                Ok(0) => {}
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }

            // Need more TLS records from the socket
            match self.tls.read_tls(&mut self.tcp) {
                Ok(0) => return Ok(0), // TCP EOF
                Ok(_) => {
                    self.tls
                        .process_new_packets()
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.tls.writer().write(buf)?;
        self.flush_tls()?;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tls.writer().flush()?;
        self.flush_tls()?;
        self.tcp.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.tls.writer().write_all(buf)?;
        self.flush_tls()
    }
}

impl TlsStream {
    /// Flush pending TLS records to the underlying TCP socket.
    fn flush_tls(&mut self) -> io::Result<()> {
        while self.tls.wants_write() {
            match self.tls.write_tls(&mut self.tcp) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
