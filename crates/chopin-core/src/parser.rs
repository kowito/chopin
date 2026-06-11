//! Zero-copy HTTP/1.1 request parser.
//!
//! Parses raw byte slices into [`Request`](crate::http::Request) structs with
//! no heap allocation.  Returns [`ParseError::Incomplete`] for partial data
//! (the event loop will accumulate more bytes and retry), or
//! [`ParseError::TooLarge`] when the request exceeds [`MAX_REQUEST_SIZE`].
//!
//! This is an internal module; application code does not call the parser directly.
// src/parser.rs
use crate::http::{MAX_HEADERS, Method, Request};
use memchr::memchr;

/// Hard limit on total request size (headers + body).  Requests exceeding this
/// are rejected with `ParseError::TooLarge` to prevent OOM from huge bodies.
/// This is the *default* used when no per-server override is configured.
pub const MAX_REQUEST_SIZE: usize = 4_194_304; // 4 MiB

#[derive(Debug)]
pub enum ParseError {
    Incomplete,
    InvalidFormat,
    TooLarge,
}

/// Parses an HTTP request out of the given buffer.
/// Returns the parsed Request and the total number of bytes consumed (length of headers + body).
///
/// `max_size` caps the total allowed request size (headers + body).  Pass
/// [`MAX_REQUEST_SIZE`] for the 4 MiB default or a custom value configured via
/// [`Server::with_max_request_size`].
#[inline(always)]
pub fn parse_request(
    buf_mut: &mut [u8],
    max_size: usize,
) -> Result<(Request<'_>, usize), ParseError> {
    let ptr = buf_mut.as_mut_ptr();
    let len = buf_mut.len();
    let buf = &*buf_mut;

    // Basic HTTP request line: METHOD PATH HTTP/1.x\r\n
    // Find first space for Method (SIMD-accelerated)
    let space1 = memchr(b' ', buf).ok_or(ParseError::Incomplete)?;
    let method = Method::from_bytes(&buf[..space1]);

    // Find second space for Path (SIMD-accelerated)
    let space2 = memchr(b' ', &buf[space1 + 1..])
        .map(|i| i + space1 + 1)
        .ok_or(ParseError::Incomplete)?;
    let path_bytes = &buf[space1 + 1..space2];

    // Validate path as UTF-8
    let full_path = std::str::from_utf8(path_bytes).map_err(|_| ParseError::InvalidFormat)?;

    let (path, query) = match full_path.find('?') {
        Some(idx) => (&full_path[..idx], Some(&full_path[idx + 1..])),
        None => (full_path, None),
    };

    // Find the end of the request line (SIMD-accelerated \r scan)
    let req_line_end = {
        let search_start = space2 + 1;
        let mut pos = search_start;
        loop {
            match memchr(b'\r', &buf[pos..]) {
                Some(offset) => {
                    let abs = pos + offset;
                    if abs + 1 < buf.len() && buf[abs + 1] == b'\n' {
                        break abs;
                    }
                    pos = abs + 1;
                }
                None => return Err(ParseError::Incomplete),
            }
        }
    };

    let mut headers = [("", ""); MAX_HEADERS];
    let mut header_count: u8 = 0;
    let mut cursor = req_line_end + 2;

    while cursor + 1 < buf.len() {
        if header_count as usize >= MAX_HEADERS {
            return Err(ParseError::TooLarge);
        }

        if buf[cursor] == b'\r' && buf[cursor + 1] == b'\n' {
            cursor += 2;
            break; // End of headers
        }

        // Find the colon (SIMD-accelerated)
        let colon_idx = match memchr(b':', &buf[cursor..]) {
            Some(offset) => {
                let abs = cursor + offset;
                // Make sure we didn't skip past a \r (malformed header)
                if let Some(cr_offset) = memchr(b'\r', &buf[cursor..abs]) {
                    let _ = cr_offset; // colon is after \r — no colon on this line
                    return Err(ParseError::InvalidFormat);
                }
                abs
            }
            None => return Err(ParseError::InvalidFormat),
        };

        let name =
            std::str::from_utf8(&buf[cursor..colon_idx]).map_err(|_| ParseError::InvalidFormat)?;

        // Find header line end (SIMD-accelerated \r scan)
        let line_end = {
            let search_start = colon_idx + 1;
            let mut pos = search_start;
            loop {
                match memchr(b'\r', &buf[pos..]) {
                    Some(offset) => {
                        let abs = pos + offset;
                        if abs + 1 < buf.len() && buf[abs + 1] == b'\n' {
                            break abs;
                        }
                        pos = abs + 1;
                    }
                    None => return Err(ParseError::Incomplete),
                }
            }
        };

        let mut val_start = colon_idx + 1;
        while val_start < line_end && buf[val_start] == b' ' {
            val_start += 1;
        }

        let val = std::str::from_utf8(&buf[val_start..line_end])
            .map_err(|_| ParseError::InvalidFormat)?;

        headers[header_count as usize] = (name, val);
        header_count += 1;
        cursor = line_end + 2;
    }

    let header_end = cursor;

    // Use raw pointer for remaining buffer to avoid creating a `&mut [u8]` that
    // overlaps with the `&str` header slices stored in `headers`.  Stacked Borrows
    // forbids creating a mutable reference while immutable references derived from
    // the same allocation are still live, even when the ranges don't overlap
    // logically.  Raw-pointer reads/writes are outside the borrow-checker model
    // and are sound here because:
    //   - Headers (`&str` slices) are read-only and confined to buf[..header_end].
    //   - All chunked-body writes target positions >= header_end.
    //   - We never create a `&mut` that aliases a live `&` reference.
    // SAFETY: header_end is <= len (derived from buf parsing above).
    let remaining_ptr = unsafe { ptr.add(header_end) };
    let remaining_len = len - header_end;

    let mut expected_len = 0;
    let mut is_chunked = false;

    for header in headers.iter().take(header_count as usize) {
        let (name, val) = *header;
        if name.eq_ignore_ascii_case("content-length") {
            expected_len = val.parse::<usize>().unwrap_or(0);
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            && val.eq_ignore_ascii_case("chunked")
        {
            is_chunked = true;
        }
    }

    // For Content-Length requests, we'll check size limits when the body is complete.
    // For chunked requests, size limits are enforced during chunk processing.
    // This prevents rejecting legitimate requests that haven't sent their body yet.

    let consumed;
    let final_body;

    if is_chunked {
        let mut read_pos = 0;
        let mut write_pos = 0;

        loop {
            // Scan for \r\n using raw pointer reads (no &mut slice).
            let crlf = {
                let mut found = None;
                let max = remaining_len.saturating_sub(1);
                for i in read_pos..max {
                    // SAFETY: i and i+1 are within [0, remaining_len), so
                    // remaining_ptr.add(i) / .add(i+1) are in bounds.
                    unsafe {
                        if *remaining_ptr.add(i) == b'\r' && *remaining_ptr.add(i + 1) == b'\n' {
                            found = Some(i);
                            break;
                        }
                    }
                }
                found
            }
            .ok_or(ParseError::Incomplete)?;

            // Build a temporary shared slice (not a `&mut`) for UTF-8 decoding.
            let hex_str = {
                // SAFETY: read_pos..crlf is within [0, remaining_len).  We create
                // a `*const u8` slice that doesn't alias any live `&mut`.
                let tmp = unsafe {
                    std::slice::from_raw_parts(
                        remaining_ptr.add(read_pos) as *const u8,
                        crlf - read_pos,
                    )
                };
                std::str::from_utf8(tmp).map_err(|_| ParseError::InvalidFormat)?
            };
            let chunk_len =
                usize::from_str_radix(hex_str.trim(), 16).map_err(|_| ParseError::InvalidFormat)?;

            // D.1: Enforce size limit on chunked bodies
            if write_pos + chunk_len > max_size.saturating_sub(header_end) {
                return Err(ParseError::TooLarge);
            }

            if chunk_len == 0 {
                read_pos = crlf + 2;
                // Find final \r\n (end of chunked body).
                if read_pos + 2 > remaining_len {
                    return Err(ParseError::Incomplete);
                }
                // SAFETY: read_pos and read_pos+1 are within bounds (checked above).
                unsafe {
                    if *remaining_ptr.add(read_pos) == b'\r'
                        && *remaining_ptr.add(read_pos + 1) == b'\n'
                    {
                        read_pos += 2;
                    }
                }
                break;
            }

            let data_start = crlf + 2;
            if data_start + chunk_len + 2 > remaining_len {
                return Err(ParseError::Incomplete);
            }

            // Copy chunk data from data_start to write_pos via raw pointer.
            // We use `copy` (memmove semantics) rather than `copy_nonoverlapping`
            // because as chunks are compacted, the write position can advance
            // into the range of a later chunk's source data.
            // SAFETY:
            //   - Source: remaining_ptr.add(data_start) … +chunk_len is within [0, remaining_len).
            //   - Dest:   remaining_ptr.add(write_pos) … +chunk_len is also within [0, remaining_len).
            //   - No aliasing with `&str` header refs (they're below header_end).
            unsafe {
                std::ptr::copy(
                    remaining_ptr.add(data_start),
                    remaining_ptr.add(write_pos),
                    chunk_len,
                );
            }
            write_pos += chunk_len;
            read_pos = data_start + chunk_len + 2; // Skip trailing \r\n
        }

        // Build a shared body slice from the compacted region.
        // SAFETY: write_pos bytes at remaining_ptr are valid (compacted above).
        final_body = unsafe { std::slice::from_raw_parts(remaining_ptr, write_pos) };
        consumed = header_end + read_pos;
    } else {
        if remaining_len < expected_len {
            return Err(ParseError::Incomplete);
        }
        // D.1: Check size limit only when we have the complete body
        if header_end + expected_len > max_size {
            return Err(ParseError::TooLarge);
        }
        // Build a shared body slice from the raw pointer.
        // SAFETY: expected_len bytes at remaining_ptr are valid (we checked
        // remaining_len >= expected_len above).
        final_body = unsafe { std::slice::from_raw_parts(remaining_ptr, expected_len) };
        consumed = header_end + expected_len;
    }

    Ok((
        Request {
            method,
            path,
            query,
            headers,
            header_count,
            body: final_body,
        },
        consumed,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Method;

    #[test]
    fn test_parse_basic_request() {
        let mut req = b"GET /some/path?foo=bar HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nBodyContent".to_vec();
        let (request, consumed) = parse_request(&mut req, MAX_REQUEST_SIZE).unwrap();

        assert_eq!(request.method, Method::Get);
        assert_eq!(request.path, "/some/path");
        assert_eq!(request.query, Some("foo=bar"));
        assert_eq!(request.header_count, 2);
        assert_eq!(request.body, b"BodyContent");
        assert_eq!(consumed, req.len());
    }

    #[test]
    fn test_parse_incomplete_request() {
        let mut req = b"GET /some/path?foo=bar HTT".to_vec();
        assert!(matches!(
            parse_request(&mut req, MAX_REQUEST_SIZE),
            Err(ParseError::Incomplete)
        ));
    }

    #[test]
    fn test_parse_chunked_request() {
        let mut req = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n".to_vec();
        let (request, _consumed) = parse_request(&mut req, MAX_REQUEST_SIZE).unwrap();
        assert_eq!(request.body, b"Wikipedia in\r\n\r\nchunks.");
    }

    #[test]
    fn test_parse_too_large_content_length() {
        // Content-Length exceeds MAX_REQUEST_SIZE but body is missing.
        // Parser should wait for body bytes first.
        let mut req = b"POST / HTTP/1.1\r\nContent-Length: 2000000\r\n\r\n".to_vec();
        assert!(matches!(
            parse_request(&mut req, MAX_REQUEST_SIZE),
            Err(ParseError::Incomplete)
        ));
    }

    #[test]
    fn test_parse_too_large_content_length_with_complete_body() {
        // Once full body bytes are present, oversized requests are rejected.
        let body = vec![b'a'; MAX_REQUEST_SIZE + 1];
        let mut req =
            format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        req.extend_from_slice(&body);

        assert!(matches!(
            parse_request(&mut req, MAX_REQUEST_SIZE),
            Err(ParseError::TooLarge)
        ));
    }

    #[test]
    fn test_parse_within_size_limit() {
        // Small body within limit → OK
        let mut req = b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello".to_vec();
        let (request, _consumed) = parse_request(&mut req, MAX_REQUEST_SIZE).unwrap();
        assert_eq!(request.body, b"hello");
    }
}
