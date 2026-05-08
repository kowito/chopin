//! PostgreSQL connection with poll-based non-blocking I/O.
//!
//! Designed for integration with a thread-per-core event loop. The socket is set
//! to **non-blocking mode** at connect time; all reads and writes go through
//! `try_fill_read_buf` / `try_flush_write_buf` which return `WouldBlock` when
//! the socket is not ready. Higher-level methods use `poll_read` / `poll_write`
//! with a configurable timeout so the caller can integrate with epoll/kqueue.
//!
//! Features:
//! - **Non-blocking I/O** with application-level timeouts
//! - SCRAM-SHA-256 and cleartext authentication
//! - Extended Query Protocol with implicit statement caching
//! - Transaction support with safe closure-based API
//! - COPY IN (writer) and COPY OUT (reader)
//! - LISTEN/NOTIFY with notification buffering
//! - Proper affected row count from CommandComplete
//! - Raw socket fd accessor for event-loop registration

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::auth::ScramClient;
use crate::codec;
use crate::error::{PgError, PgResult};
use crate::protocol::*;
use crate::row::Row;
use crate::statement::{CacheStats, StatementCache};
#[cfg(feature = "tls")]
use crate::tls;
use crate::types::{PgValue, ToSql};

/// Default I/O timeout for poll operations (5 seconds).
const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(5);

// ─── SecretString ─────────────────────────────────────────────

/// A `String` wrapper that zeroes its memory when dropped.
///
/// Uses `std::ptr::write_volatile` to prevent the compiler from optimising
/// away the zero-fill before the allocator reclaims the bytes.  This
/// limits credential exposure through core dumps, `/proc/self/mem`, and
/// page-file leaks.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    /// Wrap a string value.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the contained string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // SAFETY: we own the allocation and will never read it again.
        // write_volatile prevents the optimizer from eliding the zeroing.
        let bytes = unsafe { self.0.as_mut_vec() };
        for b in bytes.iter_mut() {
            unsafe { std::ptr::write_volatile(b as *mut u8, 0) };
        }
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

impl PartialEq<&str> for SecretString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<str> for SecretString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

// ─── Stream Abstraction ──────────────────────────────────────

/// Unified stream type supporting TCP, Unix domain sockets, and TLS.
enum PgStream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
    #[cfg(feature = "tls")]
    Tls(tls::TlsStream),
}

impl Read for PgStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            PgStream::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            PgStream::Unix(s) => s.read(buf),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for PgStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            PgStream::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            PgStream::Unix(s) => s.write(buf),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            PgStream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            PgStream::Unix(s) => s.flush(),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.flush(),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            PgStream::Tcp(s) => s.write_all(buf),
            #[cfg(unix)]
            PgStream::Unix(s) => s.write_all(buf),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.write_all(buf),
        }
    }
}

impl PgStream {
    fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        match self {
            PgStream::Tcp(s) => s.set_nonblocking(nonblocking),
            #[cfg(unix)]
            PgStream::Unix(s) => s.set_nonblocking(nonblocking),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.set_nonblocking(nonblocking),
        }
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        match self {
            PgStream::Tcp(s) => s.as_raw_fd(),
            PgStream::Unix(s) => s.as_raw_fd(),
            #[cfg(feature = "tls")]
            PgStream::Tls(s) => s.as_raw_fd(),
        }
    }

    /// Get the SHA-256 hash of the server's TLS certificate for channel binding.
    /// Returns `None` if the connection is not TLS or no cert is available.
    #[cfg(feature = "tls")]
    fn tls_server_cert_hash(&self) -> Option<Vec<u8>> {
        match self {
            PgStream::Tls(s) => s.server_cert_hash(),
            _ => None,
        }
    }
}

/// Connection configuration.
#[derive(Debug, Clone)]
pub struct PgConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Password stored in a zeroing wrapper — never appears in `Debug` output.
    pub password: SecretString,
    pub database: String,
    /// Optional Unix domain socket directory.
    /// When set, connect via `<socket_dir>/.s.PGSQL.<port>` instead of TCP.
    pub socket_dir: Option<String>,
    /// Optional `application_name` sent during startup.
    /// Visible in `pg_stat_activity.application_name` for observability.
    pub application_name: Option<String>,
    /// Initial read buffer size in bytes.  The buffer grows dynamically as
    /// needed; this only controls the initial allocation. Default: 64 KiB.
    pub read_buf_size: usize,
    /// Initial write buffer size in bytes. Default: 64 KiB.
    pub write_buf_size: usize,
    /// SSL/TLS mode. Only effective when the `tls` feature is enabled.
    /// Default: `Prefer` (try TLS, fall back to plaintext).
    #[cfg(feature = "tls")]
    pub ssl_mode: tls::SslMode,
    /// Path to a PEM file containing one or more root CA certificates.
    ///
    /// Use this to trust a private CA, such as the
    /// [AWS RDS CA bundle](https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem).
    /// When set, these certs replace the default Mozilla WebPKI root store.
    /// Only effective when the `tls` feature is enabled.
    #[cfg(feature = "tls")]
    pub ssl_root_cert: Option<String>,
}

impl PgConfig {
    pub fn new(host: &str, port: u16, user: &str, password: &str, database: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            user: user.to_string(),
            password: SecretString::new(password),
            database: database.to_string(),
            socket_dir: None,
            application_name: None,
            read_buf_size: 64 * 1024,
            write_buf_size: 64 * 1024,
            #[cfg(feature = "tls")]
            ssl_mode: tls::SslMode::default(),
            #[cfg(feature = "tls")]
            ssl_root_cert: None,
        }
    }

    /// Set a Unix domain socket directory for the connection.
    /// The actual socket path will be `<dir>/.s.PGSQL.<port>`.
    pub fn with_socket_dir(mut self, dir: &str) -> Self {
        self.socket_dir = Some(dir.to_string());
        self
    }

    /// Set `application_name` to identify this connection in `pg_stat_activity`.
    pub fn with_application_name(mut self, name: &str) -> Self {
        self.application_name = Some(name.to_string());
        self
    }

    /// Set the initial read and write buffer sizes (bytes).
    ///
    /// Buffers grow automatically as needed; this controls only the initial
    /// allocation size. Both values are clamped to a minimum of 8 KiB.
    pub fn with_buf_sizes(mut self, read_buf_size: usize, write_buf_size: usize) -> Self {
        self.read_buf_size = read_buf_size.max(8 * 1024);
        self.write_buf_size = write_buf_size.max(8 * 1024);
        self
    }

    /// Set the SSL/TLS mode for the connection.
    #[cfg(feature = "tls")]
    pub fn with_ssl_mode(mut self, mode: tls::SslMode) -> Self {
        self.ssl_mode = mode;
        self
    }

    /// Set the path to a PEM file of root CA certificates.
    ///
    /// Required for connecting to AWS RDS (and other servers backed by a
    /// private CA). Download the bundle from:
    /// <https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem>
    ///
    /// # Example
    /// ```ignore
    /// let config = PgConfig::new("mydb.cluster-xyz.us-east-1.rds.amazonaws.com", 5432, "user", "pass", "mydb")
    ///     .with_ssl_mode(SslMode::VerifyFull)
    ///     .with_ssl_root_cert("/etc/ssl/certs/aws-rds-bundle.pem");
    /// ```
    #[cfg(feature = "tls")]
    pub fn with_ssl_root_cert(mut self, path: &str) -> Self {
        self.ssl_root_cert = Some(path.to_string());
        self
    }

    /// Parse from a connection string: `postgres://user:pass@host:port/db`
    ///
    /// For Unix domain sockets, use a path as the host:
    /// `postgres://user:pass@%2Fvar%2Frun%2Fpostgresql/db`  (URL-encoded slashes)
    /// or `postgres://user:pass@/db?host=/var/run/postgresql`
    ///
    /// IPv6 addresses must use bracket notation: `postgres://user:pass@[::1]:5432/db`
    ///
    /// Passwords containing `@` or `:` must be percent-encoded.
    pub fn from_url(url: &str) -> PgResult<Self> {
        let url = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .ok_or_else(|| PgError::Protocol("Invalid URL scheme".to_string()))?;

        // Use rsplit_once so that passwords containing '@' (percent-encoded as %40
        // or literal '@') are handled correctly — the last '@' separates
        // userinfo from host.
        let (userpass, hostdb) = url
            .rsplit_once('@')
            .ok_or_else(|| PgError::Protocol("Missing @ in URL".to_string()))?;

        // Split user:password on the FIRST ':' only so passwords containing
        // ':' (percent-encoded) are preserved intact.
        let (user, password) = userpass.split_once(':').unwrap_or((userpass, ""));

        // Check for ?host= query parameter (Unix socket)
        let (hostdb_part, query_part) = hostdb.split_once('?').unwrap_or((hostdb, ""));

        let (hostport, database) = hostdb_part
            .split_once('/')
            .ok_or_else(|| PgError::Protocol("Missing database in URL".to_string()))?;

        // Parse query params for socket dir, sslmode, and sslrootcert
        let mut socket_dir: Option<String> = None;
        let mut application_name: Option<String> = None;
        #[cfg(feature = "tls")]
        let mut ssl_mode = tls::SslMode::default();
        #[cfg(feature = "tls")]
        let mut ssl_root_cert: Option<String> = None;
        if !query_part.is_empty() {
            for param in query_part.split('&') {
                if let Some(value) = param.strip_prefix("host=")
                    && value.starts_with('/')
                {
                    socket_dir = Some(value.to_string());
                }
                if let Some(value) = param.strip_prefix("application_name=") {
                    application_name = Some(percent_decode(value));
                }
                #[cfg(feature = "tls")]
                if let Some(value) = param.strip_prefix("sslmode=")
                    && let Some(mode) = tls::SslMode::parse(value)
                {
                    ssl_mode = mode;
                }
                #[cfg(feature = "tls")]
                if let Some(value) = param.strip_prefix("sslrootcert=") {
                    ssl_root_cert = Some(percent_decode(value));
                }
            }
        }

        // Decode percent-encoded host (e.g., %2Fvar%2Frun -> /var/run)
        let decoded_host = percent_decode(hostport);
        let is_unix_path = decoded_host.starts_with('/');
        if is_unix_path {
            socket_dir = Some(decoded_host);
        }

        let (host, port) = if socket_dir.is_some() {
            // Unix socket — host is irrelevant, use default port
            let port_str = if hostport.is_empty() || is_unix_path {
                "5432"
            } else {
                hostport.rsplit_once(':').map(|(_, p)| p).unwrap_or("5432")
            };
            let port: u16 = port_str
                .parse()
                .map_err(|_| PgError::Protocol("Invalid port".to_string()))?;
            ("localhost".to_string(), port)
        } else {
            // Handle IPv6 bracket notation: [::1]:5432 or [::1]
            let (h, port_str) = if hostport.starts_with('[') {
                let bracket_end = hostport.find(']').ok_or_else(|| {
                    PgError::Protocol("Unterminated IPv6 address in URL".to_string())
                })?;
                let addr = &hostport[1..bracket_end];
                let rest = &hostport[bracket_end + 1..];
                let p = rest.strip_prefix(':').unwrap_or("5432");
                (addr, p)
            } else {
                let (h, p) = hostport.split_once(':').unwrap_or((hostport, "5432"));
                (h, p)
            };
            let port: u16 = port_str
                .parse()
                .map_err(|_| PgError::Protocol("Invalid port".to_string()))?;
            (h.to_string(), port)
        };

        Ok(Self {
            host,
            port,
            user: percent_decode(user),
            password: SecretString::new(percent_decode(password)),
            database: percent_decode(database),
            socket_dir,
            application_name,
            read_buf_size: 64 * 1024,
            write_buf_size: 64 * 1024,
            #[cfg(feature = "tls")]
            ssl_mode,
            #[cfg(feature = "tls")]
            ssl_root_cert,
        })
    }
}

/// Minimal percent-decoding for URL host component.
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2]))
        {
            result.push((hi << 4 | lo) as char);
            i += 3;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// A notification received via LISTEN/NOTIFY.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Process ID of the notifying backend.
    pub process_id: i32,
    /// Channel name.
    pub channel: String,
    /// Payload string.
    pub payload: String,
}

/// Type alias for a notice handler function pointer.
type NoticeHandler = Box<dyn Fn(&str, &str, &str) + Send + Sync>;

/// A synchronous PostgreSQL connection with poll-based non-blocking I/O.
///
/// The socket is set to non-blocking mode at connect time.  I/O methods
/// internally poll with a configurable timeout so they can be adapted to
/// an event-loop's readiness notifications.
pub struct PgConnection {
    stream: PgStream,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    read_pos: usize,
    tx_status: TransactionStatus,
    stmt_cache: StatementCache,
    process_id: i32,
    secret_key: i32,
    server_params: Vec<(String, String)>,
    /// Buffered notifications received during query processing.
    notifications: VecDeque<Notification>,
    /// Number of rows affected by the last command (from CommandComplete).
    last_affected_rows: u64,
    /// The last CommandComplete tag string.
    last_command_tag: String,
    /// Whether the socket is in non-blocking mode.
    nonblocking: bool,
    /// Application-level I/O timeout for poll operations.
    io_timeout: Duration,
    /// Optional callback invoked when the server sends a NoticeResponse.
    notice_handler: Option<NoticeHandler>,
    /// Flag set on fatal I/O errors. A broken connection must not be
    /// returned to the pool; it will be discarded on drop.
    broken: bool,
    /// Host used to connect — stored for cancel request.
    cancel_host: String,
    /// Port used to connect — stored for cancel request.
    cancel_port: u16,
    /// Unix socket directory, if used — stored for cancel request.
    cancel_socket_dir: Option<String>,
    /// Monotonically increasing counter for generating unique portal names.
    portal_counter: u32,
}

impl PgConnection {
    /// Connect to PostgreSQL (blocking during handshake, then switches to
    /// non-blocking mode once authentication completes).
    pub fn connect(config: &PgConfig) -> PgResult<Self> {
        let stream = if let Some(ref socket_dir) = config.socket_dir {
            // Unix domain socket connection
            #[cfg(unix)]
            {
                let socket_path = format!("{}/.s.PGSQL.{}", socket_dir, config.port);
                let unix_stream = UnixStream::connect(&socket_path).map_err(PgError::Io)?;
                PgStream::Unix(unix_stream)
            }
            #[cfg(not(unix))]
            {
                let _ = socket_dir;
                return Err(PgError::Protocol(
                    "Unix domain sockets are not supported on this platform".to_string(),
                ));
            }
        } else {
            let addr = format!("{}:{}", config.host, config.port);
            let tcp = TcpStream::connect(&addr).map_err(PgError::Io)?;
            // Disable Nagle's algorithm for lower latency
            let _ = tcp.set_nodelay(true);

            // TLS negotiation (when feature enabled)
            #[cfg(feature = "tls")]
            {
                let root_cert = config.ssl_root_cert.as_deref();
                match config.ssl_mode {
                    tls::SslMode::Disable => PgStream::Tcp(tcp),
                    tls::SslMode::Prefer => {
                        match tls::negotiate(tcp, &config.host, root_cert) {
                            Ok(tls::TlsNegotiateResult::Tls(tls_stream)) => {
                                PgStream::Tls(tls_stream)
                            }
                            Ok(tls::TlsNegotiateResult::Rejected(tcp)) => PgStream::Tcp(tcp),
                            Err(_) => {
                                // TLS negotiation failed — reconnect plain-text
                                let tcp = TcpStream::connect(&addr).map_err(PgError::Io)?;
                                let _ = tcp.set_nodelay(true);
                                PgStream::Tcp(tcp)
                            }
                        }
                    }
                    tls::SslMode::Require | tls::SslMode::VerifyFull => {
                        match tls::negotiate(tcp, &config.host, root_cert)? {
                            tls::TlsNegotiateResult::Tls(tls_stream) => PgStream::Tls(tls_stream),
                            tls::TlsNegotiateResult::Rejected(_) => {
                                return Err(PgError::Protocol(format!(
                                    "Server does not support SSL (sslmode={:?})",
                                    config.ssl_mode
                                )));
                            }
                        }
                    }
                }
            }

            #[cfg(not(feature = "tls"))]
            PgStream::Tcp(tcp)
        };

        let mut conn = Self {
            stream,
            read_buf: vec![0u8; config.read_buf_size],
            write_buf: vec![0u8; config.write_buf_size],
            read_pos: 0,
            tx_status: TransactionStatus::Idle,
            stmt_cache: StatementCache::new(),
            process_id: 0,
            secret_key: 0,
            server_params: Vec::new(),
            notifications: VecDeque::new(),
            last_affected_rows: 0,
            last_command_tag: String::new(),
            nonblocking: false,
            io_timeout: DEFAULT_IO_TIMEOUT,
            notice_handler: None,
            broken: false,
            cancel_host: config.host.clone(),
            cancel_port: config.port,
            cancel_socket_dir: config.socket_dir.clone(),
            portal_counter: 0,
        };

        conn.startup(config)?;

        // Switch to non-blocking after successful authentication
        conn.stream.set_nonblocking(true).map_err(PgError::Io)?;
        conn.nonblocking = true;

        Ok(conn)
    }

    /// Connect with a custom I/O timeout.
    pub fn connect_with_timeout(config: &PgConfig, timeout: Duration) -> PgResult<Self> {
        let mut conn = Self::connect(config)?;
        conn.io_timeout = timeout;
        Ok(conn)
    }

    /// Set the application-level I/O timeout.
    pub fn set_io_timeout(&mut self, timeout: Duration) {
        self.io_timeout = timeout;
    }

    /// Get the current I/O timeout.
    pub fn io_timeout(&self) -> Duration {
        self.io_timeout
    }

    /// Set a callback that is invoked when the server sends a NoticeResponse.
    ///
    /// The callback receives `(severity, code, message)`. This is useful for
    /// logging warnings, deprecation notices, etc.
    ///
    /// # Example
    /// ```ignore
    /// conn.set_notice_handler(|severity, code, message| {
    ///     eprintln!("PG {}: {} ({})", severity, message, code);
    /// });
    /// ```
    pub fn set_notice_handler<F>(&mut self, handler: F)
    where
        F: Fn(&str, &str, &str) + Send + Sync + 'static,
    {
        self.notice_handler = Some(Box::new(handler));
    }

    /// Remove the notice handler.
    pub fn clear_notice_handler(&mut self) {
        self.notice_handler = None;
    }

    /// Set the maximum number of statements to cache before LRU eviction.
    pub fn set_statement_cache_capacity(&mut self, capacity: usize) {
        self.stmt_cache.set_max_capacity(capacity);
    }

    /// Return the raw file descriptor for event-loop registration
    /// (epoll / kqueue).
    #[cfg(unix)]
    pub fn raw_fd(&self) -> std::os::unix::io::RawFd {
        self.stream.as_raw_fd()
    }

    /// Check if the socket is in non-blocking mode.
    pub fn is_nonblocking(&self) -> bool {
        self.nonblocking
    }

    /// Set non-blocking mode on the socket.
    pub fn set_nonblocking(&mut self, nonblocking: bool) -> PgResult<()> {
        self.stream
            .set_nonblocking(nonblocking)
            .map_err(PgError::Io)?;
        self.nonblocking = nonblocking;
        Ok(())
    }

    /// Perform the startup and authentication handshake.
    fn startup(&mut self, config: &PgConfig) -> PgResult<()> {
        // Build optional startup parameters
        let mut extra_params: Vec<(&str, &str)> = Vec::new();
        if let Some(ref name) = config.application_name {
            extra_params.push(("application_name", name.as_str()));
        }

        // Send StartupMessage
        self.ensure_write_capacity(512);
        let n = codec::encode_startup(
            &mut self.write_buf,
            &config.user,
            &config.database,
            &extra_params,
        );
        self.stream
            .write_all(&self.write_buf[..n])
            .map_err(PgError::Io)?;

        // Read server response
        loop {
            self.fill_read_buf(None)?;

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let header = codec::decode_header(&self.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::AuthenticationRequest => {
                        let auth_type = codec::read_i32(&self.read_buf, 5);
                        match AuthType::from_i32(auth_type) {
                            Some(AuthType::Ok) => {
                                // Handled! Keep going to ReadyForQuery
                            }
                            Some(AuthType::CleartextPassword) => {
                                let n = codec::encode_password(
                                    &mut self.write_buf,
                                    config.password.as_str(),
                                );
                                self.stream
                                    .write_all(&self.write_buf[..n])
                                    .map_err(PgError::Io)?;
                            }
                            Some(AuthType::SASLInit) => {
                                // B.3: Use SCRAM-SHA-256-PLUS with channel binding when TLS is active
                                #[cfg(feature = "tls")]
                                let (mut scram, mechanism) = if let Some(cb_data) =
                                    self.stream.tls_server_cert_hash()
                                {
                                    (
                                        ScramClient::new_with_channel_binding(
                                            &config.user,
                                            config.password.as_str(),
                                            cb_data,
                                        ),
                                        "SCRAM-SHA-256-PLUS",
                                    )
                                } else {
                                    (
                                        ScramClient::new(&config.user, config.password.as_str()),
                                        "SCRAM-SHA-256",
                                    )
                                };
                                #[cfg(not(feature = "tls"))]
                                let (mut scram, mechanism) = (
                                    ScramClient::new(&config.user, config.password.as_str()),
                                    "SCRAM-SHA-256",
                                );

                                let client_first = scram.client_first_message();
                                let n = codec::encode_sasl_initial(
                                    &mut self.write_buf,
                                    mechanism,
                                    &client_first,
                                );
                                self.stream
                                    .write_all(&self.write_buf[..n])
                                    .map_err(PgError::Io)?;

                                self.consume_read(msg_len);
                                self.wait_for_sasl_continue(&mut scram, config)?;
                                // After SASL, we might still have messages in the buffer
                                // so we don't return, we continue the outer loop.
                                continue;
                            }
                            Some(AuthType::MD5Password) => {
                                // Salt is 4 bytes following the auth_type int32
                                if body.len() < 8 {
                                    return Err(PgError::Protocol(
                                        "MD5Password message too short".to_string(),
                                    ));
                                }
                                let salt: [u8; 4] = [body[4], body[5], body[6], body[7]];
                                let hash = crate::auth::md5_password_hash(
                                    &config.user,
                                    config.password.as_str(),
                                    &salt,
                                );
                                let n = codec::encode_password(&mut self.write_buf, &hash);
                                self.stream
                                    .write_all(&self.write_buf[..n])
                                    .map_err(PgError::Io)?;
                            }
                            _ => {
                                return Err(PgError::Auth(format!(
                                    "Unsupported auth type: {}",
                                    auth_type
                                )));
                            }
                        }
                    }
                    BackendTag::ParameterStatus => {
                        let (name, consumed) = codec::read_cstring(body, 0);
                        let (value, _) = codec::read_cstring(body, consumed);
                        self.server_params
                            .push((name.to_string(), value.to_string()));
                    }
                    BackendTag::BackendKeyData => {
                        self.process_id = codec::read_i32(body, 0);
                        self.secret_key = codec::read_i32(body, 4);
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(()); // Connection is ready!
                    }
                    BackendTag::ErrorResponse => {
                        let fields = codec::parse_error_fields(body);
                        return Err(PgError::from_fields(&fields));
                    }
                    _ => {
                        // Skip unknown messages
                    }
                }
                self.consume_read(msg_len);
            }
        }
    }

    /// Handle SASL Continue/Final exchange.
    fn wait_for_sasl_continue(
        &mut self,
        scram: &mut ScramClient,
        _config: &PgConfig,
    ) -> PgResult<()> {
        loop {
            self.fill_read_buf(None)?;

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let header = codec::decode_header(&self.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.read_buf[5..msg_len].to_vec();

                match header.tag {
                    BackendTag::AuthenticationRequest => {
                        let auth_type = codec::read_i32(&self.read_buf, 5);
                        match AuthType::from_i32(auth_type) {
                            Some(AuthType::SASLContinue) => {
                                let server_first = &body[4..];
                                let client_final = scram
                                    .process_server_first(server_first)
                                    .map_err(PgError::Auth)?;

                                let n =
                                    codec::encode_sasl_response(&mut self.write_buf, &client_final);
                                self.stream
                                    .write_all(&self.write_buf[..n])
                                    .map_err(PgError::Io)?;
                            }
                            Some(AuthType::SASLFinal) => {
                                let server_final = &body[4..];
                                scram
                                    .verify_server_final(server_final)
                                    .map_err(PgError::Auth)?;
                            }
                            Some(AuthType::Ok) => {
                                self.consume_read(msg_len);
                                return Ok(());
                            }
                            _ => {
                                return Err(PgError::Auth(
                                    "Unexpected auth message during SASL".to_string(),
                                ));
                            }
                        }
                    }
                    _ => {
                        // Skip
                    }
                }
                self.consume_read(msg_len);
            }
        }
    }

    // ─── Query Methods ────────────────────────────────────────

    /// Execute a simple query (no parameters). Returns all result rows.
    pub fn query_simple(&mut self, sql: &str) -> PgResult<Vec<Row>> {
        self.ensure_write_capacity(5 + sql.len());
        let n = codec::encode_query(&mut self.write_buf, sql);
        self.flush_write_buf(n)?;
        self.read_query_results()
    }

    /// Execute a parameterized query using the Extended Query Protocol.
    /// Uses implicit statement caching for performance.
    pub fn query(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Vec<Row>> {
        let stmt = self.stmt_cache.get_or_create(sql);

        // Conservative upper bound for write buffer
        let estimated = 10 + sql.len() + (params.len() * 256);
        self.ensure_write_capacity(estimated);

        let mut pos = 0;

        if stmt.is_new {
            // Parse
            let n = codec::encode_parse(&mut self.write_buf[pos..], &stmt.name, sql, &[]);
            pos += n;

            // Describe (to get column info)
            let n = codec::encode_describe(
                &mut self.write_buf[pos..],
                DescribeTarget::Statement,
                &stmt.name,
            );
            pos += n;
        }

        // Bind — encode parameters with per-parameter format codes
        let pg_values: Vec<PgValue> = params.iter().map(|p| p.to_sql()).collect();
        let param_formats: Vec<i16> = pg_values
            .iter()
            .map(|v| if v.prefers_binary() { 1_i16 } else { 0_i16 })
            .collect();
        let param_values: Vec<Option<Vec<u8>>> = pg_values
            .iter()
            .zip(param_formats.iter())
            .map(|(v, &fmt)| {
                if fmt == 1 {
                    v.to_binary_bytes()
                } else {
                    v.to_text_bytes()
                }
            })
            .collect();
        let param_refs: Vec<Option<&[u8]>> = param_values.iter().map(|p| p.as_deref()).collect();
        let n = codec::encode_bind(
            &mut self.write_buf[pos..],
            "", // unnamed portal
            &stmt.name,
            &param_formats,
            &param_refs,
            &[1], // request all results in binary format
        );
        pos += n;

        // Execute
        let n = codec::encode_execute(&mut self.write_buf[pos..], "", 0);
        pos += n;

        // Sync
        let n = codec::encode_sync(&mut self.write_buf[pos..]);
        pos += n;

        self.flush_write_buf(pos)?;

        // Read results
        let rows = self.read_extended_results(sql, &stmt.name, stmt.is_new, stmt.columns)?;
        Ok(rows)
    }

    /// Execute a query expecting exactly one row.
    ///
    /// Returns `Ok(Row)` when exactly one row is returned, `Err(NoRows)` when
    /// zero rows are returned, and `Err(TooManyRows)` when more than one row
    /// is returned.  Use [`query_opt`] when zero rows are acceptable.
    pub fn query_one(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Row> {
        let mut rows = self.query(sql, params)?;
        match rows.len() {
            0 => Err(PgError::NoRows),
            1 => Ok(rows.remove(0)),
            _ => Err(PgError::TooManyRows),
        }
    }

    /// Execute a query expecting zero or one row.
    ///
    /// Returns `Ok(None)` when zero rows are returned, `Ok(Some(Row))` when
    /// exactly one row is returned, and `Err(TooManyRows)` when more than one
    /// row is returned.
    pub fn query_opt(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Option<Row>> {
        let mut rows = self.query(sql, params)?;
        match rows.len() {
            0 => Ok(None),
            1 => Ok(Some(rows.remove(0))),
            _ => Err(PgError::TooManyRows),
        }
    }

    /// Execute a statement that returns no rows (INSERT, UPDATE, DELETE).
    /// Returns the number of affected rows as reported by the server.
    pub fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<u64> {
        let _rows = self.query(sql, params)?;
        Ok(self.last_affected_rows)
    }

    // ─── Transaction Support ──────────────────────────────────

    /// Begin a transaction.
    pub fn begin(&mut self) -> PgResult<()> {
        self.query_simple("BEGIN")?;
        Ok(())
    }

    /// Commit the current transaction.
    pub fn commit(&mut self) -> PgResult<()> {
        self.query_simple("COMMIT")?;
        Ok(())
    }

    /// Rollback the current transaction.
    pub fn rollback(&mut self) -> PgResult<()> {
        self.query_simple("ROLLBACK")?;
        Ok(())
    }

    /// Create a savepoint.
    pub fn savepoint(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("SAVEPOINT {}", name))?;
        Ok(())
    }

    /// Rollback to a savepoint.
    pub fn rollback_to(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("ROLLBACK TO SAVEPOINT {}", name))?;
        Ok(())
    }

    /// Release a savepoint.
    pub fn release_savepoint(&mut self, name: &str) -> PgResult<()> {
        self.query_simple(&format!("RELEASE SAVEPOINT {}", name))?;
        Ok(())
    }

    /// Execute a closure within a transaction.
    ///
    /// Automatically BEGINs before calling `f`, COMMITs on success,
    /// and ROLLBACKs on error. This ensures the transaction is always
    /// finalized, even if the closure panics (via Drop).
    ///
    /// # Example
    /// ```ignore
    /// conn.transaction(|tx| {
    ///     tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Alice"])?;
    ///     tx.execute("INSERT INTO logs (msg) VALUES ($1)", &[&"User created"])?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction<F, T>(&mut self, f: F) -> PgResult<T>
    where
        F: FnOnce(&mut Transaction<'_>) -> PgResult<T>,
    {
        self.begin()?;
        let mut tx = Transaction {
            conn: self,
            finished: false,
            savepoint_name: None,
            savepoint_counter: 0,
        };
        match f(&mut tx) {
            Ok(val) => {
                tx.commit()?;
                Ok(val)
            }
            Err(e) => {
                // Attempt rollback, but propagate original error
                let _ = tx.rollback();
                Err(e)
            }
        }
    }

    // ─── COPY Protocol ────────────────────────────────────────

    /// Start a COPY FROM STDIN operation.
    pub fn copy_in(&mut self, sql: &str) -> PgResult<CopyWriter<'_>> {
        let n = codec::encode_query(&mut self.write_buf, sql);
        #[allow(clippy::unnecessary_to_owned)]
        self.write_all(&self.write_buf[..n].to_vec())?;

        // Read until CopyInResponse
        loop {
            self.fill_read_buf(None)?;
            let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? else {
                continue;
            };
            let header = codec::decode_header(&self.read_buf)
                .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
            match header.tag {
                BackendTag::CopyInResponse => {
                    self.consume_read(msg_len);
                    return Ok(CopyWriter { conn: self });
                }
                BackendTag::ErrorResponse => {
                    let body = &self.read_buf[5..msg_len];
                    return Err(self.parse_error(body));
                }
                _ => {
                    self.consume_read(msg_len);
                }
            }
        }
    }

    /// Start a COPY TO STDOUT operation.
    /// Returns a CopyReader that yields data chunks.
    pub fn copy_out(&mut self, sql: &str) -> PgResult<CopyReader<'_>> {
        let n = codec::encode_query(&mut self.write_buf, sql);
        #[allow(clippy::unnecessary_to_owned)]
        self.write_all(&self.write_buf[..n].to_vec())?;

        // Read until CopyOutResponse
        loop {
            self.fill_read_buf(None)?;
            let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? else {
                continue;
            };
            let header = codec::decode_header(&self.read_buf)
                .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
            match header.tag {
                BackendTag::CopyOutResponse => {
                    self.consume_read(msg_len);
                    return Ok(CopyReader {
                        conn: self,
                        done: false,
                    });
                }
                BackendTag::ErrorResponse => {
                    let body = &self.read_buf[5..msg_len];
                    return Err(self.parse_error(body));
                }
                _ => {
                    self.consume_read(msg_len);
                }
            }
        }
    }

    // ─── LISTEN / NOTIFY ──────────────────────────────────────

    /// Subscribe to a notification channel.
    pub fn listen(&mut self, channel: &str) -> PgResult<()> {
        self.query_simple(&format!("LISTEN {}", channel))?;
        Ok(())
    }

    /// Send a notification.
    pub fn notify(&mut self, channel: &str, payload: &str) -> PgResult<()> {
        self.query_simple(&format!("NOTIFY {}, '{}'", channel, payload))?;
        Ok(())
    }

    /// Unsubscribe from a notification channel.
    pub fn unlisten(&mut self, channel: &str) -> PgResult<()> {
        self.query_simple(&format!("UNLISTEN {}", channel))?;
        Ok(())
    }

    /// Unsubscribe from all notification channels.
    pub fn unlisten_all(&mut self) -> PgResult<()> {
        self.query_simple("UNLISTEN *")?;
        Ok(())
    }

    /// Drain and return all buffered notifications.
    pub fn drain_notifications(&mut self) -> Vec<Notification> {
        self.notifications.drain(..).collect()
    }

    /// Check if there are buffered notifications.
    pub fn has_notifications(&self) -> bool {
        !self.notifications.is_empty()
    }

    /// Get the number of buffered notifications.
    pub fn notification_count(&self) -> usize {
        self.notifications.len()
    }

    /// Poll for a notification (always non-blocking).
    /// Reads from the socket and returns the first notification found,
    /// or None if no notification is immediately available.
    pub fn poll_notification(&mut self) -> PgResult<Option<Notification>> {
        // First check buffer
        if let Some(n) = self.notifications.pop_front() {
            return Ok(Some(n));
        }

        // Try a non-blocking read (socket is already non-blocking)
        self.ensure_read_space();
        match self.stream.read(&mut self.read_buf[self.read_pos..]) {
            Ok(0) => return Err(PgError::ConnectionClosed),
            Ok(n) => {
                self.read_pos += n;
                // Process any complete messages
                while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])?
                {
                    let header = codec::decode_header(&self.read_buf).ok_or_else(|| {
                        PgError::Protocol("Incomplete message header".to_string())
                    })?;
                    if header.tag == BackendTag::NotificationResponse {
                        let body = &self.read_buf[5..msg_len];
                        let notification = Self::parse_notification(body);
                        self.notifications.push_back(notification);
                    }
                    self.consume_read(msg_len);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available
            }
            Err(e) => return Err(PgError::Io(e)),
        }

        Ok(self.notifications.pop_front())
    }

    // ─── Accessors ────────────────────────────────────────────

    /// Get the current transaction status.
    pub fn transaction_status(&self) -> TransactionStatus {
        self.tx_status
    }

    /// Get the number of cached statements.
    pub fn cached_statements(&self) -> usize {
        self.stmt_cache.len()
    }

    /// Get the number of rows affected by the last command.
    pub fn last_affected_rows(&self) -> u64 {
        self.last_affected_rows
    }

    /// Get the last CommandComplete tag string.
    pub fn last_command_tag(&self) -> &str {
        &self.last_command_tag
    }

    /// Get the backend process ID.
    pub fn process_id(&self) -> i32 {
        self.process_id
    }

    /// Get the backend secret key (used for cancel requests).
    pub fn secret_key(&self) -> i32 {
        self.secret_key
    }

    /// Get server parameters received during startup.
    pub fn server_params(&self) -> &[(String, String)] {
        &self.server_params
    }

    /// Get a specific server parameter by name.
    pub fn server_param(&self, name: &str) -> Option<&str> {
        self.server_params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Check if the connection is in a transaction.
    pub fn in_transaction(&self) -> bool {
        matches!(
            self.tx_status,
            TransactionStatus::InTransaction | TransactionStatus::Failed
        )
    }

    /// Clear the statement cache and deallocate all server-side prepared statements.
    ///
    /// Sends `DEALLOCATE ALL` to the server before clearing the client-side
    /// cache.  The statement name counter is preserved to prevent name
    /// collisions with any stale server-side references.
    pub fn clear_statement_cache(&mut self) {
        let _ = self.query_simple("DEALLOCATE ALL");
        self.stmt_cache.clear();
    }

    /// Returns `true` if the connection has been marked as broken due to a
    /// fatal I/O error.  A broken connection should be discarded (not
    /// returned to the pool).
    pub fn is_broken(&self) -> bool {
        self.broken
    }

    /// Reset the connection to a clean state for pool reuse.
    ///
    /// Sends `DISCARD ALL` which resets session state, deallocates prepared
    /// statements, closes cursors, drops temps, releases advisory locks.
    /// Then clears the client-side statement cache.
    pub fn reset(&mut self) -> PgResult<()> {
        self.query_simple("DISCARD ALL")?;
        self.stmt_cache.clear();
        Ok(())
    }

    /// Execute one or more SQL statements separated by semicolons, using
    /// the Simple Query Protocol.  Returns the number of affected rows from
    /// the **last** command.
    ///
    /// This is useful for running DDL migrations, multi-statement scripts,
    /// or any sequence of commands that don't require parameters.
    ///
    /// # Example
    /// ```ignore
    /// conn.execute_batch("CREATE TABLE t(id INT); INSERT INTO t VALUES (1); INSERT INTO t VALUES (2);")?;
    /// ```
    pub fn execute_batch(&mut self, sql: &str) -> PgResult<u64> {
        self.query_simple(sql)?;
        Ok(self.last_affected_rows)
    }

    /// Check if the connection is alive by sending a simple query.
    pub fn is_alive(&mut self) -> bool {
        self.query_simple("SELECT 1").is_ok()
    }

    // ─── Streaming Cursor ─────────────────────────────────────

    /// Open a streaming cursor over a parameterized query.
    ///
    /// Uses a PostgreSQL **named portal** to stream rows in batches without
    /// buffering the entire result set in memory. Ideal for large result sets
    /// where collecting all rows upfront would be impractical.
    ///
    /// The connection is exclusively borrowed for the lifetime of the cursor;
    /// no other queries can run until the cursor is dropped.
    ///
    /// # Parameters
    /// - `fetch_size`: number of rows to retrieve per [`Cursor::fetch`] call.
    ///   A value of `0` is clamped to `1`.
    ///
    /// # Example
    /// ```ignore
    /// let mut cursor = conn.query_cursor(
    ///     "SELECT id, email FROM users WHERE active = $1",
    ///     &[&true],
    ///     100,
    /// )?;
    /// while let batch = cursor.fetch()? {
    ///     if batch.is_empty() { break; }
    ///     for row in &batch { /* process row */ }
    /// }
    /// ```
    pub fn query_cursor(
        &mut self,
        sql: &str,
        params: &[&dyn ToSql],
        fetch_size: usize,
    ) -> PgResult<Cursor<'_>> {
        let stmt = self.stmt_cache.get_or_create(sql);
        self.portal_counter = self.portal_counter.wrapping_add(1);
        let portal_name = format!("p{}", self.portal_counter);

        let estimated = 30 + sql.len() + params.len() * 256 + portal_name.len() * 2 + 50;
        self.ensure_write_capacity(estimated);

        let mut pos = 0;
        let stmt_name = stmt.name.clone();
        let is_new = stmt.is_new;
        let cached_columns = stmt.columns.clone();

        if is_new {
            let n = codec::encode_parse(&mut self.write_buf[pos..], &stmt_name, sql, &[]);
            pos += n;
            let n = codec::encode_describe(
                &mut self.write_buf[pos..],
                DescribeTarget::Statement,
                &stmt_name,
            );
            pos += n;
        }

        // Bind to the named portal (not the unnamed "").
        let pg_values: Vec<PgValue> = params.iter().map(|p| p.to_sql()).collect();
        let param_formats: Vec<i16> = pg_values
            .iter()
            .map(|v| if v.prefers_binary() { 1_i16 } else { 0_i16 })
            .collect();
        let param_values: Vec<Option<Vec<u8>>> = pg_values
            .iter()
            .zip(param_formats.iter())
            .map(|(v, &fmt)| {
                if fmt == 1 {
                    v.to_binary_bytes()
                } else {
                    v.to_text_bytes()
                }
            })
            .collect();
        let param_refs: Vec<Option<&[u8]>> = param_values.iter().map(|p| p.as_deref()).collect();

        let n = codec::encode_bind(
            &mut self.write_buf[pos..],
            &portal_name,
            &stmt_name,
            &param_formats,
            &param_refs,
            &[1], // request binary-format results
        );
        pos += n;

        // Flush (not Sync) — the portal stays alive for subsequent Execute calls.
        let n = codec::encode_flush(&mut self.write_buf[pos..]);
        pos += n;
        self.flush_write_buf(pos)?;

        // Read ParseComplete?, ParameterDescription?, RowDescription/NoData, BindComplete.
        let columns = self.read_cursor_bind(sql, &stmt_name, is_new, cached_columns)?;

        Ok(Cursor {
            conn: self,
            portal_name,
            fetch_size: fetch_size.max(1),
            exhausted: false,
            columns: Rc::new(columns),
        })
    }

    /// Internal: read server responses after Bind for a named portal.
    ///
    /// Drains ParseComplete?, ParameterDescription?, RowDescription/NoData,
    /// and returns when BindComplete is received, giving back the column layout.
    fn read_cursor_bind(
        &mut self,
        sql: &str,
        stmt_name: &str,
        is_new: bool,
        cached_columns: Option<Vec<codec::ColumnDesc>>,
    ) -> PgResult<Vec<codec::ColumnDesc>> {
        let mut columns: Vec<codec::ColumnDesc> = cached_columns.unwrap_or_default();

        loop {
            if codec::message_complete(&self.read_buf[..self.read_pos])?.is_none() {
                self.fill_read_buf(None)?;
            }
            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let tag = BackendTag::from(self.read_buf[0]);
                match tag {
                    BackendTag::ParseComplete | BackendTag::ParameterDescription => {
                        self.consume_read(msg_len);
                    }
                    BackendTag::RowDescription => {
                        let mut cols = {
                            let body = &self.read_buf[5..msg_len];
                            codec::parse_row_description(body)
                        };
                        for col in &mut cols {
                            col.format_code = FormatCode::Binary;
                        }
                        if is_new {
                            let sname = stmt_name.to_string();
                            if let Some(evicted) =
                                self.stmt_cache.insert(sql, sname, 0, Some(cols.clone()))
                            {
                                self.close_statement_on_server(&evicted.name);
                            }
                        }
                        columns = cols;
                        self.consume_read(msg_len);
                    }
                    BackendTag::NoData => {
                        if is_new {
                            let sname = stmt_name.to_string();
                            if let Some(evicted) = self.stmt_cache.insert(sql, sname, 0, None) {
                                self.close_statement_on_server(&evicted.name);
                            }
                        }
                        self.consume_read(msg_len);
                    }
                    BackendTag::BindComplete => {
                        self.consume_read(msg_len);
                        return Ok(columns);
                    }
                    BackendTag::ErrorResponse => {
                        let err = {
                            let body = &self.read_buf[5..msg_len];
                            self.parse_error_with_context(body, sql)
                        };
                        self.consume_read(msg_len);
                        // Sent a Flush (not Sync); restore clean protocol state.
                        let n = codec::encode_sync(&mut self.write_buf);
                        let _ = self.flush_write_buf(n);
                        let _ = self.drain_to_ready();
                        return Err(err);
                    }
                    BackendTag::NoticeResponse => {
                        let body = &self.read_buf[5..msg_len];
                        self.dispatch_notice(body);
                        self.consume_read(msg_len);
                    }
                    _ => {
                        self.consume_read(msg_len);
                    }
                }
            }
        }
    }

    // ─── Query Pipelining ─────────────────────────────────────

    /// Execute multiple queries as a single pipelined batch.
    ///
    /// All queries are serialised into one write to the server and their
    /// results are read back in order. This eliminates per-query
    /// round-trip latency for workloads that require many sequential
    /// queries.
    ///
    /// Statement caching is applied per query: new statements are
    /// automatically parsed and described; cached statements are reused.
    ///
    /// # Returns
    /// `Vec<Vec<Row>>` — one inner `Vec<Row>` per query, in order.
    ///
    /// # Error behaviour
    /// The first query error aborts the pipeline. Results for prior queries
    /// in the same pipeline that already completed are **not** rolled back
    /// unless they were inside the same transaction. Wrap in
    /// [`transaction`](Self::transaction) for atomicity.
    ///
    /// # Example
    /// ```ignore
    /// let results = conn.pipeline(&[
    ///     ("SELECT $1::int4 + $2::int4", &[&1_i32, &2_i32] as &[&dyn ToSql]),
    ///     ("SELECT $1::text", &[&"hello"]),
    /// ])?;
    /// ```
    pub fn pipeline(&mut self, queries: &[(&str, &[&dyn ToSql])]) -> PgResult<Vec<Vec<Row>>> {
        if queries.is_empty() {
            return Ok(Vec::new());
        }

        // Estimate write buffer: header + sql + params per query.
        let estimated: usize = queries
            .iter()
            .map(|(sql, params)| 50 + sql.len() + params.len() * 256)
            .sum::<usize>()
            + 10;
        self.ensure_write_capacity(estimated);

        let mut pos = 0;

        // Collect (stmt_name, is_new, cached_columns) before encoding anything.
        let mut stmt_infos: Vec<(String, bool, Option<Vec<codec::ColumnDesc>>)> =
            Vec::with_capacity(queries.len());

        for (sql, params) in queries {
            let stmt = self.stmt_cache.get_or_create(sql);
            let stmt_name = stmt.name.clone();
            let is_new = stmt.is_new;
            let cached = stmt.columns.clone();

            if is_new {
                let n = codec::encode_parse(&mut self.write_buf[pos..], &stmt_name, sql, &[]);
                pos += n;
                let n = codec::encode_describe(
                    &mut self.write_buf[pos..],
                    DescribeTarget::Statement,
                    &stmt_name,
                );
                pos += n;
            }

            let pg_values: Vec<PgValue> = params.iter().map(|p| p.to_sql()).collect();
            let param_formats: Vec<i16> = pg_values
                .iter()
                .map(|v| if v.prefers_binary() { 1_i16 } else { 0_i16 })
                .collect();
            let param_values: Vec<Option<Vec<u8>>> = pg_values
                .iter()
                .zip(param_formats.iter())
                .map(|(v, &fmt)| {
                    if fmt == 1 {
                        v.to_binary_bytes()
                    } else {
                        v.to_text_bytes()
                    }
                })
                .collect();
            let param_refs: Vec<Option<&[u8]>> =
                param_values.iter().map(|p| p.as_deref()).collect();

            let n = codec::encode_bind(
                &mut self.write_buf[pos..],
                "", // unnamed portal — discarded after Execute
                &stmt_name,
                &param_formats,
                &param_refs,
                &[1], // binary results
            );
            pos += n;
            let n = codec::encode_execute(&mut self.write_buf[pos..], "", 0);
            pos += n;

            stmt_infos.push((stmt_name, is_new, cached));
        }

        // Single Sync terminates the whole pipeline.
        let n = codec::encode_sync(&mut self.write_buf[pos..]);
        pos += n;
        self.flush_write_buf(pos)?;

        // Read all pipeline results.
        let mut results: Vec<Vec<Row>> = (0..queries.len()).map(|_| Vec::new()).collect();
        let mut query_idx: usize = 0;
        let mut current_columns: Rc<Vec<codec::ColumnDesc>> = Rc::new(Vec::new());
        let mut got_row_desc = false;

        loop {
            if codec::message_complete(&self.read_buf[..self.read_pos])?.is_none() {
                self.fill_read_buf(None)?;
            }
            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let tag = BackendTag::from(self.read_buf[0]);
                match tag {
                    BackendTag::ParseComplete | BackendTag::ParameterDescription => {
                        self.consume_read(msg_len);
                    }
                    BackendTag::RowDescription => {
                        let mut cols = {
                            let body = &self.read_buf[5..msg_len];
                            codec::parse_row_description(body)
                        };
                        for col in &mut cols {
                            col.format_code = FormatCode::Binary;
                        }
                        let is_new = stmt_infos[query_idx].1;
                        if is_new {
                            let sql = queries[query_idx].0;
                            let sname = stmt_infos[query_idx].0.clone();
                            if let Some(evicted) =
                                self.stmt_cache.insert(sql, sname, 0, Some(cols.clone()))
                            {
                                self.close_statement_on_server(&evicted.name);
                            }
                            stmt_infos[query_idx].1 = false;
                        }
                        current_columns = Rc::new(cols);
                        got_row_desc = true;
                        self.consume_read(msg_len);
                    }
                    BackendTag::NoData => {
                        let is_new = stmt_infos[query_idx].1;
                        if is_new {
                            let sql = queries[query_idx].0;
                            let sname = stmt_infos[query_idx].0.clone();
                            if let Some(evicted) = self.stmt_cache.insert(sql, sname, 0, None) {
                                self.close_statement_on_server(&evicted.name);
                            }
                            stmt_infos[query_idx].1 = false;
                        }
                        self.consume_read(msg_len);
                    }
                    BackendTag::BindComplete => {
                        if !got_row_desc {
                            // Cached statement — restore columns from the cache snapshot.
                            if let Some(ref cols) = stmt_infos[query_idx].2 {
                                current_columns = Rc::new(cols.clone());
                            }
                        }
                        got_row_desc = false;
                        self.consume_read(msg_len);
                    }
                    BackendTag::DataRow => {
                        let raw_values = {
                            let body = &self.read_buf[5..msg_len];
                            codec::parse_data_row(body)
                        };
                        if query_idx < results.len() {
                            results[query_idx]
                                .push(Row::new(Rc::clone(&current_columns), raw_values));
                        }
                        self.consume_read(msg_len);
                    }
                    BackendTag::CommandComplete | BackendTag::EmptyQueryResponse => {
                        if tag == BackendTag::CommandComplete {
                            let (ctag, affected) = {
                                let body = &self.read_buf[5..msg_len];
                                extract_command_complete(body)
                            };
                            self.last_command_tag = ctag;
                            self.last_affected_rows = affected;
                        }
                        self.consume_read(msg_len);
                        query_idx += 1;
                        current_columns = Rc::new(Vec::new());
                        got_row_desc = false;
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(self.read_buf[5]);
                        self.consume_read(msg_len);
                        return Ok(results);
                    }
                    BackendTag::ErrorResponse => {
                        let sql = if query_idx < queries.len() {
                            queries[query_idx].0
                        } else {
                            ""
                        };
                        let err = {
                            let body = &self.read_buf[5..msg_len];
                            self.parse_error_with_context(body, sql)
                        };
                        self.consume_read(msg_len);
                        self.drain_to_ready()?;
                        return Err(err);
                    }
                    BackendTag::NotificationResponse => {
                        let notification = {
                            let body = &self.read_buf[5..msg_len];
                            Self::parse_notification(body)
                        };
                        self.notifications.push_back(notification);
                        self.consume_read(msg_len);
                    }
                    BackendTag::NoticeResponse => {
                        let body = &self.read_buf[5..msg_len];
                        self.dispatch_notice(body);
                        self.consume_read(msg_len);
                    }
                    _ => {
                        self.consume_read(msg_len);
                    }
                }
            }
        }
    }

    /// Return statement cache statistics (hits, misses, evictions, size).
    pub fn cache_stats(&self) -> CacheStats {
        self.stmt_cache.stats()
    }

    /// Create a cancel token for this connection.
    ///
    /// The returned [`CancelToken`] can be cloned and sent to any thread or
    /// worker.  Calling [`CancelToken::send`] will interrupt a query that is
    /// currently executing on this connection by sending a `CancelRequest`
    /// over a fresh, short-lived TCP connection to the server.
    pub fn cancel_token(&self) -> CancelToken {
        CancelToken {
            host: self.cancel_host.clone(),
            port: self.cancel_port,
            socket_dir: self.cancel_socket_dir.clone(),
            process_id: self.process_id,
            secret_key: self.secret_key,
        }
    }

    // ─── Internal Methods ─────────────────────────────────────

    // ─── Non-blocking read/write primitives ───────────────────

    /// Try to read data into the read buffer without blocking.
    /// Returns `Ok(n)` with bytes read, or `Err(PgError::WouldBlock)` if no
    /// data is available, or another error on failure.
    pub fn try_fill_read_buf(&mut self) -> PgResult<usize> {
        self.ensure_read_space();

        match self.stream.read(&mut self.read_buf[self.read_pos..]) {
            Ok(0) => {
                self.broken = true;
                Err(PgError::ConnectionClosed)
            }
            Ok(n) => {
                self.read_pos += n;
                Ok(n)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(PgError::WouldBlock),
            Err(e) => {
                self.broken = true;
                Err(PgError::Io(e))
            }
        }
    }

    /// Try to write a buffer to the socket without blocking.
    /// Returns `Ok(n)` with bytes written, or `Err(PgError::WouldBlock)` if the
    /// socket is not writable.
    pub fn try_write(&mut self, data: &[u8]) -> PgResult<usize> {
        match self.stream.write(data) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(PgError::WouldBlock),
            Err(e) => {
                self.broken = true;
                Err(PgError::Io(e))
            }
        }
    }

    /// Wait for the socket to become readable using `poll(2)`.
    ///
    /// This is a true OS-level wait — the thread sleeps in the kernel until
    /// the socket has data or the timeout expires.  No busy-waiting, no
    /// `thread::sleep`.
    #[cfg(unix)]
    fn wait_readable(&self, timeout: Duration) -> PgResult<()> {
        let fd = self.stream.as_raw_fd();
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ret < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(()); // EINTR — caller will retry
            }
            return Err(PgError::Io(e));
        }
        if ret == 0 {
            return Err(PgError::Timeout);
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(PgError::ConnectionClosed);
        }
        Ok(())
    }

    /// Wait for the socket to become writable using `poll(2)`.
    #[cfg(unix)]
    fn wait_writable(&self, timeout: Duration) -> PgResult<()> {
        let fd = self.stream.as_raw_fd();
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ret < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(PgError::Io(e));
        }
        if ret == 0 {
            return Err(PgError::Timeout);
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(PgError::ConnectionClosed);
        }
        Ok(())
    }

    /// Poll the socket for readability with a timeout.
    ///
    /// Uses the OS `poll(2)` syscall for efficient, zero-waste waiting.
    /// The thread sleeps in the kernel until data arrives or the timeout
    /// expires — no busy-loop, no `thread::sleep`.
    pub fn poll_read(&mut self, timeout: Duration) -> PgResult<usize> {
        let start = Instant::now();
        loop {
            match self.try_fill_read_buf() {
                Ok(n) => return Ok(n),
                Err(PgError::WouldBlock) => {
                    let elapsed = start.elapsed();
                    if elapsed >= timeout {
                        return Err(PgError::Timeout);
                    }
                    #[cfg(unix)]
                    self.wait_readable(timeout - elapsed)?;
                    #[cfg(not(unix))]
                    std::thread::sleep(Duration::from_micros(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Poll the socket for writability with a timeout.
    /// Writes all of `data` or times out.
    pub fn poll_write(&mut self, data: &[u8], timeout: Duration) -> PgResult<()> {
        let start = Instant::now();
        let mut written = 0;
        while written < data.len() {
            match self.try_write(&data[written..]) {
                Ok(n) => written += n,
                Err(PgError::WouldBlock) => {
                    let elapsed = start.elapsed();
                    if elapsed >= timeout {
                        return Err(PgError::Timeout);
                    }
                    #[cfg(unix)]
                    self.wait_writable(timeout - elapsed)?;
                    #[cfg(not(unix))]
                    std::thread::sleep(Duration::from_micros(50));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Internal: fill the read buffer, blocking with the connection's
    /// configured timeout. This is the workhorse used by query methods.
    fn fill_read_buf(&mut self, min_size: Option<usize>) -> PgResult<()> {
        if let Some(min) = min_size {
            self.ensure_read_capacity(min);
        }

        self.ensure_read_space();

        if self.nonblocking {
            // Use poll_read with timeout
            self.poll_read(self.io_timeout)?;
        } else {
            // Blocking path (used during startup before we switch to NB)
            let n = self
                .stream
                .read(&mut self.read_buf[self.read_pos..])
                .map_err(PgError::Io)?;
            if n == 0 {
                return Err(PgError::ConnectionClosed);
            }
            self.read_pos += n;
        }
        Ok(())
    }

    /// Internal: write all bytes to the socket, respecting non-blocking mode.
    fn write_all(&mut self, data: &[u8]) -> PgResult<()> {
        if self.nonblocking {
            self.poll_write(data, self.io_timeout)
        } else {
            self.stream.write_all(data).map_err(PgError::Io)
        }
    }

    /// Internal: flush the first `n` bytes of `self.write_buf` to the stream.
    ///
    /// This avoids the `.to_vec()` copy that was previously needed to work
    /// around borrow-checker limitations when `self.write_buf` is the source
    /// and `self.write_all()` takes `&mut self`.  By inlining the write loop
    /// here, the compiler can see that `stream` and `write_buf` are disjoint
    /// fields (split borrow).
    fn flush_write_buf(&mut self, n: usize) -> PgResult<()> {
        if self.nonblocking {
            let timeout = self.io_timeout;
            let start = Instant::now();
            let mut written = 0;
            while written < n {
                match self.stream.write(&self.write_buf[written..n]) {
                    Ok(w) => written += w,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        let elapsed = start.elapsed();
                        if elapsed >= timeout {
                            return Err(PgError::Timeout);
                        }
                        #[cfg(unix)]
                        self.wait_writable(timeout - elapsed)?;
                        #[cfg(not(unix))]
                        std::thread::sleep(Duration::from_micros(50));
                    }
                    Err(e) => {
                        self.broken = true;
                        return Err(PgError::Io(e));
                    }
                }
            }
            Ok(())
        } else {
            self.stream
                .write_all(&self.write_buf[..n])
                .map_err(PgError::Io)
        }
    }

    /// Ensure there is room in read_buf for at least one read call.
    fn ensure_read_space(&mut self) {
        if self.read_pos == self.read_buf.len() {
            if self.read_pos >= 5
                && let Some(header) = codec::decode_header(&self.read_buf)
            {
                // Guard against malicious or corrupt servers advertising a
                // length that exceeds our safety limit — do not allocate.
                // The overflow will be caught as Err(BufferOverflow) by the
                // next call to message_complete().
                if header.length as usize > codec::MAX_MESSAGE_SIZE {
                    return;
                }
                let total = 1 + header.length as usize;
                self.ensure_read_capacity(total - self.read_pos);
                return;
            }
            self.ensure_read_capacity(8192);
        }
    }

    fn consume_read(&mut self, n: usize) {
        self.read_buf.copy_within(n..self.read_pos, 0);
        self.read_pos -= n;
    }

    fn ensure_read_capacity(&mut self, additional: usize) {
        if self.read_pos + additional > self.read_buf.len() {
            let new_len = (self.read_pos + additional).max(self.read_buf.len() * 2);
            self.read_buf.resize(new_len, 0);
        }
    }

    fn ensure_write_capacity(&mut self, additional: usize) {
        if additional > self.write_buf.len() {
            let new_len = additional.max(self.write_buf.len() * 2);
            self.write_buf.resize(new_len, 0);
        }
    }

    fn read_query_results(&mut self) -> PgResult<Vec<Row>> {
        let mut rows = Vec::new();
        let mut columns_rc: Rc<Vec<codec::ColumnDesc>> = Rc::new(Vec::new());

        loop {
            if codec::message_complete(&self.read_buf[..self.read_pos])?.is_none() {
                self.fill_read_buf(None)?;
            }

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let header = codec::decode_header(&self.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::RowDescription => {
                        columns_rc = Rc::new(codec::parse_row_description(body));
                    }
                    BackendTag::DataRow => {
                        let raw_values = codec::parse_data_row(body);
                        rows.push(Row::new(Rc::clone(&columns_rc), raw_values));
                    }
                    BackendTag::CommandComplete => {
                        let (tag, rows_affected) = extract_command_complete(body);
                        self.last_command_tag = tag;
                        self.last_affected_rows = rows_affected;
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.parse_error(body);
                        self.consume_read(msg_len);
                        // Drain to ReadyForQuery
                        self.drain_to_ready()?;
                        return Err(err);
                    }
                    BackendTag::NotificationResponse => {
                        let notification = Self::parse_notification(body);
                        self.notifications.push_back(notification);
                    }
                    BackendTag::EmptyQueryResponse => {}
                    BackendTag::NoticeResponse => {
                        self.dispatch_notice(body);
                    }
                    _ => {}
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn read_extended_results(
        &mut self,
        sql: &str,
        stmt_name: &str,
        is_new: bool,
        cached_columns: Option<Vec<codec::ColumnDesc>>,
    ) -> PgResult<Vec<Row>> {
        let mut rows = Vec::new();
        let mut columns_rc: Rc<Vec<codec::ColumnDesc>> = match cached_columns {
            Some(c) => Rc::new(c),
            None => Rc::new(Vec::new()),
        };

        loop {
            if codec::message_complete(&self.read_buf[..self.read_pos])?.is_none() {
                self.fill_read_buf(None)?;
            }

            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let header = codec::decode_header(&self.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::ParseComplete => {}
                    BackendTag::ParameterDescription => {}
                    BackendTag::RowDescription => {
                        let mut columns = codec::parse_row_description(body);
                        // The driver always requests binary-format results in the
                        // Bind message (&[1]).  RowDescription from a Describe
                        // *Statement* always has format_code = Text (0x0) because
                        // no Bind has occurred yet.  Override every column to
                        // Binary so DataRow bytes are decoded correctly.
                        for col in &mut columns {
                            col.format_code = FormatCode::Binary;
                        }
                        if is_new
                            && let Some(evicted) = self.stmt_cache.insert(
                                sql,
                                stmt_name.to_string(),
                                0,
                                Some(columns.clone()),
                            )
                        {
                            self.close_statement_on_server(&evicted.name);
                        }
                        columns_rc = Rc::new(columns);
                    }
                    BackendTag::NoData if is_new => {
                        if let Some(evicted) =
                            self.stmt_cache.insert(sql, stmt_name.to_string(), 0, None)
                        {
                            self.close_statement_on_server(&evicted.name);
                        }
                    }
                    BackendTag::NoData => {}
                    BackendTag::BindComplete => {}
                    BackendTag::DataRow => {
                        let raw_values = codec::parse_data_row(body);
                        rows.push(Row::new(Rc::clone(&columns_rc), raw_values));
                    }
                    BackendTag::CommandComplete => {
                        let (tag, rows_affected) = extract_command_complete(body);
                        self.last_command_tag = tag;
                        self.last_affected_rows = rows_affected;
                    }
                    BackendTag::ReadyForQuery => {
                        self.tx_status = TransactionStatus::from(body[0]);
                        self.consume_read(msg_len);
                        return Ok(rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.parse_error_with_context(body, sql);
                        self.consume_read(msg_len);
                        self.drain_to_ready()?;
                        return Err(err);
                    }
                    BackendTag::NotificationResponse => {
                        let notification = Self::parse_notification(body);
                        self.notifications.push_back(notification);
                    }
                    BackendTag::NoticeResponse => {
                        self.dispatch_notice(body);
                    }
                    _ => {}
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn drain_to_ready(&mut self) -> PgResult<()> {
        loop {
            // Only read from the socket when the buffer has no complete message;
            // data may already be buffered from an earlier fill_read_buf call.
            if codec::message_complete(&self.read_buf[..self.read_pos])?.is_none() {
                self.fill_read_buf(None)?;
            }
            while let Some(msg_len) = codec::message_complete(&self.read_buf[..self.read_pos])? {
                let header = codec::decode_header(&self.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                if header.tag == BackendTag::ReadyForQuery {
                    let body = &self.read_buf[5..msg_len];
                    self.tx_status = TransactionStatus::from(body[0]);
                    self.consume_read(msg_len);
                    return Ok(());
                }
                self.consume_read(msg_len);
            }
        }
    }

    fn parse_error(&self, body: &[u8]) -> PgError {
        let fields = codec::parse_error_fields(body);
        PgError::from_fields(&fields)
    }

    /// Parse an error and attach query context for better debugging.
    fn parse_error_with_context(&self, body: &[u8], query: &str) -> PgError {
        let fields = codec::parse_error_fields(body);
        let mut err = PgError::from_fields(&fields);
        if let PgError::Server(ref mut server_err) = err
            && server_err.internal_query.is_none()
        {
            server_err.internal_query = Some(query.to_string());
        }
        err
    }

    /// Dispatch a NoticeResponse to the registered handler.
    fn dispatch_notice(&self, body: &[u8]) {
        if let Some(ref handler) = self.notice_handler {
            let fields = codec::parse_error_fields(body);
            let mut severity = "";
            let mut code = "";
            let mut message = "";
            for (field_type, value) in &fields {
                match field_type {
                    b'S' => severity = value,
                    b'C' => code = value,
                    b'M' => message = value,
                    _ => {}
                }
            }
            handler(severity, code, message);
        }
    }

    /// Send a Close('S') message to deallocate a server-side prepared statement.
    /// This is fire-and-forget — we don't wait for CloseComplete.
    fn close_statement_on_server(&mut self, name: &str) {
        self.ensure_write_capacity(7 + name.len());
        let n = codec::encode_close(&mut self.write_buf, CloseTarget::Statement, name);
        let _ = self.flush_write_buf(n);
    }

    // Parse a CommandComplete tag to extract affected row count.
    // Tags look like: "INSERT 0 5", "UPDATE 3", "DELETE 1", "SELECT 10", etc.
    // parse_command_complete is now a free function: extract_command_complete()

    /// Parse a NotificationResponse message body.
    fn parse_notification(body: &[u8]) -> Notification {
        let process_id = codec::read_i32(body, 0);
        let (channel, consumed) = codec::read_cstring(body, 4);
        let (payload, _) = codec::read_cstring(body, 4 + consumed);
        Notification {
            process_id,
            channel: channel.to_string(),
            payload: payload.to_string(),
        }
    }
}

/// Extract the command tag and affected row count from a CommandComplete body.
/// This is a free function (not a method) to avoid borrow conflicts when
/// `body` is a slice of the connection's read buffer.
fn extract_command_complete(body: &[u8]) -> (String, u64) {
    let (tag, _) = codec::read_cstring(body, 0);
    let tag_str = tag.to_string();
    let affected_rows = tag
        .rsplit(' ')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    (tag_str, affected_rows)
}

impl Drop for PgConnection {
    fn drop(&mut self) {
        // Switch to blocking mode so the Terminate message is reliably sent.
        // On a non-blocking socket write_all may fail with WouldBlock,
        // silently leaving the server-side session open.
        if self.nonblocking {
            let _ = self.stream.set_nonblocking(false);
        }
        let n = codec::encode_terminate(&mut self.write_buf);
        let _ = self.stream.write_all(&self.write_buf[..n]);
    }
}

// ─── CancelToken ──────────────────────────────────────────────

/// Cancel token for interrupting a running backend query.
///
/// Obtained via [`PgConnection::cancel_token`].  The token is cheap to
/// clone and can be stored or sent to another worker.  Calling
/// [`CancelToken::send`] opens a new, short-lived connection to the same
/// PostgreSQL server and sends a `CancelRequest` message, which causes the
/// backend to interrupt the currently executing query and return a server
/// error with SQLSTATE `57014` (`query_canceled`) to the original
/// connection.
///
/// # Notes
/// - A successful `send()` does **not** guarantee the query was cancelled —
///   it may have already completed.
/// - The original connection stays open and valid after cancellation.
#[derive(Debug, Clone)]
pub struct CancelToken {
    host: String,
    port: u16,
    socket_dir: Option<String>,
    /// Backend process ID (from `BackendKeyData`).
    process_id: i32,
    /// Backend secret key (from `BackendKeyData`).
    secret_key: i32,
}

impl CancelToken {
    /// Send a cancel request to the PostgreSQL backend.
    ///
    /// Opens a fresh TCP (or Unix domain socket) connection to the server,
    /// sends the 16-byte `CancelRequest` message, and closes immediately.
    pub fn send(&self) -> PgResult<()> {
        use std::io::Write as _;

        // CancelRequest wire format (no message type byte, just raw bytes):
        //   Int32(16)         — total message length including self
        //   Int32(80877102)   — cancel request code (1234 << 16 | 5678)
        //   Int32(process_id) — backend PID
        //   Int32(secret_key) — backend secret key
        let mut msg = [0u8; 16];
        msg[0..4].copy_from_slice(&16i32.to_be_bytes());
        msg[4..8].copy_from_slice(&80877102i32.to_be_bytes());
        msg[8..12].copy_from_slice(&self.process_id.to_be_bytes());
        msg[12..16].copy_from_slice(&self.secret_key.to_be_bytes());

        if let Some(ref dir) = self.socket_dir {
            #[cfg(unix)]
            {
                use std::os::unix::net::UnixStream;
                let path = format!("{}/.s.PGSQL.{}", dir, self.port);
                UnixStream::connect(path)
                    .map_err(PgError::Io)?
                    .write_all(&msg)
                    .map_err(PgError::Io)
            }
            #[cfg(not(unix))]
            {
                let _ = dir;
                Err(PgError::Protocol(
                    "Unix domain sockets not supported on this platform".to_string(),
                ))
            }
        } else {
            let addr = format!("{}:{}", self.host, self.port);
            TcpStream::connect(&addr)
                .map_err(PgError::Io)?
                .write_all(&msg)
                .map_err(PgError::Io)
        }
    }
}

// ─── Cursor ───────────────────────────────────────────────────

/// A server-side streaming cursor backed by a named PostgreSQL portal.
///
/// Created by [`PgConnection::query_cursor`]. Call [`fetch`](Cursor::fetch)
/// repeatedly to retrieve rows in batches. The cursor is automatically
/// closed when dropped.
///
/// The connection is exclusively borrowed for the lifetime of the cursor —
/// no other queries can run while the cursor is open.
pub struct Cursor<'a> {
    conn: &'a mut PgConnection,
    portal_name: String,
    fetch_size: usize,
    /// True once the server has sent `CommandComplete` (no more rows).
    exhausted: bool,
    /// Column layout shared by all fetched rows.
    columns: Rc<Vec<codec::ColumnDesc>>,
}

impl<'a> Cursor<'a> {
    /// Fetch the next batch of up to `fetch_size` rows.
    ///
    /// Returns an empty `Vec` when no more rows are available.
    pub fn fetch(&mut self) -> PgResult<Vec<Row>> {
        if self.exhausted {
            return Ok(Vec::new());
        }

        // Encode Execute(portal, fetch_size) + Flush.
        {
            let estimated = 22 + self.portal_name.len();
            self.conn.ensure_write_capacity(estimated);
            let mut pos = 0;
            let n = codec::encode_execute(
                &mut self.conn.write_buf,
                &self.portal_name,
                self.fetch_size as i32,
            );
            pos += n;
            let n = codec::encode_flush(&mut self.conn.write_buf[pos..]);
            pos += n;
            self.conn.flush_write_buf(pos)?;
        }

        let mut rows = Vec::new();
        loop {
            if codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?.is_none() {
                self.conn.fill_read_buf(None)?;
            }
            while let Some(msg_len) =
                codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?
            {
                let tag = BackendTag::from(self.conn.read_buf[0]);
                match tag {
                    BackendTag::DataRow => {
                        let raw_values = {
                            let body = &self.conn.read_buf[5..msg_len];
                            codec::parse_data_row(body)
                        };
                        rows.push(Row::new(Rc::clone(&self.columns), raw_values));
                        self.conn.consume_read(msg_len);
                    }
                    BackendTag::PortalSuspended => {
                        // More rows available; caller may call fetch() again.
                        self.conn.consume_read(msg_len);
                        return Ok(rows);
                    }
                    BackendTag::CommandComplete => {
                        let (ctag, affected) = {
                            let body = &self.conn.read_buf[5..msg_len];
                            extract_command_complete(body)
                        };
                        self.conn.last_command_tag = ctag;
                        self.conn.last_affected_rows = affected;
                        self.conn.consume_read(msg_len);
                        self.exhausted = true;
                        // Send Sync to finalize the pipeline and receive ReadyForQuery.
                        let n = codec::encode_sync(&mut self.conn.write_buf);
                        self.conn.flush_write_buf(n)?;
                        self.conn.drain_to_ready()?;
                        return Ok(rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = {
                            let body = &self.conn.read_buf[5..msg_len];
                            self.conn.parse_error_with_context(body, &self.portal_name)
                        };
                        self.conn.consume_read(msg_len);
                        self.exhausted = true;
                        // Attempt to recover to a clean protocol state.
                        let n = codec::encode_sync(&mut self.conn.write_buf);
                        let _ = self.conn.flush_write_buf(n);
                        let _ = self.conn.drain_to_ready();
                        return Err(err);
                    }
                    BackendTag::NoticeResponse => {
                        let body = &self.conn.read_buf[5..msg_len];
                        self.conn.dispatch_notice(body);
                        self.conn.consume_read(msg_len);
                    }
                    _ => {
                        self.conn.consume_read(msg_len);
                    }
                }
            }
        }
    }
}

impl Drop for Cursor<'_> {
    fn drop(&mut self) {
        if self.exhausted {
            return; // Protocol is already in a clean state.
        }
        // The named portal is still open on the server.
        // Close it and sync to prevent resource leaks.
        let portal_name = self.portal_name.clone();
        let estimated = 12 + portal_name.len();
        self.conn.ensure_write_capacity(estimated);
        let mut pos = 0;
        let n = codec::encode_close(&mut self.conn.write_buf, CloseTarget::Portal, &portal_name);
        pos += n;
        let n = codec::encode_sync(&mut self.conn.write_buf[pos..]);
        pos += n;
        let _ = self.conn.flush_write_buf(pos);
        let _ = self.conn.drain_to_ready();
    }
}

// ─── Transaction ──────────────────────────────────────────────
/// A transaction guard. Ensures the transaction is committed or rolled back.
/// methods as `PgConnection`. On drop, if neither `commit` nor `rollback`
/// was called, automatically rolls back.
pub struct Transaction<'a> {
    conn: &'a mut PgConnection,
    finished: bool,
    /// If Some, this is a nested transaction backed by a SAVEPOINT.
    savepoint_name: Option<String>,
    /// Counter for generating unique savepoint names in nested calls.
    savepoint_counter: u32,
}

impl<'a> Transaction<'a> {
    /// Commit this transaction (or release savepoint if nested).
    pub fn commit(&mut self) -> PgResult<()> {
        if !self.finished {
            self.finished = true;
            if let Some(ref name) = self.savepoint_name {
                self.conn.release_savepoint(name)
            } else {
                self.conn.commit()
            }
        } else {
            Ok(())
        }
    }

    /// Rollback this transaction (or rollback to savepoint if nested).
    pub fn rollback(&mut self) -> PgResult<()> {
        if !self.finished {
            self.finished = true;
            if let Some(ref name) = self.savepoint_name {
                self.conn.rollback_to(name)
            } else {
                self.conn.rollback()
            }
        } else {
            Ok(())
        }
    }

    /// Execute a nested transaction using a SAVEPOINT.
    ///
    /// Creates a savepoint, calls the closure, and either releases
    /// (on success) or rolls back to the savepoint (on error/drop).
    ///
    /// Nesting is unlimited — each level creates a new savepoint.
    ///
    /// # Example
    /// ```ignore
    /// conn.transaction(|tx| {
    ///     tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Alice"])?;
    ///     tx.transaction(|nested| {
    ///         nested.execute("INSERT INTO logs (msg) VALUES ($1)", &[&"nested"])?;
    ///         Ok(())
    ///     })?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction<F, T>(&mut self, f: F) -> PgResult<T>
    where
        F: FnOnce(&mut Transaction<'_>) -> PgResult<T>,
    {
        self.savepoint_counter += 1;
        let sp_name = format!("chopin_sp_{}", self.savepoint_counter);
        self.conn.savepoint(&sp_name)?;
        let mut nested = Transaction {
            conn: self.conn,
            finished: false,
            savepoint_name: Some(sp_name),
            savepoint_counter: 0,
        };
        match f(&mut nested) {
            Ok(val) => {
                nested.commit()?;
                Ok(val)
            }
            Err(e) => {
                let _ = nested.rollback();
                Err(e)
            }
        }
    }

    /// Execute a simple query (no parameters).
    pub fn query_simple(&mut self, sql: &str) -> PgResult<Vec<Row>> {
        self.conn.query_simple(sql)
    }

    /// Execute a parameterized query.
    pub fn query(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Vec<Row>> {
        self.conn.query(sql, params)
    }

    /// Execute a query expecting exactly one row.
    pub fn query_one(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Row> {
        self.conn.query_one(sql, params)
    }

    /// Execute a query expecting zero or one row.
    pub fn query_opt(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<Option<Row>> {
        self.conn.query_opt(sql, params)
    }

    /// Execute a statement that returns no rows.
    pub fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> PgResult<u64> {
        self.conn.execute(sql, params)
    }

    /// Create a savepoint within this transaction.
    pub fn savepoint(&mut self, name: &str) -> PgResult<()> {
        self.conn.savepoint(name)
    }

    /// Rollback to a savepoint.
    pub fn rollback_to(&mut self, name: &str) -> PgResult<()> {
        self.conn.rollback_to(name)
    }

    /// Release a savepoint.
    pub fn release_savepoint(&mut self, name: &str) -> PgResult<()> {
        self.conn.release_savepoint(name)
    }

    /// Get the transaction status.
    pub fn status(&self) -> TransactionStatus {
        self.conn.transaction_status()
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if !self.finished {
            // Auto-rollback on drop (savepoint if nested, full rollback otherwise)
            if let Some(ref name) = self.savepoint_name {
                let _ = self.conn.rollback_to(name);
            } else {
                let _ = self.conn.rollback();
            }
        }
    }
}

// ─── COPY Writer ──────────────────────────────────────────────

/// COPY writer for streaming data into PostgreSQL via COPY FROM STDIN.
pub struct CopyWriter<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> CopyWriter<'a> {
    /// Write a chunk of COPY data.
    pub fn write_data(&mut self, data: &[u8]) -> PgResult<()> {
        self.conn.ensure_write_capacity(5 + data.len());
        let n = codec::encode_copy_data(&mut self.conn.write_buf, data);
        self.conn.flush_write_buf(n)
    }

    /// Abort the COPY operation with an error message.
    ///
    /// Sends a CopyFail message to the server. The server will respond
    /// with an ErrorResponse and then ReadyForQuery. The connection
    /// remains usable after this call.
    pub fn fail(self, reason: &str) -> PgResult<()> {
        self.conn.ensure_write_capacity(6 + reason.len());
        let n = codec::encode_copy_fail(&mut self.conn.write_buf, reason);
        self.conn.flush_write_buf(n)?;

        // Drain to ReadyForQuery (server sends ErrorResponse first)
        loop {
            self.conn.fill_read_buf(None)?;
            while let Some(msg_len) =
                codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?
            {
                let header = codec::decode_header(&self.conn.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                match header.tag {
                    BackendTag::ErrorResponse => {
                        // Expected — server acknowledges the CopyFail
                        self.conn.consume_read(msg_len);
                    }
                    BackendTag::ReadyForQuery => {
                        let body = &self.conn.read_buf[5..msg_len];
                        self.conn.tx_status = TransactionStatus::from(body[0]);
                        self.conn.consume_read(msg_len);
                        return Ok(());
                    }
                    _ => {
                        self.conn.consume_read(msg_len);
                    }
                }
            }
        }
    }

    /// Write a text row (tab-separated values with newline).
    pub fn write_row(&mut self, columns: &[&str]) -> PgResult<()> {
        let line = columns.join("\t") + "\n";
        self.write_data(line.as_bytes())
    }

    /// Finish the COPY operation successfully.
    pub fn finish(self) -> PgResult<u64> {
        let n = codec::encode_copy_done(&mut self.conn.write_buf);
        self.conn.flush_write_buf(n)?;

        // Drain to ReadyForQuery
        loop {
            self.conn.fill_read_buf(None)?;
            while let Some(msg_len) =
                codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?
            {
                let header = codec::decode_header(&self.conn.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.conn.read_buf[5..msg_len];
                match header.tag {
                    BackendTag::CommandComplete => {
                        let (tag, rows_affected) = extract_command_complete(body);
                        self.conn.last_command_tag = tag;
                        self.conn.last_affected_rows = rows_affected;
                    }
                    BackendTag::ReadyForQuery => {
                        self.conn.tx_status = TransactionStatus::from(body[0]);
                        self.conn.consume_read(msg_len);
                        return Ok(self.conn.last_affected_rows);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.conn.parse_error(body);
                        self.conn.consume_read(msg_len);
                        return Err(err);
                    }
                    _ => {}
                }
                self.conn.consume_read(msg_len);
            }
        }
    }
}

// ─── COPY Reader ──────────────────────────────────────────────

/// COPY reader for receiving data from PostgreSQL via COPY TO STDOUT.
pub struct CopyReader<'a> {
    conn: &'a mut PgConnection,
    done: bool,
}

impl<'a> CopyReader<'a> {
    /// Read the next chunk of COPY data.
    /// Returns None when the COPY operation is complete.
    pub fn read_data(&mut self) -> PgResult<Option<Vec<u8>>> {
        if self.done {
            return Ok(None);
        }

        loop {
            // Only refill the buffer when it doesn't already hold a complete
            // message.  If a previous fill read the entire COPY response in
            // one TCP segment, the CopyData / CopyDone / ReadyForQuery bytes
            // are already in `read_buf` and we must not block waiting for
            // more socket data before processing them.
            if codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?.is_none() {
                self.conn.fill_read_buf(None)?;
            }

            while let Some(msg_len) =
                codec::message_complete(&self.conn.read_buf[..self.conn.read_pos])?
            {
                let header = codec::decode_header(&self.conn.read_buf)
                    .ok_or_else(|| PgError::Protocol("Incomplete message header".to_string()))?;
                let body = &self.conn.read_buf[5..msg_len];

                match header.tag {
                    BackendTag::CopyData => {
                        let data = body.to_vec();
                        self.conn.consume_read(msg_len);
                        return Ok(Some(data));
                    }
                    BackendTag::CopyDone => {
                        self.conn.consume_read(msg_len);
                        // Continue to receive CommandComplete + ReadyForQuery
                    }
                    BackendTag::CommandComplete => {
                        let (tag, rows_affected) = extract_command_complete(body);
                        self.conn.last_command_tag = tag;
                        self.conn.last_affected_rows = rows_affected;
                        self.conn.consume_read(msg_len);
                    }
                    BackendTag::ReadyForQuery => {
                        self.conn.tx_status = TransactionStatus::from(body[0]);
                        self.conn.consume_read(msg_len);
                        self.done = true;
                        return Ok(None);
                    }
                    BackendTag::ErrorResponse => {
                        let err = self.conn.parse_error(body);
                        self.conn.consume_read(msg_len);
                        self.done = true;
                        return Err(err);
                    }
                    _ => {
                        self.conn.consume_read(msg_len);
                    }
                }
            }
        }
    }

    /// Read all remaining COPY data into a single Vec.
    pub fn read_all(&mut self) -> PgResult<Vec<u8>> {
        let mut result = Vec::new();
        while let Some(chunk) = self.read_data()? {
            result.extend_from_slice(&chunk);
        }
        Ok(result)
    }

    /// Check if the COPY operation is complete.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PgConfig::new ────────────────────────────────────────────────────────

    #[test]
    fn test_pgconfig_new_fields() {
        let cfg = PgConfig::new("db.example.com", 5432, "alice", "s3cret", "mydb");
        assert_eq!(cfg.host, "db.example.com");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.user, "alice");
        assert_eq!(cfg.password, "s3cret");
        assert_eq!(cfg.database, "mydb");
        assert!(cfg.socket_dir.is_none());
    }

    #[test]
    fn test_pgconfig_new_custom_port() {
        let cfg = PgConfig::new("host", 9999, "u", "p", "d");
        assert_eq!(cfg.port, 9999);
    }

    #[test]
    fn test_pgconfig_with_socket_dir_sets_field() {
        let cfg =
            PgConfig::new("localhost", 5432, "u", "p", "d").with_socket_dir("/var/run/postgresql");
        assert_eq!(cfg.socket_dir.as_deref(), Some("/var/run/postgresql"));
    }

    #[test]
    fn test_pgconfig_clone_preserves_all_fields() {
        let cfg = PgConfig::new("h", 1234, "u", "p", "db").with_socket_dir("/tmp");
        let cloned = cfg.clone();
        assert_eq!(cloned.host, "h");
        assert_eq!(cloned.port, 1234);
        assert_eq!(cloned.user, "u");
        assert_eq!(cloned.password, "p");
        assert_eq!(cloned.database, "db");
        assert_eq!(cloned.socket_dir, Some("/tmp".to_string()));
    }

    #[test]
    fn test_pgconfig_debug_contains_host() {
        let cfg = PgConfig::new("myhost", 5432, "u", "p", "d");
        let s = format!("{:?}", cfg);
        assert!(s.contains("myhost"), "Debug must include host: {}", s);
    }

    // ─── PgConfig::from_url — happy paths ────────────────────────────────────

    #[test]
    fn test_from_url_basic_postgres_scheme() {
        let cfg = PgConfig::from_url("postgres://bob:hunter2@dbhost:5432/appdb").unwrap();
        assert_eq!(cfg.host, "dbhost");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.user, "bob");
        assert_eq!(cfg.password, "hunter2");
        assert_eq!(cfg.database, "appdb");
        assert!(cfg.socket_dir.is_none());
    }

    #[test]
    fn test_from_url_postgresql_scheme() {
        let cfg = PgConfig::from_url("postgresql://u:p@host:5432/db").unwrap();
        assert_eq!(cfg.host, "host");
        assert_eq!(cfg.user, "u");
    }

    #[test]
    fn test_from_url_default_port() {
        // When no port is given, should default to 5432
        let cfg = PgConfig::from_url("postgres://u:p@myhost/mydb").unwrap();
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.host, "myhost");
    }

    #[test]
    fn test_from_url_no_password() {
        // user only, no colon → password is empty string
        let cfg = PgConfig::from_url("postgres://alice@host:5432/db").unwrap();
        assert_eq!(cfg.user, "alice");
        assert_eq!(cfg.password, "");
    }

    #[test]
    fn test_from_url_custom_port() {
        let cfg = PgConfig::from_url("postgres://u:p@host:9000/db").unwrap();
        assert_eq!(cfg.port, 9000);
    }

    #[test]
    fn test_from_url_unix_socket_query_param() {
        let cfg = PgConfig::from_url("postgres://u:p@/db?host=/var/run/postgresql").unwrap();
        assert_eq!(cfg.socket_dir.as_deref(), Some("/var/run/postgresql"));
        assert_eq!(cfg.database, "db");
    }

    #[test]
    fn test_from_url_unix_socket_percent_encoded() {
        let cfg = PgConfig::from_url("postgres://u:p@%2Fvar%2Frun%2Fpostgresql/db").unwrap();
        assert_eq!(cfg.socket_dir.as_deref(), Some("/var/run/postgresql"));
        assert_eq!(cfg.database, "db");
    }

    // ─── PgConfig::from_url — error paths ────────────────────────────────────

    #[test]
    fn test_from_url_invalid_scheme_errors() {
        let result = PgConfig::from_url("mysql://u:p@host/db");
        assert!(result.is_err(), "Non-postgres scheme must fail");
    }

    #[test]
    fn test_from_url_missing_at_symbol_errors() {
        let result = PgConfig::from_url("postgres://no-at-sign/db");
        assert!(result.is_err(), "URL without @ must fail");
    }

    #[test]
    fn test_from_url_missing_database_errors() {
        // No "/" after host — no database segment
        let result = PgConfig::from_url("postgres://u:p@host");
        assert!(result.is_err(), "URL without database must fail");
    }

    #[test]
    fn test_from_url_invalid_port_errors() {
        let result = PgConfig::from_url("postgres://u:p@host:notaport/db");
        assert!(result.is_err(), "Non-numeric port must fail");
    }

    #[test]
    fn test_from_url_empty_string_errors() {
        let result = PgConfig::from_url("");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_url_special_chars_in_password() {
        // Passwords with @ in them need proper URL encoding, but basic case works
        let cfg = PgConfig::from_url("postgres://user:p%40ss@host:5432/db");
        // Parser finds last @ — this might fail or partially parse; just verify no panic
        let _ = cfg; // result can be Ok or Err, but must not panic
    }

    // ─── Notification struct ──────────────────────────────────────────────────

    #[test]
    fn test_notification_fields() {
        let n = Notification {
            process_id: 12345,
            channel: "my_channel".to_string(),
            payload: "hello world".to_string(),
        };
        assert_eq!(n.process_id, 12345);
        assert_eq!(n.channel, "my_channel");
        assert_eq!(n.payload, "hello world");
    }

    #[test]
    fn test_notification_clone() {
        let n = Notification {
            process_id: 42,
            channel: "ch".to_string(),
            payload: "pay".to_string(),
        };
        let n2 = n.clone();
        assert_eq!(n2.process_id, n.process_id);
        assert_eq!(n2.channel, n.channel);
        assert_eq!(n2.payload, n.payload);
    }

    #[test]
    fn test_notification_debug() {
        let n = Notification {
            process_id: 1,
            channel: "c".to_string(),
            payload: "p".to_string(),
        };
        let s = format!("{:?}", n);
        assert!(
            s.contains("process_id"),
            "Debug must include process_id: {}",
            s
        );
    }
}
