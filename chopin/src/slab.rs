// src/slab.rs
use crate::conn::{Conn, ConnState};

pub struct ConnectionSlab {
    entries: Box<[Conn]>,
    head_free: i32,
    active_count: usize,
}

impl ConnectionSlab {
    /// Allocate the huge array of Conns strictly once upon worker startup.
    pub fn new(capacity: usize) -> Self {
        // Initialize connections dynamically but avoid re-allocations
        let mut entries = Vec::with_capacity(capacity);
        for i in 0..capacity {
            let mut conn = Conn::empty();
            // The fd field works as the `next` index pointer.
            // The last entry points to -1 (null)
            conn.fd = if i == capacity - 1 { -1 } else { (i + 1) as i32 };
            entries.push(conn);
        }

        Self {
            entries: entries.into_boxed_slice(),
            head_free: 0,
            active_count: 0,
        }
    }

    /// O(1) allocation: returns an index to the free connection.
    /// Returns None if out of capacity.
    #[inline(always)]
    pub fn allocate(&mut self, new_fd: i32) -> Option<usize> {
        if self.head_free == -1 {
            return None; // Out of connections
        }

        let idx = self.head_free as usize;
        let conn = &mut self.entries[idx];
        
        // Grab the next free offset
        self.head_free = conn.fd; 
        
        // Setup connection specifically
        conn.fd = new_fd;
        conn.state = ConnState::Accepted;
        conn.parse_pos = 0;
        conn.route_id = 0;
        // Notice we do NOT clear read_buf/write_buf. 
        // We defer to state parsing tracking to never leak state, saving memset cycles.

        self.active_count += 1;
        Some(idx)
    }

    /// O(1) deallocation: returns connection back to the free list.
    #[inline(always)]
    pub fn free(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }

        let conn = &mut self.entries[index];
        if conn.state == ConnState::Free {
            return; // Double free prevention
        }

        // Point this free entry at the old head
        conn.fd = self.head_free;
        conn.state = ConnState::Free;

        // Make this entry the new head
        self.head_free = index as i32;
        self.active_count -= 1;
    }

    /// Access connection mutably by index
    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Conn> {
        self.entries.get_mut(index)
    }
    
    /// Access connection by index
    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&Conn> {
        self.entries.get(index)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.active_count
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.entries.len()
    }
}

// Quick benchmarks natively integrated without bench harness overhead
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slab_operations() {
        let mut slab = ConnectionSlab::new(10);
        
        assert_eq!(slab.active_count, 0);
        assert_eq!(slab.capacity(), 10);

        let idx1 = slab.allocate(100).unwrap();
        assert_eq!(idx1, 0);
        assert_eq!(slab.entries[idx1].fd, 100);
        assert_eq!(slab.entries[idx1].state, ConnState::Accepted);

        let idx2 = slab.allocate(101).unwrap();
        assert_eq!(idx2, 1);
        
        slab.free(idx1);
        assert_eq!(slab.active_count, 1);
        
        // Notice we reused index 0 since it was pushed to the head of the free list
        let idx3 = slab.allocate(102).unwrap();
        assert_eq!(idx3, 0); 
    }
}
