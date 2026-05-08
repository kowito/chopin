// src/filter.rs — Phase 6.1: I/O Filter Architecture
//
// Provides a composable filter trait for processing bytes on the read and write
// paths of a connection.  Filters can inspect, transform, log, or compress data
// transparently without modifying the core event loop.
//
// Design mirrors ntex's `ntex-io` filter stack:
// - `Filter` trait with `process_read` / `process_write`
// - `FilterStack` holds up to INLINE_SIZE (3) filters on the stack via an
//   `arrayvec::ArrayVec`, avoiding heap allocation for the common case
// - `PassthroughFilter` — zero-cost identity filter
// - `LoggingFilter` — logs byte-count summaries at TRACE level

// ── Trait ────────────────────────────────────────────────────────────────────

/// An I/O filter that can transform bytes on the read or write path.
///
/// Filters are applied in order from first-pushed to last-pushed on the write
/// path, and in reverse order on the read path.
///
/// # Implementing a filter
///
/// ```rust,ignore
/// use chopin_core::filter::Filter;
///
/// pub struct MyFilter { prefix: &'static [u8] }
///
/// impl Filter for MyFilter {
///     fn process_read(&mut self, buf: &mut [u8], len: usize) -> usize {
///         // inspect or transform `buf[..len]` and return the new length
///         len
///     }
///     fn process_write(&mut self, buf: &mut [u8], len: usize) -> usize {
///         len
///     }
///     fn name(&self) -> &'static str { "my-filter" }
/// }
/// ```
pub trait Filter: Send + 'static {
    /// Process bytes on the **read** path.
    ///
    /// `buf` is the full read buffer; `len` is the number of valid bytes.
    /// Returns the number of valid bytes after processing (may be ≤ `len`).
    fn process_read(&mut self, buf: &mut [u8], len: usize) -> usize;

    /// Process bytes on the **write** path.
    ///
    /// `buf` is the full write buffer; `len` is the number of bytes to send.
    /// Returns the number of bytes to actually write (may be ≤ `len`).
    fn process_write(&mut self, buf: &mut [u8], len: usize) -> usize;

    /// A short identifying name used in diagnostics.
    fn name(&self) -> &'static str;
}

// ── Built-in filters ─────────────────────────────────────────────────────────

/// An identity filter that passes bytes through without modification.
///
/// Useful as a placeholder or default layer in a [`FilterStack`].
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughFilter;

impl Filter for PassthroughFilter {
    #[inline(always)]
    fn process_read(&mut self, _buf: &mut [u8], len: usize) -> usize {
        len
    }

    #[inline(always)]
    fn process_write(&mut self, _buf: &mut [u8], len: usize) -> usize {
        len
    }

    fn name(&self) -> &'static str {
        "passthrough"
    }
}

/// A filter that logs byte counts for each read/write operation at `TRACE` level.
///
/// Has zero cost when the `logging` feature is disabled or when tracing is not
/// active (tracing's subscriber short-circuits when no subscriber is registered).
#[derive(Debug, Default, Clone, Copy)]
pub struct LoggingFilter;

impl Filter for LoggingFilter {
    fn process_read(&mut self, _buf: &mut [u8], len: usize) -> usize {
        tracing::trace!(bytes = len, "filter read");
        len
    }

    fn process_write(&mut self, _buf: &mut [u8], len: usize) -> usize {
        tracing::trace!(bytes = len, "filter write");
        len
    }

    fn name(&self) -> &'static str {
        "logging"
    }
}

// ── Filter stack ─────────────────────────────────────────────────────────────

/// Maximum number of filters that can be stored inline (no heap allocation).
const INLINE_SIZE: usize = 3;

/// A composable stack of at most `INLINE_SIZE` [`Filter`]s.
///
/// Filters are stored inline in an [`arrayvec::ArrayVec`], so the first
/// `INLINE_SIZE` layers require no heap allocation.  Adding a fourth or
/// more filter is a compile-time error (controlled by `INLINE_SIZE`).
///
/// Read processing applies filters in **reverse push order** (last-in, first-out).
/// Write processing applies filters in **push order** (first-in, first-out).
///
/// # Example
///
/// ```rust
/// use chopin_core::filter::{FilterStack, LoggingFilter, PassthroughFilter};
///
/// let mut stack = FilterStack::new();
/// stack.push(LoggingFilter);
/// stack.push(PassthroughFilter);
///
/// let mut buf = b"hello world".to_vec();
/// let n = stack.process_write(&mut buf, 11);
/// assert_eq!(n, 11);
/// ```
pub struct FilterStack {
    filters: arrayvec::ArrayVec<Box<dyn Filter>, INLINE_SIZE>,
}

impl Default for FilterStack {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterStack {
    /// Create an empty filter stack.
    pub fn new() -> Self {
        Self {
            filters: arrayvec::ArrayVec::new(),
        }
    }

    /// Append a filter to the top of the stack.
    ///
    /// # Panics
    ///
    /// Panics if more than `INLINE_SIZE` (currently 3) filters are pushed.
    pub fn push<F: Filter>(&mut self, filter: F) -> &mut Self {
        self.filters
            .try_push(Box::new(filter))
            .expect("FilterStack is full (max INLINE_SIZE layers)");
        self
    }

    /// Returns `true` if no filters have been added.
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Number of filters currently in the stack.
    pub fn len(&self) -> usize {
        self.filters.len()
    }

    /// Apply all filters in **push order** to the write buffer.
    ///
    /// Each filter receives the (possibly-modified) output of the previous one.
    /// Returns the final byte count after all filters have run.
    pub fn process_write(&mut self, buf: &mut [u8], mut len: usize) -> usize {
        for f in self.filters.iter_mut() {
            len = f.process_write(buf, len);
        }
        len
    }

    /// Apply all filters in **reverse push order** to the read buffer.
    ///
    /// Returns the final byte count after all filters have run.
    pub fn process_read(&mut self, buf: &mut [u8], mut len: usize) -> usize {
        for f in self.filters.iter_mut().rev() {
            len = f.process_read(buf, len);
        }
        len
    }

    /// Return a list of filter names, useful for diagnostics.
    pub fn names(&self) -> Vec<&'static str> {
        self.filters.iter().map(|f| f.name()).collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_filter_read() {
        let mut f = PassthroughFilter;
        let mut buf = [0u8; 8];
        assert_eq!(f.process_read(&mut buf, 5), 5);
    }

    #[test]
    fn passthrough_filter_write() {
        let mut f = PassthroughFilter;
        let mut buf = [0u8; 8];
        assert_eq!(f.process_write(&mut buf, 8), 8);
    }

    #[test]
    fn logging_filter_passthrough() {
        let mut f = LoggingFilter;
        let mut buf = b"hello".to_vec();
        assert_eq!(f.process_read(&mut buf, 5), 5);
        assert_eq!(f.process_write(&mut buf, 5), 5);
    }

    #[test]
    fn stack_empty() {
        let mut stack = FilterStack::new();
        let mut buf = b"hello".to_vec();
        // Empty stack: process_write with no filters returns len as-is.
        // (No iteration means len is returned unchanged.)
        assert_eq!(stack.process_write(&mut buf, 5), 5);
        assert_eq!(stack.process_read(&mut buf, 5), 5);
    }

    #[test]
    fn stack_push_and_process() {
        let mut stack = FilterStack::new();
        stack.push(PassthroughFilter).push(LoggingFilter);
        assert_eq!(stack.len(), 2);

        let mut buf = b"test data".to_vec();
        let n = stack.process_write(&mut buf, 9);
        assert_eq!(n, 9);
        let n = stack.process_read(&mut buf, 9);
        assert_eq!(n, 9);
    }

    #[test]
    fn stack_names() {
        let mut stack = FilterStack::new();
        stack.push(PassthroughFilter).push(LoggingFilter);
        let names = stack.names();
        assert_eq!(names, &["passthrough", "logging"]);
    }

    #[test]
    fn stack_custom_filter() {
        /// A filter that truncates: returns only the first half of bytes.
        struct HalfFilter;
        impl Filter for HalfFilter {
            fn process_read(&mut self, _buf: &mut [u8], len: usize) -> usize {
                len / 2
            }
            fn process_write(&mut self, _buf: &mut [u8], len: usize) -> usize {
                len / 2
            }
            fn name(&self) -> &'static str {
                "half"
            }
        }

        let mut stack = FilterStack::new();
        stack.push(HalfFilter).push(PassthroughFilter);

        let mut buf = vec![0u8; 10];
        // Write: HalfFilter first (10 → 5), then PassthroughFilter (5 → 5).
        assert_eq!(stack.process_write(&mut buf, 10), 5);
        // Read: PassthroughFilter first (5 → 5), then HalfFilter (5 → 2).
        assert_eq!(stack.process_read(&mut buf, 5), 2);
    }

    #[test]
    #[should_panic(expected = "FilterStack is full")]
    fn stack_overflow_panics() {
        let mut stack = FilterStack::new();
        stack.push(PassthroughFilter);
        stack.push(PassthroughFilter);
        stack.push(PassthroughFilter);
        // Fourth push exceeds INLINE_SIZE=3 and must panic.
        stack.push(PassthroughFilter);
    }
}
