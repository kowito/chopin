use crate::parser::ParseError;
use memchr::memchr;

#[derive(Debug)]
pub struct Part<'a> {
    pub name: Option<&'a str>,
    pub filename: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub body: &'a [u8],
}

pub struct Multipart<'a> {
    body: &'a [u8],
    boundary_marker: std::vec::Vec<u8>,
}

impl<'a> Multipart<'a> {
    pub fn new(body: &'a [u8], boundary: &str) -> Self {
        let mut marker = std::vec::Vec::with_capacity(boundary.len() + 2);
        marker.extend_from_slice(b"--");
        marker.extend_from_slice(boundary.as_bytes());
        Self {
            body,
            boundary_marker: marker,
        }
    }

    // helper to find byte sequence (SIMD-accelerated via memchr for first byte)
    fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        let first = needle[0];
        let mut offset = 0;
        while offset + needle.len() <= data.len() {
            match memchr(first, &data[offset..]) {
                Some(pos) => {
                    let abs = offset + pos;
                    if abs + needle.len() <= data.len() && data[abs..abs + needle.len()] == *needle
                    {
                        return Some(abs);
                    }
                    offset = abs + 1;
                }
                None => return None,
            }
        }
        None
    }
}

impl<'a> Iterator for Multipart<'a> {
    type Item = Result<Part<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.body.is_empty() {
            return None;
        }

        let mut start = Self::find(self.body, &self.boundary_marker)?;
        start += self.boundary_marker.len();

        // Check for -- (end of multiparts)
        if self.body.len() >= start + 2 && self.body[start] == b'-' && self.body[start + 1] == b'-'
        {
            self.body = &[];
            return None;
        }

        // Skip \r\n
        if self.body.len() >= start + 2
            && self.body[start] == b'\r'
            && self.body[start + 1] == b'\n'
        {
            start += 2;
        }

        // Parse headers until \r\n\r\n
        let header_end = Self::find(&self.body[start..], b"\r\n\r\n")?;
        let header_slice = &self.body[start..start + header_end];
        let body_start = start + header_end + 4;

        // find next boundary to determine body end
        let end_boundary_pos = Self::find(&self.body[body_start..], &self.boundary_marker);

        let body_end = match end_boundary_pos {
            Some(pos) => body_start + pos,
            None => return Some(Err(ParseError::Incomplete)),
        };

        // Body usually ends with \r\n before the boundary
        let actual_body_end = if body_end >= 2
            && self.body[body_end - 2] == b'\r'
            && self.body[body_end - 1] == b'\n'
        {
            body_end - 2
        } else {
            body_end
        };

        let body_slice = &self.body[body_start..actual_body_end];

        // Advance self.body to the next boundary
        self.body = &self.body[body_end..];

        // Very basic header parsing for name/filename/content-type
        let mut name = None;
        let mut filename = None;
        let mut content_type = None;

        let headers_str = std::str::from_utf8(header_slice).ok()?;
        for line in headers_str.split("\r\n") {
            if line
                .as_bytes()
                .get(..20)
                .is_some_and(|h| h.eq_ignore_ascii_case(b"content-disposition:"))
            {
                let rest = &line[20..];
                // parse name="foo" (case-insensitive key match without allocation)
                if let Some(idx) = rest.to_ascii_lowercase().find("name=\"") {
                    let after = &rest[idx + 6..];
                    if let Some(end) = after.find('"') {
                        name = Some(&after[..end]);
                    }
                }
                // parse filename="foo.txt"
                if let Some(idx) = rest.to_ascii_lowercase().find("filename=\"") {
                    let after = &rest[idx + 10..];
                    if let Some(end) = after.find('"') {
                        filename = Some(&after[..end]);
                    }
                }
            } else if line
                .as_bytes()
                .get(..13)
                .is_some_and(|h| h.eq_ignore_ascii_case(b"content-type:"))
            {
                content_type = Some(line[13..].trim());
            }
        }

        Some(Ok(Part {
            name,
            filename,
            content_type,
            body: body_slice,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary() -> &'static str {
        "testboundary"
    }

    /// Build a minimal single-field multipart body.
    fn single_field(field_name: &str, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"--testboundary\r\n");
        v.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                field_name
            )
            .as_bytes(),
        );
        v.extend_from_slice(body);
        v.extend_from_slice(b"\r\n--testboundary--\r\n");
        v
    }

    /// Build a two-field multipart body with optional filename.
    fn two_fields(n1: &str, b1: &[u8], n2: &str, file: Option<&str>, b2: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        // Part 1
        v.extend_from_slice(b"--testboundary\r\n");
        v.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", n1).as_bytes(),
        );
        v.extend_from_slice(b1);
        v.extend_from_slice(b"\r\n");
        // Part 2
        v.extend_from_slice(b"--testboundary\r\n");
        match file {
            Some(f) => v.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\r\n",
                    n2, f
                )
                .as_bytes(),
            ),
            None => v.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", n2).as_bytes(),
            ),
        }
        v.extend_from_slice(b2);
        v.extend_from_slice(b"\r\n--testboundary--\r\n");
        v
    }

    // ─── single part ─────────────────────────────────────────────────────────

    #[test]
    fn test_single_part_name_and_body() {
        let body = single_field("username", b"alice");
        let mut mp = Multipart::new(&body, boundary());
        let part = mp.next().expect("should have one part").unwrap();
        assert_eq!(part.name, Some("username"));
        assert_eq!(part.body, b"alice");
        assert!(mp.next().is_none(), "should have no more parts");
    }

    #[test]
    fn test_single_part_empty_body() {
        let body = single_field("field", b"");
        let mut mp = Multipart::new(&body, boundary());
        let part = mp.next().unwrap().unwrap();
        assert_eq!(part.name, Some("field"));
        assert_eq!(part.body, b"");
    }

    // ─── two parts ────────────────────────────────────────────────────────────

    #[test]
    fn test_two_parts_basic() {
        let body = two_fields("first", b"hello", "second", None, b"world");
        let mut mp = Multipart::new(&body, boundary());
        let p1 = mp.next().unwrap().unwrap();
        assert_eq!(p1.name, Some("first"));
        assert_eq!(p1.body, b"hello");
        let p2 = mp.next().unwrap().unwrap();
        assert_eq!(p2.name, Some("second"));
        assert_eq!(p2.body, b"world");
        assert!(mp.next().is_none());
    }

    #[test]
    fn test_file_part_has_filename() {
        let body = two_fields("meta", b"info", "upload", Some("photo.jpg"), b"JPEG_DATA");
        let mut mp = Multipart::new(&body, boundary());
        let _meta = mp.next().unwrap().unwrap();
        let file_part = mp.next().unwrap().unwrap();
        assert_eq!(file_part.name, Some("upload"));
        assert_eq!(file_part.filename, Some("photo.jpg"));
        assert_eq!(file_part.body, b"JPEG_DATA");
    }

    // ─── content-type header ─────────────────────────────────────────────────

    #[test]
    fn test_content_type_parsed() {
        let mut v = Vec::new();
        v.extend_from_slice(b"--testboundary\r\n");
        v.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"data.json\"\r\n",
        );
        v.extend_from_slice(b"Content-Type: application/json\r\n");
        v.extend_from_slice(b"\r\n");
        v.extend_from_slice(b"{\"key\":\"val\"}");
        v.extend_from_slice(b"\r\n--testboundary--\r\n");

        let mut mp = Multipart::new(&v, boundary());
        let part = mp.next().unwrap().unwrap();
        assert_eq!(part.content_type, Some("application/json"));
        assert_eq!(part.body, b"{\"key\":\"val\"}");
    }

    // ─── empty / missing boundary ────────────────────────────────────────────

    #[test]
    fn test_empty_body_yields_none() {
        let mut mp = Multipart::new(b"", boundary());
        assert!(mp.next().is_none());
    }

    #[test]
    fn test_no_boundary_in_body_yields_none() {
        // Body has content but not the right boundary
        let mut mp = Multipart::new(
            b"--wrongboundary\r\nsome data\r\n--wrongboundary--",
            boundary(),
        );
        assert!(mp.next().is_none());
    }

    // ─── incomplete body ─────────────────────────────────────────────────────

    #[test]
    fn test_missing_closing_boundary_returns_err() {
        // Part starts but never terminates with --boundary or --boundary--
        let mut v = Vec::new();
        v.extend_from_slice(b"--testboundary\r\n");
        v.extend_from_slice(b"Content-Disposition: form-data; name=\"f\"\r\n\r\n");
        v.extend_from_slice(b"truncated body with no closing boundary");
        let mut mp = Multipart::new(&v, boundary());
        let result = mp.next().unwrap();
        assert!(
            result.is_err(),
            "truncated body should return Err(Incomplete)"
        );
    }
}
