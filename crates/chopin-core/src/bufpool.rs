// src/bufpool.rs
//
// Phase 1.2: Thread-local buffer pool.
//
// Eliminates repeated `malloc`/`free` cycles for short-lived `Vec<u8>` buffers
// used during JSON serialization, response body construction, and chunked
// encoding. Each worker thread keeps its own free-list — zero synchronization.
//
// Design
// ------
// - Pool holds up to `MAX_POOLED` buffers, each up to `MAX_BUF_CAPACITY` bytes.
// - Buffers are returned to the pool (via `BufGuard::drop`) only if they fit
//   the size limit; oversized buffers are deallocated normally.
// - `BufGuard` implements `Deref<Target = Vec<u8>>` so callers can use it
//   like a regular `Vec<u8>`.
//
// Usage
// -----
// ```rust
// let mut buf = bufpool::get();   // checkout a cleared Vec
// buf.extend_from_slice(b"hello");
// // buf returned to pool automatically when dropped
// ```

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

/// Maximum number of buffers retained in the per-thread pool.
const MAX_POOLED: usize = 8;

/// Buffers larger than this capacity are discarded on return (not pooled)
/// to avoid holding onto large one-off allocations indefinitely.
const MAX_BUF_CAPACITY: usize = 256 * 1024; // 256 KiB

thread_local! {
    static POOL: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

/// Check out a cleared `Vec<u8>` from the pool, or allocate a new one.
#[inline]
pub fn get() -> BufGuard {
    let inner = POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
    BufGuard { inner }
}

/// Check out a cleared `Vec<u8>` with a pre-reserved capacity hint.
/// Useful when the approximate response size is known (e.g. JSON serialization).
#[inline]
pub fn get_with_capacity(hint: usize) -> BufGuard {
    let mut inner = POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
    if inner.capacity() < hint {
        inner.reserve(hint - inner.capacity());
    }
    BufGuard { inner }
}

/// Return a `Vec<u8>` to the pool. Called automatically by `BufGuard::drop`.
#[inline]
fn return_buf(mut buf: Vec<u8>) {
    if buf.capacity() > MAX_BUF_CAPACITY {
        return; // Discard; don't hold onto giant allocations.
    }
    buf.clear();
    POOL.with(|p| {
        let mut pool = p.borrow_mut();
        if pool.len() < MAX_POOLED {
            pool.push(buf);
        }
        // else: pool full — just drop `buf` normally.
    });
}

/// RAII guard for a pooled buffer. Returns the buffer to the pool on drop.
pub struct BufGuard {
    inner: Vec<u8>,
}

impl Deref for BufGuard {
    type Target = Vec<u8>;
    #[inline]
    fn deref(&self) -> &Vec<u8> {
        &self.inner
    }
}

impl DerefMut for BufGuard {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.inner
    }
}

impl Drop for BufGuard {
    fn drop(&mut self) {
        // Take the Vec out of self so we can pass it by value.
        let buf = std::mem::take(&mut self.inner);
        return_buf(buf);
    }
}

impl BufGuard {
    /// Consume the guard, returning the inner `Vec<u8>` without returning it to
    /// the pool. Use when you need to hand ownership to another owner (e.g. a
    /// response body).
    #[inline]
    pub fn into_vec(mut self) -> Vec<u8> {
        std::mem::take(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_reuse() {
        let ptr1 = {
            let mut buf = get();
            buf.extend_from_slice(b"hello");
            buf.as_ptr() as usize
        };
        // After drop, buf should be back in pool
        let ptr2 = {
            let buf = get();
            buf.as_ptr() as usize
        };
        // Same allocation reused
        assert_eq!(ptr1, ptr2);
    }

    #[test]
    fn test_get_with_capacity() {
        let buf = get_with_capacity(4096);
        assert!(buf.capacity() >= 4096);
    }

    #[test]
    fn test_into_vec() {
        let mut buf = get();
        buf.extend_from_slice(b"world");
        let v = buf.into_vec();
        assert_eq!(v, b"world");
        // Pool should not have a pending return (into_vec consumed it)
    }

    #[test]
    fn test_oversized_not_pooled() {
        let cap_before = POOL.with(|p| p.borrow().len());
        {
            let mut buf = Vec::with_capacity(MAX_BUF_CAPACITY + 1);
            buf.push(0u8);
            return_buf(buf);
        }
        let cap_after = POOL.with(|p| p.borrow().len());
        // Pool size should not have grown (oversized buf discarded)
        assert_eq!(cap_before, cap_after);
    }
}
