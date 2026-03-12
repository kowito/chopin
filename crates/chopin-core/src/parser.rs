// src/parser.rs
use crate::http::{MAX_HEADERS, Method, Request};
use memchr::memchr;

/// Hard limit on total request size (headers + body).  Requests exceeding this
/// are rejected with `ParseError::TooLarge` to prevent OOM from huge bodies.
pub const MAX_REQUEST_SIZE: usize = 1_048_576; // 1 MiB

#[derive(Debug)]
pub enum ParseError {
    Incomplete,
    InvalidFormat,
    TooLarge,
}

/// Parses an HTTP request out of the given buffer.
/// Returns the parsed Request and the total number of bytes consumed (length of headers + body).
#[inline(always)]
pub fn parse_request(buf_mut: &mut [u8]) -> Result<(Request<'_>, usize), ParseError> {
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

    // SAFETY: We have parsed headers from buf[..header_end].
    // We now take a mutable slice of buf[header_end..] to decode the body (if chunked).
    // The immutable slices we took previously are strictly in buf[..header_end] and will not be mutated.
    let remaining =
        unsafe { std::slice::from_raw_parts_mut(ptr.add(header_end), len - header_end) };

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

    // D.1: Reject requests whose declared body would exceed the size limit.
    if header_end + expected_len > MAX_REQUEST_SIZE {
        return Err(ParseError::TooLarge);
    }

    let consumed;
    let final_body;

    if is_chunked {
        let mut read_pos = 0;
        let mut write_pos = 0;

        loop {
            let mut crlf = None;
            for i in read_pos..remaining.len().saturating_sub(1) {
                if remaining[i] == b'\r' && remaining[i + 1] == b'\n' {
                    crlf = Some(i);
                    break;
                }
            }
            let crlf = crlf.ok_or(ParseError::Incomplete)?;

            let hex_str = std::str::from_utf8(&remaining[read_pos..crlf])
                .map_err(|_| ParseError::InvalidFormat)?;
            let chunk_len =
                usize::from_str_radix(hex_str.trim(), 16).map_err(|_| ParseError::InvalidFormat)?;

            // D.1: Enforce size limit on chunked bodies
            if write_pos + chunk_len > MAX_REQUEST_SIZE - header_end {
                return Err(ParseError::TooLarge);
            }

            if chunk_len == 0 {
                read_pos = crlf + 2;
                // find final \r\n (end of chunked body)
                if read_pos + 2 > remaining.len() {
                    return Err(ParseError::Incomplete);
                }
                if remaining[read_pos] == b'\r' && remaining[read_pos + 1] == b'\n' {
                    read_pos += 2;
                }
                break;
            }

            let data_start = crlf + 2;
            if data_start + chunk_len + 2 > remaining.len() {
                return Err(ParseError::Incomplete);
            }

            remaining.copy_within(data_start..data_start + chunk_len, write_pos);
            write_pos += chunk_len;
            read_pos = data_start + chunk_len + 2; // Skip \r\n
        }

        // Safety: the body is now compacted at the beginning of `remaining`
        let body_ptr = remaining.as_ptr();
        final_body = unsafe { std::slice::from_raw_parts(body_ptr, write_pos) };
        consumed = header_end + read_pos;
    } else {
        if remaining.len() < expected_len {
            return Err(ParseError::Incomplete);
        }
        let body_ptr = remaining.as_ptr();
        final_body = unsafe { std::slice::from_raw_parts(body_ptr, expected_len) };
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
        let (request, consumed) = parse_request(&mut req).unwrap();

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
            parse_request(&mut req),
            Err(ParseError::Incomplete)
        ));
    }

    #[test]
    fn test_parse_chunked_request() {
        let mut req = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n".to_vec();
        let (request, _consumed) = parse_request(&mut req).unwrap();
        assert_eq!(request.body, b"Wikipedia in\r\n\r\nchunks.");
    }

    #[test]
    fn test_parse_too_large_content_length() {
        // Content-Length exceeds MAX_REQUEST_SIZE → TooLarge
        let mut req = b"POST / HTTP/1.1\r\nContent-Length: 2000000\r\n\r\n".to_vec();
        assert!(matches!(
            parse_request(&mut req),
            Err(ParseError::TooLarge)
        ));
    }

    #[test]
    fn test_parse_within_size_limit() {
        // Small body within limit → OK
        let mut req = b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello".to_vec();
        let (request, _consumed) = parse_request(&mut req).unwrap();
        assert_eq!(request.body, b"hello");
    }
}
