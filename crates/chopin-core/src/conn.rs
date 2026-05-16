// src/conn.rs

pub const DEFAULT_READ_BUF_SIZE: usize = 32768; // Phase 1: 32 KiB read buffer (was 8 KiB)
pub const DEFAULT_WRITE_BUF_SIZE: usize = 32768;

/// Maximum size the adaptive read buffer growth will reach.
pub const MAX_READ_BUF_SIZE: usize = u16::MAX as usize; // 65535 bytes ≈ 64 KiB
/// Maximum size the write buffer is allowed to grow to.
/// 16 MiB gives ample headroom for large JSON API responses; responses larger
/// than this threshold will still be served via the zero-copy writev path
/// (Body::Bytes / Body::Static) which bypasses the write buffer entirely.
pub const MAX_WRITE_BUF_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Grow threshold: grow when buffer is ≥ 75% utilised.
const GROW_THRESH_NUM: usize = 3;
const GROW_THRESH_DEN: usize = 4;

/// Shrink threshold: shrink when buffer is > 2× default and utilisation is 0.
const SHRINK_FACTOR: usize = 2;

/// Connection flags (bit field)
pub const CONN_KEEP_ALIVE: u8 = 1;
pub const CONN_EPOLLOUT: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ConnState {
    #[default]
    Free = 0,
    Accepted = 1,
    Reading = 2,
    Parsing = 3,
    Routing = 4,
    Handling = 5,
    Writing = 6,
    Closing = 7,
}

// 64-byte aligned struct avoiding false sharing and fitting cache lines
#[repr(C, align(64))]
pub struct Conn {
    pub fd: i32,              // File Descriptor or Free List Next Index
    pub state: ConnState,     // State machine enum
    pub flags: u8,            // Bit 0: keep-alive (was padding)
    /// IPv6-mapped peer address captured at accept time via `getpeername(2)`.
    /// IPv4 addresses are stored as IPv4-mapped IPv6 (`::ffff:a.b.c.d`).
    /// Falls back to `[0u8; 16]` on UNIX sockets or errors.
    pub peer_addr: [u8; 16],
    pub read_len: u16,        // Valid bytes in read_buf
    pub write_pos: u32,       // Bytes already written (for partial write resume)
    pub write_len: u32,       // Total bytes to write in write_buf
    pub last_active: u32,     // Cached timestamp in seconds
    pub requests_served: u32, // Number of HTTP requests served on this keep-alive connection

    // Zero-copy sendfile state (set when serving Body::File)
    pub sendfile_fd: i32,     // File descriptor to sendfile from (-1 = inactive)
    pub sendfile_offset: u64, // Current offset in the file
    pub sendfile_remaining: u64, // Bytes still to transfer

    // Zero-copy body tracking (writev path — set for Body::Static/Bytes when wstart == 0)
    pub body_ptr: usize, // raw ptr to body bytes (0 = no body pending)
    pub body_total: u32, // total body length in bytes
    pub body_sent: u32,  // bytes already flushed
    pub body_owned: Option<Box<[u8]>>, // owns Body::Bytes allocation; None for Static/empty

    // io_uring: tracks which operation is currently in-flight for this connection.
    // Prevents double-submission (e.g. submitting OP_READ while previous OP_READ pending).
    // 0 = no pending op.
    #[cfg(feature = "io-uring")]
    pub pending_op: u8,

    /// Number of requests currently being processed on this connection (Phase 3.1).
    /// Incremented when a request is dispatched to a handler, decremented when the
    /// response is fully written. Allows future pipelining/concurrent dispatch logic
    /// to gate reads based on in-flight back-pressure.
    pub inflight: u8,

    /// Per-connection TLS session. `None` for plain-text connections.
    /// Populated immediately after `accept()` when the server is configured with TLS.
    #[cfg(feature = "tls")]
    pub tls_session: Option<Box<crate::tls::TlsSession>>,

    pub read_buf: Box<[u8]>,
    pub write_buf: Box<[u8]>,
}

impl Conn {
    // A fresh unused connection slot using default buffer sizes.
    pub fn empty() -> Self {
        Self::with_buf_sizes(DEFAULT_READ_BUF_SIZE, DEFAULT_WRITE_BUF_SIZE)
    }

    /// Create a connection slot with explicit buffer sizes (set at slab-init time).
    pub fn with_buf_sizes(read_size: usize, write_size: usize) -> Self {
        Self {
            fd: -1,
            state: ConnState::Free,
            flags: 0,
            peer_addr: [0u8; 16],
            read_len: 0,
            write_pos: 0u32,
            write_len: 0u32,
            last_active: 0,
            requests_served: 0,
            sendfile_fd: -1,
            sendfile_offset: 0,
            sendfile_remaining: 0,
            body_ptr: 0,
            body_total: 0,
            body_sent: 0,
            body_owned: None,
            #[cfg(feature = "io-uring")]
            pending_op: 0,
            inflight: 0,
            #[cfg(feature = "tls")]
            tls_session: None,
            read_buf: vec![0u8; read_size].into_boxed_slice(),
            write_buf: vec![0u8; write_size].into_boxed_slice(),
        }
    }

    /// Close and reset any in-progress sendfile transfer.
    #[inline]
    pub fn close_sendfile(&mut self) {
        if self.sendfile_fd >= 0 {
            unsafe {
                libc::close(self.sendfile_fd);
            }
            self.sendfile_fd = -1;
            self.sendfile_offset = 0;
            self.sendfile_remaining = 0;
        }
    }

    /// Clear any pending zero-copy body state (writev path).
    #[inline]
    pub fn body_clear(&mut self) {
        self.body_ptr = 0;
        self.body_total = 0;
        self.body_sent = 0;
        self.body_owned = None;
    }

    /// Drop the TLS session associated with this connection (plain-text no-op).
    #[inline]
    pub fn tls_clear(&mut self) {
        #[cfg(feature = "tls")]
        {
            self.tls_session = None;
        }
    }

    // ── Phase 1.3: Adaptive buffer watermarks ────────────────────────────────

    /// Grow `read_buf` by 2× (up to [`MAX_READ_BUF_SIZE`]) if the buffer is
    /// above the high-watermark (≥ 75% full).  Existing data is preserved.
    /// No-op when already at maximum size or below the watermark.
    #[inline]
    pub fn maybe_grow_read_buf(&mut self) {
        let cap = self.read_buf.len();
        let used = self.read_len as usize;
        if used < cap * GROW_THRESH_NUM / GROW_THRESH_DEN || cap >= MAX_READ_BUF_SIZE {
            return;
        }
        let new_cap = (cap * 2).min(MAX_READ_BUF_SIZE);
        let mut new_buf = vec![0u8; new_cap].into_boxed_slice();
        new_buf[..used].copy_from_slice(&self.read_buf[..used]);
        self.read_buf = new_buf;
    }

    /// Grow `write_buf` so it is at least `needed` bytes.  Returns `true` when
    /// the buffer is now large enough.  Returns `false` if `needed` exceeds
    /// [`MAX_WRITE_BUF_SIZE`] (caller should fall back to a smaller response).
    /// Existing write data (bytes `0..write_len`) is preserved.
    #[inline]
    pub fn try_grow_write_buf(&mut self, needed: usize) -> bool {
        if needed <= self.write_buf.len() {
            return true;
        }
        if needed > MAX_WRITE_BUF_SIZE {
            return false;
        }
        let new_cap = needed
            .next_power_of_two()
            .max(self.write_buf.len() * 2)
            .min(MAX_WRITE_BUF_SIZE);
        let existing = self.write_len as usize;
        let mut new_buf = vec![0u8; new_cap].into_boxed_slice();
        if existing > 0 {
            new_buf[..existing].copy_from_slice(&self.write_buf[..existing]);
        }
        self.write_buf = new_buf;
        true
    }

    /// Overwrite the current write buffer position/length accounting directly.
    /// For use in tests only.
    #[cfg(test)]
    pub fn set_write_cursor(&mut self, pos: u32, len: u32) {
        self.write_pos = pos;
        self.write_len = len;
    }

    /// Shrink oversized buffers back toward the default when the connection is
    /// idle (both buffers empty).  Reclaims heap memory after serving a large
    /// request or response that triggered a grow.
    #[inline]
    pub fn maybe_shrink_bufs(&mut self) {
        if self.read_len == 0 && self.read_buf.len() > DEFAULT_READ_BUF_SIZE * SHRINK_FACTOR {
            self.read_buf = vec![0u8; DEFAULT_READ_BUF_SIZE].into_boxed_slice();
        }
        if self.write_len == 0 && self.write_buf.len() > DEFAULT_WRITE_BUF_SIZE * SHRINK_FACTOR {
            self.write_buf = vec![0u8; DEFAULT_WRITE_BUF_SIZE].into_boxed_slice();
        }
    }
}

impl Default for Conn {
    fn default() -> Self {
        Self::empty()
    }
}

// Ensure tests verify our struct sizing statically
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_conn_alignment() {
        assert_eq!(std::mem::align_of::<Conn>(), 64);
        // Buffer data is now heap-allocated (Box<[u8]>); the struct holds fat pointers.
        // We only verify alignment and that size is a multiple of 64.
        let total_size = std::mem::size_of::<Conn>();
        assert_eq!(total_size % 64, 0, "Conn struct size not a multiple of 64!");
    }

    // ── Phase 1.3: watermark tests ──────────────────────────────────────────

    #[test]
    fn test_maybe_grow_read_buf_triggers_at_75_percent() {
        let mut conn = Conn::with_buf_sizes(64, 64);
        // Fill to exactly 75% — should NOT grow yet (threshold is >=)
        conn.read_len = 47; // 47/64 ≈ 73.4% — below threshold
        conn.maybe_grow_read_buf();
        assert_eq!(conn.read_buf.len(), 64);

        // 48/64 = 75% — should grow
        conn.read_len = 48;
        conn.maybe_grow_read_buf();
        assert_eq!(conn.read_buf.len(), 128);
        // Existing data still accessible
        assert_eq!(conn.read_len, 48);
    }

    #[test]
    fn test_maybe_grow_read_buf_capped_at_max() {
        let mut conn = Conn::with_buf_sizes(MAX_READ_BUF_SIZE, 64);
        conn.read_len = MAX_READ_BUF_SIZE as u16;
        conn.maybe_grow_read_buf();
        assert_eq!(conn.read_buf.len(), MAX_READ_BUF_SIZE); // No growth past max
    }

    #[test]
    fn test_try_grow_write_buf_basic() {
        let mut conn = Conn::with_buf_sizes(64, 64);
        // Already large enough
        assert!(conn.try_grow_write_buf(32));
        assert_eq!(conn.write_buf.len(), 64);

        // Needs to grow
        assert!(conn.try_grow_write_buf(100));
        assert!(conn.write_buf.len() >= 100);
    }

    #[test]
    fn test_try_grow_write_buf_preserves_data() {
        let mut conn = Conn::with_buf_sizes(64, 64);
        conn.write_buf[0] = 0xAB;
        conn.write_buf[1] = 0xCD;
        conn.write_len = 2;
        conn.try_grow_write_buf(200);
        assert_eq!(conn.write_buf[0], 0xAB);
        assert_eq!(conn.write_buf[1], 0xCD);
    }

    #[test]
    fn test_try_grow_write_buf_exceeds_max() {
        let mut conn = Conn::with_buf_sizes(64, 64);
        // Request that exceeds MAX_WRITE_BUF_SIZE (16 MiB) — must return false
        assert!(!conn.try_grow_write_buf(MAX_WRITE_BUF_SIZE + 1));
        assert_eq!(conn.write_buf.len(), 64); // Unchanged
    }

    #[test]
    fn test_try_grow_write_buf_large_response() {
        // Ensure buffers can grow well past the old 64 KiB u16 limit
        let mut conn = Conn::with_buf_sizes(64, 64);
        let target = 4 * 1024 * 1024; // 4 MiB — previously impossible
        assert!(conn.try_grow_write_buf(target));
        assert!(conn.write_buf.len() >= target);
    }

    #[test]
    fn test_maybe_shrink_bufs() {
        // Buffers at > 2× default → empty → shrink
        let mut conn = Conn::with_buf_sizes(DEFAULT_READ_BUF_SIZE * 3, DEFAULT_WRITE_BUF_SIZE * 3);
        conn.read_len = 0;
        conn.write_len = 0;
        conn.maybe_shrink_bufs();
        assert_eq!(conn.read_buf.len(), DEFAULT_READ_BUF_SIZE);
        assert_eq!(conn.write_buf.len(), DEFAULT_WRITE_BUF_SIZE);
    }

    #[test]
    fn test_maybe_shrink_bufs_nonempty_noop() {
        let large = DEFAULT_READ_BUF_SIZE * 3;
        let mut conn = Conn::with_buf_sizes(large, large);
        conn.read_len = 1; // not empty — should NOT shrink
        conn.write_len = 0;
        conn.maybe_shrink_bufs();
        assert_eq!(conn.read_buf.len(), large);
    }
}
