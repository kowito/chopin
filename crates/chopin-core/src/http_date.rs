//! HTTP utilities for RFC 7231 compliant operations and common HTTP helpers.
//!
//! Provides:
//! - Status code utilities
//! - Header formatting helpers

/// HTTP status code categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCategory {
    Informational, // 1xx
    Success,       // 2xx
    Redirection,   // 3xx
    ClientError,   // 4xx
    ServerError,   // 5xx
}

/// Get the category of an HTTP status code
#[inline(always)]
pub fn status_category(code: u16) -> StatusCategory {
    match code / 100 {
        1 => StatusCategory::Informational,
        2 => StatusCategory::Success,
        3 => StatusCategory::Redirection,
        4 => StatusCategory::ClientError,
        5 => StatusCategory::ServerError,
        _ => StatusCategory::ClientError, // Default to 4xx for unknown codes
    }
}

/// Get the reason phrase for an HTTP status code
#[inline(always)]
pub fn status_reason(code: u16) -> &'static str {
    match code {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

/// Format Content-Length header value into a buffer.
///
/// Returns the bytes written to the buffer.
///
/// # Example
/// ```ignore
/// let mut buf = [0u8; 20];
/// let len = format_content_length(1234, &mut buf);
/// // buf[..len] contains "Content-Length: 1234\r\n"
/// ```
#[inline]
pub fn format_content_length(size: usize, out: &mut [u8]) -> usize {
    let prefix = b"Content-Length: ";
    let mut i = prefix.len();

    if i > out.len() {
        return 0;
    }

    out[..i].copy_from_slice(prefix);

    // Format the number
    let size_str = size.to_string();
    let size_bytes = size_str.as_bytes();

    if i + size_bytes.len() + 2 > out.len() {
        return 0;
    }

    out[i..i + size_bytes.len()].copy_from_slice(size_bytes);
    i += size_bytes.len();

    out[i] = b'\r';
    i += 1;
    out[i] = b'\n';
    i += 1;

    i
}

/// Check if a status code is cacheable.
///
/// Only 200, 203, 204, 206, 300, 301, 404, 405, 410, 414, and 501 are cacheable
/// without explicit Cache-Control directives.
#[inline(always)]
pub fn is_cacheable_status(code: u16) -> bool {
    matches!(
        code,
        200 | 203 | 204 | 206 | 300 | 301 | 404 | 405 | 410 | 414 | 501
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_category() {
        assert_eq!(status_category(200), StatusCategory::Success);
        assert_eq!(status_category(301), StatusCategory::Redirection);
        assert_eq!(status_category(404), StatusCategory::ClientError);
        assert_eq!(status_category(500), StatusCategory::ServerError);
        assert_eq!(status_category(100), StatusCategory::Informational);
    }

    #[test]
    fn test_status_reason() {
        assert_eq!(status_reason(200), "OK");
        assert_eq!(status_reason(404), "Not Found");
        assert_eq!(status_reason(500), "Internal Server Error");
        assert_eq!(status_reason(999), "Unknown");
    }

    #[test]
    fn test_is_cacheable_status() {
        assert!(is_cacheable_status(200));
        assert!(is_cacheable_status(301));
        assert!(is_cacheable_status(404));
        assert!(!is_cacheable_status(201));
        assert!(!is_cacheable_status(500));
    }

    #[test]
    fn test_format_content_length() {
        let mut buf = [0u8; 50];
        let len = format_content_length(1234, &mut buf);
        let result = std::str::from_utf8(&buf[..len]).unwrap();
        assert_eq!(result, "Content-Length: 1234\r\n");
    }
}
