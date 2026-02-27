// src/parser.rs
use crate::http::{MAX_HEADERS, Method, Request};

#[derive(Debug)]
pub enum ParseError {
    Incomplete,
    InvalidFormat,
    TooLarge,
}

/// Parses an HTTP request out of the given buffer.
/// Returns the parsed Request and the total number of bytes consumed (length of headers).
pub fn parse_request(buf: &[u8]) -> Result<(Request<'_>, usize), ParseError> {
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
    let remaining = &buf[header_end..];

    Ok((
        Request {
            method,
            path,
            query,
            headers,
            header_count,
            body: remaining,
        },
        header_end,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Method;

    #[test]
    fn test_parse_basic_request() {
        let req = b"GET /some/path?foo=bar HTTP/1.1\r\nHost: localhost\r\nKeep-Alive: true\r\n\r\nBodyContent";
        let (request, consumed) = parse_request(req).unwrap();

        assert_eq!(request.method, Method::Get);
        assert_eq!(request.path, "/some/path");
        assert_eq!(request.query, Some("foo=bar"));
        assert_eq!(request.header_count, 2);
        assert_eq!(request.headers[0], ("Host", "localhost"));
        assert_eq!(request.headers[1], ("Keep-Alive", "true"));
        assert_eq!(request.body, b"BodyContent");
        assert_eq!(consumed, req.len() - 11);
    }

    #[test]
    fn test_parse_incomplete_request() {
        let req = b"GET /some/path?foo=bar HTT";
        assert!(matches!(parse_request(req), Err(ParseError::Incomplete)));
    }
}
