// src/headers.rs
//
// Zero-allocation response headers using fixed-size inline storage.
//
// Design: up to MAX_INLINE_HEADERS header values are stored in a
// stack-allocated `ArrayVec` (the "slab"). When the slab is full, additional
// headers spill into an optional `Vec` that is lazily created on first
// overflow. Header values ≤ MAX_INLINE_VALUE bytes are stored ‘inline’ in an
// `ArrayString`; longer values fall back to a heap `String`.

use arrayvec::{ArrayString, ArrayVec};
use std::fmt::Write;

// ── constants ────────────────────────────────────────────────────────────────

/// Maximum byte length of a header value stored inline (on the stack).
pub const MAX_INLINE_VALUE: usize = 64;

/// Number of headers kept in the inline slab before spilling to the heap.
pub const MAX_INLINE_HEADERS: usize = 8;

// ── HeaderValue ──────────────────────────────────────────────────────────────

/// An HTTP response header value that avoids heap allocation for short strings.
///
/// | Variant  | Storage   | Allocation |
/// |----------|-----------|------------|
/// | `Static` | pointer   | none       |
/// | `Inline` | 64-byte array on stack | none |
/// | `Heap`   | `String` on heap | one |
#[derive(Debug, Clone)]
pub enum HeaderValue {
    /// A compile-time constant string — zero cost to store.
    Static(&'static str),
    /// A short (≤ 64 byte) runtime string stored on the stack.
    Inline(ArrayString<MAX_INLINE_VALUE>),
    /// A long runtime string that did not fit inline.
    Heap(String),
}

impl HeaderValue {
    /// Return the value as a plain `&str`.
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        match self {
            HeaderValue::Static(s) => s,
            HeaderValue::Inline(s) => s.as_str(),
            HeaderValue::Heap(s) => s.as_str(),
        }
    }

    /// Build from an owned `String` – avoids the extra allocation when short.
    #[inline]
    fn from_owned(s: String) -> Self {
        if s.len() <= MAX_INLINE_VALUE {
            HeaderValue::Inline(ArrayString::from(&s).unwrap())
        } else {
            HeaderValue::Heap(s)
        }
    }
}

// ── IntoHeaderValue trait ────────────────────────────────────────────────────

/// Convert a value into a [`HeaderValue`].
///
/// Implement this trait to use your own types with
/// [`Response::with_header`](crate::http::Response::with_header).
pub trait IntoHeaderValue {
    fn into_header_value(self) -> HeaderValue;
}

/// `&'static str` → zero-cost `Static` variant.
impl IntoHeaderValue for &'static str {
    #[inline(always)]
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::Static(self)
    }
}

/// Owned `String` → `Inline` when short, otherwise `Heap`.
impl IntoHeaderValue for String {
    #[inline]
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::from_owned(self)
    }
}

// Numeric types ---------------------------------------------------------------
// We write the decimal representation into an ArrayString via the standard
// `fmt::Write` trait so we avoid pulling in a separate dependency.

macro_rules! impl_into_header_value_int {
    ($($t:ty),*) => {
        $(impl IntoHeaderValue for $t {
            #[inline]
            fn into_header_value(self) -> HeaderValue {
                let mut buf = ArrayString::<MAX_INLINE_VALUE>::new();
                // Numbers are always shorter than 64 bytes, so write! is infallible.
                write!(buf, "{}", self).ok();
                HeaderValue::Inline(buf)
            }
        })*
    };
}

impl_into_header_value_int!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

// ── Header ───────────────────────────────────────────────────────────────────

/// A single HTTP response header name/value pair.
///
/// Header names are always `&'static str` because they are HTTP standard
/// field names (or custom `X-` fields) that are known at compile time.
#[derive(Debug, Clone)]
pub struct Header {
    pub name: &'static str,
    pub value: HeaderValue,
}

// ── Headers container ────────────────────────────────────────────────────────

/// A compact, allocation-free (in the common case) container for HTTP response
/// headers.
///
/// Up to [`MAX_INLINE_HEADERS`] (8) headers are stored entirely in a
/// stack-allocated `ArrayVec` (*slab*). When the slab is full, subsequent
/// headers flow into an optional heap `Vec` (*spill*) that is created on first
/// overflow. Both pools are iterated in insertion order.
///
/// Using a struct (rather than an enum) avoids the `large_enum_variant` lint
/// while keeping the same allocation-free fast path.
#[derive(Debug, Clone)]
pub struct Headers {
    /// Fast path: up to 8 headers stored on the stack.
    slab: ArrayVec<Header, MAX_INLINE_HEADERS>,
    /// Slow path: created lazily when the slab overflows.
    spill: Option<Vec<Header>>,
}

impl Default for Headers {
    fn default() -> Self {
        Headers::new()
    }
}

impl Headers {
    /// Create an empty header collection (no heap allocation).
    #[inline(always)]
    pub fn new() -> Self {
        Headers {
            slab: ArrayVec::new(),
            spill: None,
        }
    }

    /// Add a header by name and value.
    ///
    /// The value may be any type that implements [`IntoHeaderValue`]:
    /// `&'static str`, `String`, or any integer type.
    ///
    /// Headers 1–8 go into the inline slab (stack). Any additional headers
    /// are pushed into the lazily created spill `Vec`.
    #[inline]
    pub fn add(&mut self, name: &'static str, value: impl IntoHeaderValue) {
        let header = Header {
            name,
            value: value.into_header_value(),
        };
        if !self.slab.is_full() {
            // SAFETY: checked `!is_full()`, so push cannot panic.
            self.slab.push(header);
        } else {
            self.spill.get_or_insert_with(Vec::new).push(header);
        }
    }

    /// Add a header where both name and value are compile-time constants.
    /// This is the absolute zero-cost path: no memcopy, no heap allocation.
    #[inline(always)]
    pub fn add_static(&mut self, name: &'static str, value: &'static str) {
        self.add(name, value);
    }

    /// Iterate over all headers in insertion order (slab first, then spill).
    #[inline]
    pub fn iter(
        &self,
    ) -> std::iter::Chain<std::slice::Iter<'_, Header>, std::slice::Iter<'_, Header>> {
        let spill_slice: &[Header] = self.spill.as_deref().unwrap_or(&[]);
        self.slab.iter().chain(spill_slice.iter())
    }

    /// Returns `true` if no headers have been added.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.slab.is_empty()
    }

    /// Returns the total number of headers.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.slab.len() + self.spill.as_ref().map_or(0, |v| v.len())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_headers_no_spill() {
        let mut h = Headers::new();
        for i in 0..MAX_INLINE_HEADERS {
            h.add("X-Test", format!("value-{}", i));
        }
        assert!(h.spill.is_none(), "should not spill at capacity");
        assert_eq!(h.len(), MAX_INLINE_HEADERS);
    }

    #[test]
    fn test_spill_to_heap_on_overflow() {
        let mut h = Headers::new();
        for i in 0..=MAX_INLINE_HEADERS {
            // 9 headers → 1 over the inline limit → spill
            h.add("X-Test", format!("value-{}", i));
        }
        assert!(h.spill.is_some(), "9th header should create spill Vec");
        assert_eq!(h.len(), MAX_INLINE_HEADERS + 1);
    }

    #[test]
    fn test_static_str_value() {
        let mut h = Headers::new();
        h.add("Content-Type", "application/json");
        let hdr = h.iter().next().unwrap();
        assert_eq!(hdr.value.as_str(), "application/json");
        // The &'static str path should use the Static variant.
        assert!(matches!(hdr.value, HeaderValue::Static(_)));
    }

    #[test]
    fn test_short_string_inline() {
        let val = "gzip".to_string();
        let mut h = Headers::new();
        h.add("Content-Encoding", val);
        let hdr = h.iter().next().unwrap();
        assert!(matches!(hdr.value, HeaderValue::Inline(_)));
        assert_eq!(hdr.value.as_str(), "gzip");
    }

    #[test]
    fn test_long_string_heap() {
        let long = "x".repeat(MAX_INLINE_VALUE + 1);
        let v = HeaderValue::from_owned(long.clone());
        assert!(matches!(v, HeaderValue::Heap(_)));
        assert_eq!(v.as_str(), long);
    }

    #[test]
    fn test_integer_value_inline() {
        let mut h = Headers::new();
        h.add("Content-Length", 12345usize);
        let hdr = h.iter().next().unwrap();
        assert!(matches!(hdr.value, HeaderValue::Inline(_)));
        assert_eq!(hdr.value.as_str(), "12345");
    }

    #[test]
    fn test_iter_order_preserved() {
        let mut h = Headers::new();
        h.add("X-A", "1");
        h.add("X-B", "2");
        h.add("X-C", "3");
        let names: Vec<&str> = h.iter().map(|hdr| hdr.name).collect();
        assert_eq!(names, vec!["X-A", "X-B", "X-C"]);
    }

    #[test]
    fn test_heap_iter_order_preserved() {
        let mut h = Headers::new();
        for i in 0..=MAX_INLINE_HEADERS {
            let name: &'static str = ["A", "B", "C", "D", "E", "F", "G", "H", "I"][i];
            h.add(name, i as u32);
        }
        assert!(h.spill.is_some(), "9th header should have caused a spill");
        let vals: Vec<&str> = h.iter().map(|hdr| hdr.value.as_str()).collect::<Vec<_>>();
        assert_eq!(vals, vec!["0", "1", "2", "3", "4", "5", "6", "7", "8"]);
    }
}
