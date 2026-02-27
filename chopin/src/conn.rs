// src/conn.rs

pub const READ_BUF_SIZE: usize = 2036;
pub const WRITE_BUF_SIZE: usize = 2036;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnState {
    Free = 0,
    Accepted = 1,
    Reading = 2,
    Parsing = 3,
    Routing = 4,
    Handling = 5,
    Writing = 6,
    Closing = 7,
}

impl Default for ConnState {
    fn default() -> Self {
        ConnState::Free
    }
}

// 64-byte aligned struct avoiding false sharing and fitting cache lines
#[repr(C, align(64))]
pub struct Conn {
    pub fd: i32,                // File Descriptor or Free List Next Index
    pub state: ConnState,       // State machine enum
    pub parse_pos: u16,         // Parse checkpoint / total read valid bytes / total write len
    pub write_pos: u16,         // Bytes already written (for partial write resume)
    pub route_id: u16,          // Cached Route index for later lookup / State
    pub last_active: u32,       // Cached timestamp in seconds
    pub requests_served: u32,   // Number of HTTP requests served on this keep-alive connection
    
    pub read_buf: [u8; READ_BUF_SIZE],
    pub write_buf: [u8; WRITE_BUF_SIZE],
}

impl Conn {
    // A fresh unused connection slot
    pub fn empty() -> Self {
        Self {
            fd: -1,
            state: ConnState::Free,
            parse_pos: 0,
            write_pos: 0,
            route_id: 0,
            last_active: 0,
            requests_served: 0,
            read_buf: [0; READ_BUF_SIZE],
            write_buf: [0; WRITE_BUF_SIZE],
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
        
        let header_padding = 24_usize; // Expected based on Rust struct padding rules
        let total_size = header_padding + READ_BUF_SIZE + WRITE_BUF_SIZE;
        
        // Assert total size is a multiple of 64
        assert_eq!(total_size % 64, 0, "Conn total size not a multiple of 64!");
        assert_eq!(std::mem::size_of::<Conn>(), total_size);
    }
}
