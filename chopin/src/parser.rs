// src/parser.rs
use crate::http::{MAX_HEADERS, Method, Request};

#[derive(Debug)]
pub enum ParseError {
    Incomplete,
    InvalidFormat,
    TooLarge,
}

/// Parses an HTTP request out of the given buffer.
/// Returns the parsed Request and the total number of bytes consumed (length of headers + body).
pub fn parse_request(buf_mut: &mut [u8]) -> Result<(Request<'_>, usize), ParseError> {
    let ptr = buf_mut.as_mut_ptr();
    let len = buf_mut.len();
    let buf = &*buf_mut;

    // Basic HTTP request line: METHOD PATH HTTP/1.x\r\n
    // Find first space for Method
    let mut space1 = 0;
    while space1 < buf.len() && buf[space1] != b' ' {
        space1 += 1;
    }
    if space1 >= buf.len() {
        return Err(ParseError::Incomplete);
    }
    let method = Method::from_bytes(&buf[..space1]);

    // Find second space for Path
    let mut space2 = space1 + 1;
    while space2 < buf.len() && buf[space2] != b' ' {
        space2 += 1;
    }
    if space2 >= buf.len() {
        return Err(ParseError::Incomplete);
    }
    let path_bytes = &buf[space1 + 1..space2];

    // Validate path as UTF-8
    let full_path = std::str::from_utf8(path_bytes).map_err(|_| ParseError::InvalidFormat)?;

    let (path, query) = match full_path.find('?') {
        Some(idx) => (&full_path[..idx], Some(&full_path[idx + 1..])),
        None => (full_path, None),
    };

    // Find the end of the request line
    let mut req_line_end = space2 + 1;
    while req_line_end + 1 < buf.len()
        && !(buf[req_line_end] == b'\r' && buf[req_line_end + 1] == b'\n')
    {
        req_line_end += 1;
    }
    if req_line_end + 1 >= buf.len() {
        return Err(ParseError::Incomplete);
    }

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

        // Find the colon
        let mut colon_idx = cursor;
        while colon_idx < buf.len() && buf[colon_idx] != b':' && buf[colon_idx] != b'\r' {
            colon_idx += 1;
        }

        if colon_idx >= buf.len() || buf[colon_idx] == b'\r' {
            return Err(ParseError::InvalidFormat);
        }

        let name =
            std::str::from_utf8(&buf[cursor..colon_idx]).map_err(|_| ParseError::InvalidFormat)?;

        // Find header line end
        let mut line_end = colon_idx + 1;
        while line_end + 1 < buf.len() && !(buf[line_end] == b'\r' && buf[line_end + 1] == b'\n') {
            line_end += 1;
        }

        if line_end + 1 >= buf.len() {
            return Err(ParseError::Incomplete);
        }

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
}
