//! TLS 1.2/1.3 termination via rustls (requires feature `tls`).
//!
//! The raw socket fd is kept for epoll/kqueue readiness; all plaintext I/O
//! goes through [`rustls::ServerConnection`].  Configure the server certificate
//! and key with [`TlsConfig`] and pass it to [`Server::bind_tls`](crate::server::Server).
//!
//! ```rust,no_run
//! # #[cfg(feature = "tls")]
//! # {
//! use chopin_core::tls::TlsConfig;
//! use chopin_core::{Router, Server};
//!
//! let tls = TlsConfig::from_pem_files("cert.pem", "key.pem").unwrap();
//! let mut router = Router::new();
//! Server::bind_tls("0.0.0.0:443", tls).serve(router).unwrap();
//! # }
//! ```
// src/tls.rs — TLS server support for chopin-core (feature = "tls")

use rustls::pki_types::CertificateDer;
use rustls::{ServerConfig, ServerConnection};
use std::io::{self, Read, Write};
use std::sync::Arc;

// ─── Server TLS Configuration ─────────────────────────────────────────────────

/// Shared TLS server configuration.
///
/// Created once at startup from PEM files and then cloned (cheaply) per worker
/// because it wraps an `Arc` internally.
#[derive(Clone)]
pub struct TlsServerConfig {
    inner: Arc<ServerConfig>,
}

impl TlsServerConfig {
    /// Load TLS configuration from PEM-encoded certificate and private key files.
    ///
    /// Supports certificate chains (multiple `BEGIN CERTIFICATE` blocks) and
    /// all key types supported by rustls (RSA PKCS#1, PKCS#8, EC).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use chopin_core::tls::TlsServerConfig;
    /// let cfg = TlsServerConfig::from_pem_files("cert.pem", "key.pem").unwrap();
    /// ```
    pub fn from_pem_files(cert_path: &str, key_path: &str) -> Result<Self, String> {
        let cert_pem = std::fs::read(cert_path)
            .map_err(|e| format!("Cannot read cert '{}': {}", cert_path, e))?;
        let key_pem = std::fs::read(key_path)
            .map_err(|e| format!("Cannot read key '{}': {}", key_path, e))?;

        // Parse certificate chain
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .filter_map(|c| c.ok())
            .collect();
        if certs.is_empty() {
            return Err(format!("No certificates found in '{}'", cert_path));
        }

        // Parse private key (any supported format)
        let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .map_err(|e| format!("Cannot parse key '{}': {}", key_path, e))?
            .ok_or_else(|| format!("No private key found in '{}'", key_path))?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| format!("TLS config error: {}", e))?;

        Ok(TlsServerConfig {
            inner: Arc::new(config),
        })
    }

    /// Create a new per-connection TLS session from this configuration.
    pub fn new_session(&self) -> Result<TlsSession, rustls::Error> {
        let conn = ServerConnection::new(Arc::clone(&self.inner))?;
        Ok(TlsSession { conn })
    }
}

// ─── Per-connection TLS Session ───────────────────────────────────────────────

/// Per-connection TLS state (one per accepted socket).
pub struct TlsSession {
    pub conn: ServerConnection,
}

// ─── Raw fd I/O adapters ──────────────────────────────────────────────────────

/// Zero-allocation `Read` impl that reads from a raw non-blocking fd.
struct FdReader(i32);

impl Read for FdReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.0, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

/// Zero-allocation `Write` impl that writes to a raw non-blocking fd.
struct FdWriter(i32);

impl Write for FdWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(self.0, buf.as_ptr() as *const libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ─── TLS I/O Helpers ──────────────────────────────────────────────────────────

/// Pull ciphertext from the socket fd into rustls, decrypt, and read plaintext
/// into `buf`.
///
/// Returns the number of plaintext bytes written into `buf`.
/// Returns `Ok(0)` on clean EOF. Returns `WouldBlock` if no data available.
pub fn tls_read(fd: i32, session: &mut TlsSession, buf: &mut [u8]) -> io::Result<usize> {
    // Pull ciphertext bytes from the kernel socket buffer into rustls.
    match session.conn.read_tls(&mut FdReader(fd)) {
        Ok(0) => return Ok(0), // Clean EOF
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            // No new ciphertext — fall through to drain already-buffered plaintext.
        }
        Err(e) => return Err(e),
        Ok(_) => {}
    }

    // Decrypt and validate the newly received TLS records.
    if let Err(e) = session.conn.process_new_packets() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
    }

    // Read available plaintext into caller's buffer.
    match session.conn.reader().read(buf) {
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
        other => other,
    }
}

/// Encrypt `buf` and flush ciphertext to the socket fd.
///
/// Returns the number of plaintext bytes consumed from `buf`.
pub fn tls_write(fd: i32, session: &mut TlsSession, buf: &[u8]) -> io::Result<usize> {
    let n = session.conn.writer().write(buf)?;
    tls_flush_pending(fd, session)?;
    Ok(n)
}

/// Write multiple non-contiguous plaintext slices through TLS in a single
/// logical operation (replaces `writev` for TLS connections).
pub fn tls_writev(fd: i32, session: &mut TlsSession, slices: &[&[u8]]) -> io::Result<usize> {
    let mut total = 0usize;
    for slice in slices {
        if slice.is_empty() {
            continue;
        }
        // Write each slice into rustls plaintext writer.
        let mut written = 0;
        while written < slice.len() {
            match session.conn.writer().write(&slice[written..]) {
                Ok(0) => break,
                Ok(n) => written += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        total += written;
    }
    // Flush all buffered ciphertext to the socket.
    tls_flush_pending(fd, session)?;
    Ok(total)
}

/// Flush any pending TLS output (server hello, application data) to the socket.
///
/// Safe to call after every handshake step and write; it is a no-op if there
/// is nothing to send.
pub fn tls_flush_pending(fd: i32, session: &mut TlsSession) -> io::Result<()> {
    while session.conn.wants_write() {
        match session.conn.write_tls(&mut FdWriter(fd)) {
            Ok(0) => break,
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Read bytes from a file fd in 64 KiB chunks and encrypt/send them through
/// a TLS session.  Used as a fallback for `Body::File` when TLS is active
/// (kernel `sendfile()` cannot encrypt data).
///
/// Returns the number of plaintext bytes sent.
pub fn tls_sendfile(
    socket_fd: i32,
    session: &mut TlsSession,
    file_fd: i32,
    offset: &mut u64,
    remaining: u64,
) -> io::Result<u64> {
    const CHUNK: usize = 65536;
    let mut chunk_buf = vec![0u8; CHUNK];
    let mut sent: u64 = 0;

    while sent < remaining {
        let to_read = ((remaining - sent) as usize).min(CHUNK);
        // pread64 — non-blocking read at offset
        let n = unsafe {
            libc::pread(
                file_fd,
                chunk_buf.as_mut_ptr() as *mut libc::c_void,
                to_read,
                *offset as libc::off_t,
            )
        };
        if n <= 0 {
            break; // EOF or error — stop sending
        }
        let plaintext = &chunk_buf[..n as usize];

        // Write through TLS
        match session.conn.writer().write_all(plaintext) {
            Ok(()) => {}
            Err(e) => return Err(e),
        }
        tls_flush_pending(socket_fd, session)?;

        *offset += n as u64;
        sent += n as u64;
    }
    Ok(sent)
}

/// Returns `true` once the TLS handshake has completed and application data
/// can be exchanged.
#[inline(always)]
pub fn is_handshake_complete(session: &TlsSession) -> bool {
    !session.conn.is_handshaking()
}

/// Returns `true` if rustls has pending ciphertext that must be flushed to
/// the socket before we can receive more data.
#[inline(always)]
pub fn wants_write(session: &TlsSession) -> bool {
    session.conn.wants_write()
}
