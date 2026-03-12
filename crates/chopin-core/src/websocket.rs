// src/websocket.rs — RFC 6455 WebSocket support
//
// Provides upgrade handshake validation, Sec-WebSocket-Accept key derivation,
// and a frame-level codec for reading/writing WebSocket frames over raw fds.

use crate::http::{Context, Request, Response};

// ── Constants ────────────────────────────────────────────────────────────────

/// The magic GUID from RFC 6455 §4.2.2, concatenated with the client's
/// `Sec-WebSocket-Key` to derive the accept key.
const WS_MAGIC: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// WebSocket frame opcodes (RFC 6455 §5.2).
pub const OPCODE_CONTINUATION: u8 = 0x0;
pub const OPCODE_TEXT: u8 = 0x1;
pub const OPCODE_BINARY: u8 = 0x2;
pub const OPCODE_CLOSE: u8 = 0x8;
pub const OPCODE_PING: u8 = 0x9;
pub const OPCODE_PONG: u8 = 0xA;

/// Maximum payload size we'll accept for a single frame (16 MiB).
const MAX_FRAME_PAYLOAD: u64 = 16 * 1024 * 1024;

// ── Handshake ────────────────────────────────────────────────────────────────

/// Check if a request is a valid WebSocket upgrade request.
///
/// Validates:
/// - `Upgrade: websocket` header (case-insensitive)
/// - `Connection: upgrade` header (case-insensitive, may contain multiple tokens)
/// - `Sec-WebSocket-Version: 13`
/// - `Sec-WebSocket-Key` present
pub fn is_upgrade_request(req: &Request<'_>) -> bool {
    let mut has_upgrade = false;
    let mut has_connection = false;
    let mut has_version_13 = false;
    let mut has_key = false;

    for i in 0..req.header_count as usize {
        let (name, val) = req.headers[i];
        if name.eq_ignore_ascii_case("upgrade") && val.eq_ignore_ascii_case("websocket") {
            has_upgrade = true;
        } else if name.eq_ignore_ascii_case("connection") {
            // Connection header may contain multiple comma-separated tokens
            for token in val.split(',') {
                if token.trim().eq_ignore_ascii_case("upgrade") {
                    has_connection = true;
                }
            }
        } else if name.eq_ignore_ascii_case("sec-websocket-version") && val == "13" {
            has_version_13 = true;
        } else if name.eq_ignore_ascii_case("sec-websocket-key") && !val.is_empty() {
            has_key = true;
        }
    }

    has_upgrade && has_connection && has_version_13 && has_key
}

/// Extract the `Sec-WebSocket-Key` header value from a request.
pub fn websocket_key<'a>(req: &'a Request<'a>) -> Option<&'a str> {
    for i in 0..req.header_count as usize {
        let (name, val) = req.headers[i];
        if name.eq_ignore_ascii_case("sec-websocket-key") {
            return Some(val);
        }
    }
    None
}

/// Compute the `Sec-WebSocket-Accept` value for a given client key.
///
/// Per RFC 6455 §4.2.2: `base64(SHA-1(key + MAGIC_GUID))`.
pub fn accept_key(client_key: &str) -> String {
    let mut input = Vec::with_capacity(client_key.len() + WS_MAGIC.len());
    input.extend_from_slice(client_key.as_bytes());
    input.extend_from_slice(WS_MAGIC);
    let hash = sha1(&input);
    base64_encode(&hash)
}

/// Build a `101 Switching Protocols` response for a valid WebSocket upgrade.
///
/// Returns `None` if the request is not a valid WS upgrade request.
pub fn upgrade_response(req: &Request<'_>) -> Option<Response> {
    if !is_upgrade_request(req) {
        return None;
    }
    let key = websocket_key(req)?;
    let accept = accept_key(key);

    let resp = Response::new(101)
        .with_header("Upgrade", "websocket")
        .with_header("Connection", "Upgrade")
        .with_header("Sec-WebSocket-Accept", accept);
    Some(resp)
}

/// Helper on [`Context`] to check for a WebSocket upgrade and produce the
/// 101 response in one call.
pub fn ws_upgrade(ctx: &Context<'_>) -> Option<Response> {
    upgrade_response(&ctx.req)
}

// ── Frame codec ──────────────────────────────────────────────────────────────

/// A decoded WebSocket frame.
#[derive(Debug, Clone)]
pub struct WsFrame {
    /// `true` if this is the final fragment.
    pub fin: bool,
    /// Opcode (OPCODE_TEXT, OPCODE_BINARY, etc.).
    pub opcode: u8,
    /// The unmasked payload data.
    pub payload: Vec<u8>,
}

/// A high-level WebSocket message (assembled from one or more frames).
#[derive(Debug, Clone, PartialEq)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<(u16, String)>),
}

/// Error type for WebSocket operations.
#[derive(Debug)]
pub enum WsError {
    /// Not enough data in the buffer to parse a complete frame.
    Incomplete,
    /// Protocol violation (reserved bits set, invalid opcode, etc.).
    Protocol(&'static str),
    /// Payload exceeds the maximum allowed size.
    PayloadTooLarge,
    /// UTF-8 validation failed on a text frame.
    InvalidUtf8,
}

/// Decode a single WebSocket frame from a byte buffer.
///
/// Returns the parsed frame and the number of bytes consumed from `buf`.
/// Client-to-server frames MUST be masked (RFC 6455 §5.1).
pub fn decode_frame(buf: &[u8]) -> Result<(WsFrame, usize), WsError> {
    if buf.len() < 2 {
        return Err(WsError::Incomplete);
    }

    let b0 = buf[0];
    let b1 = buf[1];

    let fin = b0 & 0x80 != 0;
    let rsv = b0 & 0x70;
    if rsv != 0 {
        return Err(WsError::Protocol("reserved bits must be 0"));
    }
    let opcode = b0 & 0x0F;
    let masked = b1 & 0x80 != 0;
    let mut payload_len = (b1 & 0x7F) as u64;

    let mut offset = 2usize;

    if payload_len == 126 {
        if buf.len() < offset + 2 {
            return Err(WsError::Incomplete);
        }
        payload_len = u16::from_be_bytes([buf[offset], buf[offset + 1]]) as u64;
        offset += 2;
    } else if payload_len == 127 {
        if buf.len() < offset + 8 {
            return Err(WsError::Incomplete);
        }
        payload_len = u64::from_be_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]);
        offset += 8;
    }

    if payload_len > MAX_FRAME_PAYLOAD {
        return Err(WsError::PayloadTooLarge);
    }

    let mask_key = if masked {
        if buf.len() < offset + 4 {
            return Err(WsError::Incomplete);
        }
        let key = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
        offset += 4;
        Some(key)
    } else {
        None
    };

    let payload_len = payload_len as usize;
    if buf.len() < offset + payload_len {
        return Err(WsError::Incomplete);
    }

    let mut payload = buf[offset..offset + payload_len].to_vec();

    // Unmask if masked
    if let Some(key) = mask_key {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= key[i & 3];
        }
    }

    Ok((
        WsFrame {
            fin,
            opcode,
            payload,
        },
        offset + payload_len,
    ))
}

/// Encode a WebSocket frame for sending (server → client, unmasked).
///
/// Returns the encoded frame bytes ready for writing.
pub fn encode_frame(opcode: u8, payload: &[u8], fin: bool) -> Vec<u8> {
    let mut frame = Vec::with_capacity(10 + payload.len());
    let b0 = if fin { 0x80 | opcode } else { opcode };
    frame.push(b0);

    // Server-to-client frames MUST NOT be masked (RFC 6455 §5.1).
    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 0xFFFF {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(payload);
    frame
}

/// Encode a text message frame.
pub fn encode_text(text: &str) -> Vec<u8> {
    encode_frame(OPCODE_TEXT, text.as_bytes(), true)
}

/// Encode a binary message frame.
pub fn encode_binary(data: &[u8]) -> Vec<u8> {
    encode_frame(OPCODE_BINARY, data, true)
}

/// Encode a close frame with an optional status code and reason.
pub fn encode_close(code: Option<u16>, reason: &str) -> Vec<u8> {
    match code {
        Some(c) => {
            let mut payload = Vec::with_capacity(2 + reason.len());
            payload.extend_from_slice(&c.to_be_bytes());
            payload.extend_from_slice(reason.as_bytes());
            encode_frame(OPCODE_CLOSE, &payload, true)
        }
        None => encode_frame(OPCODE_CLOSE, &[], true),
    }
}

/// Encode a ping frame.
pub fn encode_ping(data: &[u8]) -> Vec<u8> {
    encode_frame(OPCODE_PING, data, true)
}

/// Encode a pong frame (response to a ping).
pub fn encode_pong(data: &[u8]) -> Vec<u8> {
    encode_frame(OPCODE_PONG, data, true)
}

/// Parse a close frame's payload into a status code and reason text.
pub fn parse_close_payload(payload: &[u8]) -> Option<(u16, String)> {
    if payload.len() < 2 {
        return None;
    }
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    let reason = String::from_utf8_lossy(&payload[2..]).into_owned();
    Some((code, reason))
}

// ── Minimal SHA-1 (RFC 3174) ────────────────────────────────────────────────
// Only used for WebSocket accept key derivation. Not for security purposes.

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    // Pre-processing: add padding
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) block
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

// ── Minimal Base64 encode ────────────────────────────────────────────────────

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_empty() {
        let hash = sha1(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn test_sha1_abc() {
        let hash = sha1(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hi"), "SGk=");
        assert_eq!(base64_encode(b"Hel"), "SGVs");
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_sha1_multiblock() {
        // SHA-1 of the WebSocket key+GUID (60 bytes, requires 2 blocks)
        let input = b"dGhlIHNhbXBsZSBub25jZQ==258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        assert_eq!(input.len(), 60);
        let hash = sha1(input);
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "b37a4f2cc0624f1690f64606cf385945b2bec4ea");
    }

    #[test]
    fn test_sha1_nist_vector() {
        // NIST test vector: 56 bytes, requires 2 SHA-1 blocks
        let hash = sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
    }

    #[test]
    fn test_accept_key_rfc6455_example() {
        // RFC 6455 §4.2.2 example
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let accept = accept_key(key);
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn test_is_upgrade_request_valid() {
        let req = Request {
            method: crate::http::Method::Get,
            path: "/ws",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Upgrade", "websocket");
                h[1] = ("Connection", "Upgrade");
                h[2] = ("Sec-WebSocket-Version", "13");
                h[3] = ("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==");
                h
            },
            header_count: 4,
            body: &[],
        };
        assert!(is_upgrade_request(&req));
    }

    #[test]
    fn test_is_upgrade_request_missing_key() {
        let req = Request {
            method: crate::http::Method::Get,
            path: "/ws",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Upgrade", "websocket");
                h[1] = ("Connection", "Upgrade");
                h[2] = ("Sec-WebSocket-Version", "13");
                h
            },
            header_count: 3,
            body: &[],
        };
        assert!(!is_upgrade_request(&req));
    }

    #[test]
    fn test_is_upgrade_request_wrong_version() {
        let req = Request {
            method: crate::http::Method::Get,
            path: "/ws",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Upgrade", "websocket");
                h[1] = ("Connection", "Upgrade");
                h[2] = ("Sec-WebSocket-Version", "8");
                h[3] = ("Sec-WebSocket-Key", "abc123");
                h
            },
            header_count: 4,
            body: &[],
        };
        assert!(!is_upgrade_request(&req));
    }

    #[test]
    fn test_encode_decode_text_frame() {
        let encoded = encode_text("Hello, WebSocket!");
        // Server frames are unmasked, so we can decode directly
        let (frame, consumed) = decode_frame(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert!(frame.fin);
        assert_eq!(frame.opcode, OPCODE_TEXT);
        assert_eq!(frame.payload, b"Hello, WebSocket!");
    }

    #[test]
    fn test_encode_decode_binary_frame() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let encoded = encode_binary(&data);
        let (frame, _) = decode_frame(&encoded).unwrap();
        assert!(frame.fin);
        assert_eq!(frame.opcode, OPCODE_BINARY);
        assert_eq!(frame.payload, data);
    }

    #[test]
    fn test_decode_masked_frame() {
        // Build a masked text frame "Hi" with mask key [0x37, 0xfa, 0x21, 0x3d]
        let payload = b"Hi";
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let mut frame = vec![0x81u8, 0x82]; // FIN + TEXT, MASKED + len=2
        frame.extend_from_slice(&mask);
        // Mask the payload
        for (i, &b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i & 3]);
        }

        let (decoded, consumed) = decode_frame(&frame).unwrap();
        assert_eq!(consumed, frame.len());
        assert!(decoded.fin);
        assert_eq!(decoded.opcode, OPCODE_TEXT);
        assert_eq!(decoded.payload, b"Hi");
    }

    #[test]
    fn test_encode_decode_close_frame() {
        let encoded = encode_close(Some(1000), "Normal Closure");
        let (frame, _) = decode_frame(&encoded).unwrap();
        assert_eq!(frame.opcode, OPCODE_CLOSE);
        let (code, reason) = parse_close_payload(&frame.payload).unwrap();
        assert_eq!(code, 1000);
        assert_eq!(reason, "Normal Closure");
    }

    #[test]
    fn test_encode_close_no_code() {
        let encoded = encode_close(None, "");
        let (frame, _) = decode_frame(&encoded).unwrap();
        assert_eq!(frame.opcode, OPCODE_CLOSE);
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn test_encode_decode_ping_pong() {
        let ping = encode_ping(b"ping-data");
        let (frame, _) = decode_frame(&ping).unwrap();
        assert_eq!(frame.opcode, OPCODE_PING);
        assert_eq!(frame.payload, b"ping-data");

        let pong = encode_pong(b"ping-data");
        let (frame, _) = decode_frame(&pong).unwrap();
        assert_eq!(frame.opcode, OPCODE_PONG);
        assert_eq!(frame.payload, b"ping-data");
    }

    #[test]
    fn test_decode_incomplete() {
        assert!(matches!(decode_frame(&[0x81]), Err(WsError::Incomplete)));
        assert!(matches!(decode_frame(&[]), Err(WsError::Incomplete)));
    }

    #[test]
    fn test_decode_reserved_bits() {
        // RSV1 set without extension negotiation
        let frame = [0xC1, 0x00]; // FIN + RSV1 + TEXT, len=0
        assert!(matches!(
            decode_frame(&frame),
            Err(WsError::Protocol(_))
        ));
    }

    #[test]
    fn test_encode_medium_payload() {
        // Test 126-byte length encoding path
        let data = vec![0x42u8; 300];
        let encoded = encode_frame(OPCODE_BINARY, &data, true);
        let (frame, consumed) = decode_frame(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(frame.payload.len(), 300);
    }

    #[test]
    fn test_upgrade_response() {
        let req = Request {
            method: crate::http::Method::Get,
            path: "/ws",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Upgrade", "websocket");
                h[1] = ("Connection", "Upgrade");
                h[2] = ("Sec-WebSocket-Version", "13");
                h[3] = ("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==");
                h
            },
            header_count: 4,
            body: &[],
        };

        let resp = upgrade_response(&req).unwrap();
        assert_eq!(resp.status, 101);
    }

    #[test]
    fn test_upgrade_response_not_ws() {
        let req = Request {
            method: crate::http::Method::Get,
            path: "/api",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Host", "example.com");
                h
            },
            header_count: 1,
            body: &[],
        };

        assert!(upgrade_response(&req).is_none());
    }
}
