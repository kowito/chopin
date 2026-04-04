// src/conn.rs

pub const DEFAULT_READ_BUF_SIZE: usize = 8192;
pub const DEFAULT_WRITE_BUF_SIZE: usize = 32768;

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
    pub read_len: u16,        // Valid bytes in read_buf
    pub write_pos: u16,       // Bytes already written (for partial write resume)
    pub write_len: u16,       // Total bytes to write in write_buf
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
            read_len: 0,
            write_pos: 0,
            write_len: 0,
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
}
