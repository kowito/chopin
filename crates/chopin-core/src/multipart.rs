use crate::parser::ParseError;

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

    // helper to find byte sequence
    fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        data.windows(needle.len()).position(|w| w == needle)
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
            let lower = line.to_lowercase();
            if lower.starts_with("content-disposition:") {
                // parse name="foo"; filename="foo.txt"
                if let Some(idx) = lower.find("name=\"") {
                    let rest = &line[idx + 6..];
                    if let Some(end) = rest.find('"') {
                        name = Some(&rest[..end]);
                    }
                }
                if let Some(idx) = lower.find("filename=\"") {
                    let rest = &line[idx + 10..];
                    if let Some(end) = rest.find('"') {
                        filename = Some(&rest[..end]);
                    }
                }
            } else if lower.starts_with("content-type:") {
                let ct = line[13..].trim();
                content_type = Some(ct);
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
