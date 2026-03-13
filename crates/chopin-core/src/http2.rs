// src/http2.rs — HTTP/2 frame codec and connection preface support (RFC 9113)
//
// Provides protocol detection, frame-level encode/decode, SETTINGS exchange,
// and h2c upgrade detection. Full stream multiplexing is layered above this.

// ── Connection Preface ───────────────────────────────────────────────────────

/// The HTTP/2 connection preface sent by the client (RFC 9113 §3.4).
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Check if a buffer starts with the HTTP/2 connection preface.
///
/// This is used for h2c (HTTP/2 cleartext) with prior knowledge — the client
/// sends this magic string immediately after connecting.
pub fn is_h2_preface(buf: &[u8]) -> bool {
    buf.len() >= CONNECTION_PREFACE.len() && buf.starts_with(CONNECTION_PREFACE)
}

/// Check if an HTTP/1.1 request is an h2c upgrade request.
///
/// Returns `true` if both `Upgrade: h2c` and `HTTP2-Settings` headers are present.
pub fn is_h2c_upgrade(req: &crate::http::Request<'_>) -> bool {
    let mut has_upgrade = false;
    let mut has_settings = false;

    for i in 0..req.header_count as usize {
        let (name, val) = req.headers[i];
        if name.eq_ignore_ascii_case("upgrade") && val.eq_ignore_ascii_case("h2c") {
            has_upgrade = true;
        } else if name.eq_ignore_ascii_case("http2-settings") {
            has_settings = true;
        }
    }

    has_upgrade && has_settings
}

// ── Frame types (RFC 9113 §4) ────────────────────────────────────────────────

/// Frame type identifiers.
pub const FRAME_DATA: u8 = 0x0;
pub const FRAME_HEADERS: u8 = 0x1;
pub const FRAME_PRIORITY: u8 = 0x2;
pub const FRAME_RST_STREAM: u8 = 0x3;
pub const FRAME_SETTINGS: u8 = 0x4;
pub const FRAME_PUSH_PROMISE: u8 = 0x5;
pub const FRAME_PING: u8 = 0x6;
pub const FRAME_GOAWAY: u8 = 0x7;
pub const FRAME_WINDOW_UPDATE: u8 = 0x8;
pub const FRAME_CONTINUATION: u8 = 0x9;

/// Frame flags.
pub const FLAG_ACK: u8 = 0x1;
pub const FLAG_END_STREAM: u8 = 0x1;
pub const FLAG_END_HEADERS: u8 = 0x4;
pub const FLAG_PADDED: u8 = 0x8;
pub const FLAG_PRIORITY: u8 = 0x20;

/// The fixed 9-byte HTTP/2 frame header.
pub const FRAME_HEADER_SIZE: usize = 9;

/// Maximum frame payload size (default per RFC 9113 §4.2).
pub const DEFAULT_MAX_FRAME_SIZE: u32 = 16_384;

/// Maximum allowed SETTINGS_MAX_FRAME_SIZE (RFC 9113 §6.5.2).
pub const MAX_FRAME_SIZE_LIMIT: u32 = 16_777_215;

// ── SETTINGS identifiers (RFC 9113 §6.5.2) ──────────────────────────────────

pub const SETTINGS_HEADER_TABLE_SIZE: u16 = 0x1;
pub const SETTINGS_ENABLE_PUSH: u16 = 0x2;
pub const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
pub const SETTINGS_INITIAL_WINDOW_SIZE: u16 = 0x4;
pub const SETTINGS_MAX_FRAME_SIZE: u16 = 0x5;
pub const SETTINGS_MAX_HEADER_LIST_SIZE: u16 = 0x6;

// ── Error codes (RFC 9113 §7) ────────────────────────────────────────────────

pub const ERROR_NO_ERROR: u32 = 0x0;
pub const ERROR_PROTOCOL_ERROR: u32 = 0x1;
pub const ERROR_INTERNAL_ERROR: u32 = 0x2;
pub const ERROR_FLOW_CONTROL_ERROR: u32 = 0x3;
pub const ERROR_SETTINGS_TIMEOUT: u32 = 0x4;
pub const ERROR_STREAM_CLOSED: u32 = 0x5;
pub const ERROR_FRAME_SIZE_ERROR: u32 = 0x6;
pub const ERROR_REFUSED_STREAM: u32 = 0x7;
pub const ERROR_CANCEL: u32 = 0x8;
pub const ERROR_COMPRESSION_ERROR: u32 = 0x9;
pub const ERROR_CONNECT_ERROR: u32 = 0xA;
pub const ERROR_ENHANCE_YOUR_CALM: u32 = 0xB;
pub const ERROR_INADEQUATE_SECURITY: u32 = 0xC;
pub const ERROR_HTTP_1_1_REQUIRED: u32 = 0xD;

// ── Frame header ─────────────────────────────────────────────────────────────

/// A parsed HTTP/2 frame header (9 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameHeader {
    /// Payload length (24 bits, max 16,777,215).
    pub length: u32,
    /// Frame type (DATA, HEADERS, SETTINGS, etc.).
    pub frame_type: u8,
    /// Frame flags (END_STREAM, END_HEADERS, ACK, etc.).
    pub flags: u8,
    /// Stream identifier (31 bits). 0 for connection-level frames.
    pub stream_id: u32,
}

impl FrameHeader {
    /// Decode a frame header from a 9-byte buffer.
    pub fn decode(buf: &[u8; 9]) -> Self {
        let length = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
        let frame_type = buf[3];
        let flags = buf[4];
        let stream_id =
            ((buf[5] as u32) << 24) | ((buf[6] as u32) << 16) | ((buf[7] as u32) << 8) | (buf[8] as u32);
        // Clear the reserved bit (R)
        let stream_id = stream_id & 0x7FFF_FFFF;

        Self {
            length,
            frame_type,
            flags,
            stream_id,
        }
    }

    /// Encode this frame header into 9 bytes.
    pub fn encode(&self) -> [u8; 9] {
        let mut buf = [0u8; 9];
        buf[0] = ((self.length >> 16) & 0xFF) as u8;
        buf[1] = ((self.length >> 8) & 0xFF) as u8;
        buf[2] = (self.length & 0xFF) as u8;
        buf[3] = self.frame_type;
        buf[4] = self.flags;
        buf[5] = ((self.stream_id >> 24) & 0x7F) as u8; // R bit must be 0
        buf[6] = ((self.stream_id >> 16) & 0xFF) as u8;
        buf[7] = ((self.stream_id >> 8) & 0xFF) as u8;
        buf[8] = (self.stream_id & 0xFF) as u8;
        buf
    }
}

// ── Frame ────────────────────────────────────────────────────────────────────

/// A complete HTTP/2 frame (header + payload).
#[derive(Debug, Clone)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

/// Errors during frame decoding.
#[derive(Debug)]
pub enum H2Error {
    /// Not enough data in the buffer.
    Incomplete,
    /// Frame payload exceeds max frame size.
    FrameSizeError,
    /// Protocol violation.
    ProtocolError(&'static str),
}

/// Decode a single HTTP/2 frame from a byte buffer.
///
/// Returns the frame and how many bytes were consumed (header + payload).
pub fn decode_frame(buf: &[u8], max_frame_size: u32) -> Result<(Frame, usize), H2Error> {
    if buf.len() < FRAME_HEADER_SIZE {
        return Err(H2Error::Incomplete);
    }

    let header = FrameHeader::decode(buf[..9].try_into().unwrap());

    if header.length > max_frame_size {
        return Err(H2Error::FrameSizeError);
    }

    let total = FRAME_HEADER_SIZE + header.length as usize;
    if buf.len() < total {
        return Err(H2Error::Incomplete);
    }

    let payload = buf[FRAME_HEADER_SIZE..total].to_vec();

    Ok((Frame { header, payload }, total))
}

/// Encode a frame (header + payload) into a byte vector.
pub fn encode_frame(frame_type: u8, flags: u8, stream_id: u32, payload: &[u8]) -> Vec<u8> {
    let header = FrameHeader {
        length: payload.len() as u32,
        frame_type,
        flags,
        stream_id,
    };
    let mut buf = Vec::with_capacity(FRAME_HEADER_SIZE + payload.len());
    buf.extend_from_slice(&header.encode());
    buf.extend_from_slice(payload);
    buf
}

// ── Settings ─────────────────────────────────────────────────────────────────

/// Connection-level settings exchanged during the handshake.
#[derive(Debug, Clone)]
pub struct Settings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: true,
            max_concurrent_streams: 100,
            initial_window_size: 65_535,
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_list_size: 8192,
        }
    }
}

impl Settings {
    /// Parse a SETTINGS frame payload into individual settings.
    /// Each setting is a 16-bit identifier + 32-bit value (6 bytes).
    pub fn from_payload(payload: &[u8]) -> Result<Self, H2Error> {
        if !payload.len().is_multiple_of(6) {
            return Err(H2Error::ProtocolError("SETTINGS payload must be multiple of 6 bytes"));
        }

        let mut s = Self::default();
        for chunk in payload.chunks_exact(6) {
            let id = u16::from_be_bytes([chunk[0], chunk[1]]);
            let val = u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]);

            match id {
                SETTINGS_HEADER_TABLE_SIZE => s.header_table_size = val,
                SETTINGS_ENABLE_PUSH => s.enable_push = val != 0,
                SETTINGS_MAX_CONCURRENT_STREAMS => s.max_concurrent_streams = val,
                SETTINGS_INITIAL_WINDOW_SIZE => {
                    if val > 0x7FFF_FFFF {
                        return Err(H2Error::ProtocolError("initial window size too large"));
                    }
                    s.initial_window_size = val;
                }
                SETTINGS_MAX_FRAME_SIZE => {
                    if !(DEFAULT_MAX_FRAME_SIZE..=MAX_FRAME_SIZE_LIMIT).contains(&val) {
                        return Err(H2Error::ProtocolError("max frame size out of range"));
                    }
                    s.max_frame_size = val;
                }
                SETTINGS_MAX_HEADER_LIST_SIZE => s.max_header_list_size = val,
                _ => {} // Unknown settings MUST be ignored (RFC 9113 §6.5.2)
            }
        }
        Ok(s)
    }

    /// Encode server settings into a SETTINGS frame payload.
    pub fn to_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(36);
        let settings = [
            (SETTINGS_HEADER_TABLE_SIZE, self.header_table_size),
            (SETTINGS_ENABLE_PUSH, self.enable_push as u32),
            (SETTINGS_MAX_CONCURRENT_STREAMS, self.max_concurrent_streams),
            (SETTINGS_INITIAL_WINDOW_SIZE, self.initial_window_size),
            (SETTINGS_MAX_FRAME_SIZE, self.max_frame_size),
            (SETTINGS_MAX_HEADER_LIST_SIZE, self.max_header_list_size),
        ];
        for (id, val) in settings {
            payload.extend_from_slice(&id.to_be_bytes());
            payload.extend_from_slice(&val.to_be_bytes());
        }
        payload
    }

    /// Encode a SETTINGS frame (including the 9-byte header).
    pub fn to_frame(&self) -> Vec<u8> {
        let payload = self.to_payload();
        encode_frame(FRAME_SETTINGS, 0, 0, &payload)
    }

    /// Encode a SETTINGS ACK frame (empty payload, ACK flag set).
    pub fn ack_frame() -> Vec<u8> {
        encode_frame(FRAME_SETTINGS, FLAG_ACK, 0, &[])
    }
}

// ── Helper frame builders ────────────────────────────────────────────────────

/// Build a GOAWAY frame.
pub fn goaway_frame(last_stream_id: u32, error_code: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&last_stream_id.to_be_bytes());
    payload.extend_from_slice(&error_code.to_be_bytes());
    encode_frame(FRAME_GOAWAY, 0, 0, &payload)
}

/// Build a RST_STREAM frame.
pub fn rst_stream_frame(stream_id: u32, error_code: u32) -> Vec<u8> {
    encode_frame(FRAME_RST_STREAM, 0, stream_id, &error_code.to_be_bytes())
}

/// Build a WINDOW_UPDATE frame.
pub fn window_update_frame(stream_id: u32, increment: u32) -> Vec<u8> {
    let val = increment & 0x7FFF_FFFF;
    encode_frame(FRAME_WINDOW_UPDATE, 0, stream_id, &val.to_be_bytes())
}

/// Build a PING frame.
pub fn ping_frame(data: &[u8; 8]) -> Vec<u8> {
    encode_frame(FRAME_PING, 0, 0, data)
}

/// Build a PING ACK frame.
pub fn ping_ack_frame(data: &[u8; 8]) -> Vec<u8> {
    encode_frame(FRAME_PING, FLAG_ACK, 0, data)
}

/// Build the server connection preface: a SETTINGS frame.
///
/// Per RFC 9113 §3.4, the server preface is a SETTINGS frame (possibly empty).
/// This returns a SETTINGS frame with our default values.
pub fn server_preface(settings: &Settings) -> Vec<u8> {
    settings.to_frame()
}

// ── h2c Upgrade Response ─────────────────────────────────────────────────────

/// Build the HTTP/1.1 101 Switching Protocols response for h2c upgrade.
///
/// After sending this, the server MUST send the server connection preface
/// (a SETTINGS frame) followed by the response to the original request
/// on stream 1.
pub fn h2c_upgrade_response() -> Vec<u8> {
    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: h2c\r\n\r\n".to_vec()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_h2_preface() {
        assert!(is_h2_preface(CONNECTION_PREFACE));
        assert!(is_h2_preface(
            b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\nextra data"
        ));
        assert!(!is_h2_preface(b"GET / HTTP/1.1\r\n"));
        assert!(!is_h2_preface(b"PRI *"));
    }

    #[test]
    fn test_frame_header_roundtrip() {
        let hdr = FrameHeader {
            length: 1234,
            frame_type: FRAME_HEADERS,
            flags: FLAG_END_HEADERS | FLAG_END_STREAM,
            stream_id: 42,
        };
        let encoded = hdr.encode();
        let decoded = FrameHeader::decode(&encoded);
        assert_eq!(decoded, hdr);
    }

    #[test]
    fn test_frame_header_reserved_bit_cleared() {
        // Set the reserved bit (MSB of stream ID)
        let mut buf = [0u8; 9];
        buf[5] = 0x80; // R bit set
        buf[8] = 1; // stream_id = 1
        let hdr = FrameHeader::decode(&buf);
        assert_eq!(hdr.stream_id, 1); // R bit cleared
    }

    #[test]
    fn test_settings_default_roundtrip() {
        let settings = Settings::default();
        let payload = settings.to_payload();
        let parsed = Settings::from_payload(&payload).unwrap();
        assert_eq!(parsed.header_table_size, 4096);
        assert!(parsed.enable_push);
        assert_eq!(parsed.max_concurrent_streams, 100);
        assert_eq!(parsed.initial_window_size, 65_535);
        assert_eq!(parsed.max_frame_size, 16_384);
        assert_eq!(parsed.max_header_list_size, 8192);
    }

    #[test]
    fn test_settings_bad_payload_length() {
        let payload = [0u8; 7]; // Not a multiple of 6
        assert!(matches!(
            Settings::from_payload(&payload),
            Err(H2Error::ProtocolError(_))
        ));
    }

    #[test]
    fn test_settings_window_size_too_large() {
        let mut payload = [0u8; 6];
        payload[0..2].copy_from_slice(&SETTINGS_INITIAL_WINDOW_SIZE.to_be_bytes());
        payload[2..6].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        assert!(matches!(
            Settings::from_payload(&payload),
            Err(H2Error::ProtocolError(_))
        ));
    }

    #[test]
    fn test_settings_frame_size_too_small() {
        let mut payload = [0u8; 6];
        payload[0..2].copy_from_slice(&SETTINGS_MAX_FRAME_SIZE.to_be_bytes());
        payload[2..6].copy_from_slice(&100u32.to_be_bytes()); // Below 16384 minimum
        assert!(matches!(
            Settings::from_payload(&payload),
            Err(H2Error::ProtocolError(_))
        ));
    }

    #[test]
    fn test_settings_unknown_id_ignored() {
        let mut payload = [0u8; 6];
        payload[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes()); // Unknown ID
        payload[2..6].copy_from_slice(&999u32.to_be_bytes());
        let settings = Settings::from_payload(&payload).unwrap();
        // Should use defaults since we only got an unknown setting
        assert_eq!(settings.header_table_size, 4096);
    }

    #[test]
    fn test_settings_ack_frame() {
        let ack = Settings::ack_frame();
        assert_eq!(ack.len(), 9); // Just a frame header, no payload
        let (frame, consumed) = decode_frame(&ack, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(consumed, 9);
        assert_eq!(frame.header.frame_type, FRAME_SETTINGS);
        assert_eq!(frame.header.flags, FLAG_ACK);
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn test_decode_frame_incomplete() {
        assert!(matches!(
            decode_frame(&[0; 5], DEFAULT_MAX_FRAME_SIZE),
            Err(H2Error::Incomplete)
        ));
    }

    #[test]
    fn test_decode_frame_size_error() {
        // Frame header claiming 32KB payload
        let mut buf = vec![0u8; 9];
        buf[0] = 0;
        buf[1] = 0x80;
        buf[2] = 0x00; // length = 32768
        // This exceeds default max of 16384
        assert!(matches!(
            decode_frame(&buf, DEFAULT_MAX_FRAME_SIZE),
            Err(H2Error::FrameSizeError)
        ));
    }

    #[test]
    fn test_encode_decode_data_frame() {
        let data = b"Hello, HTTP/2!";
        let encoded = encode_frame(FRAME_DATA, FLAG_END_STREAM, 1, data);
        let (frame, consumed) = decode_frame(&encoded, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(frame.header.frame_type, FRAME_DATA);
        assert_eq!(frame.header.flags, FLAG_END_STREAM);
        assert_eq!(frame.header.stream_id, 1);
        assert_eq!(frame.payload, data);
    }

    #[test]
    fn test_goaway_frame() {
        let goaway = goaway_frame(0, ERROR_NO_ERROR);
        let (frame, _) = decode_frame(&goaway, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.frame_type, FRAME_GOAWAY);
        assert_eq!(frame.header.stream_id, 0);
        assert_eq!(frame.payload.len(), 8);
    }

    #[test]
    fn test_rst_stream_frame() {
        let rst = rst_stream_frame(3, ERROR_CANCEL);
        let (frame, _) = decode_frame(&rst, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.frame_type, FRAME_RST_STREAM);
        assert_eq!(frame.header.stream_id, 3);
        let code = u32::from_be_bytes(frame.payload[..4].try_into().unwrap());
        assert_eq!(code, ERROR_CANCEL);
    }

    #[test]
    fn test_window_update_frame() {
        let wu = window_update_frame(0, 65535);
        let (frame, _) = decode_frame(&wu, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.frame_type, FRAME_WINDOW_UPDATE);
        let inc = u32::from_be_bytes(frame.payload[..4].try_into().unwrap());
        assert_eq!(inc, 65535);
    }

    #[test]
    fn test_ping_roundtrip() {
        let data = b"12345678";
        let ping = ping_frame(data);
        let (frame, _) = decode_frame(&ping, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.frame_type, FRAME_PING);
        assert_eq!(frame.header.flags, 0); // Not ACK
        assert_eq!(&frame.payload[..], &data[..]);

        let pong = ping_ack_frame(data);
        let (frame, _) = decode_frame(&pong, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.flags, FLAG_ACK);
        assert_eq!(&frame.payload[..], &data[..]);
    }

    #[test]
    fn test_h2c_upgrade_detection() {
        let req = crate::http::Request {
            method: crate::http::Method::Get,
            path: "/",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Host", "example.com");
                h[1] = ("Upgrade", "h2c");
                h[2] = ("HTTP2-Settings", "AAMAAABkAARAAAAAAAIAAAAA");
                h[3] = ("Connection", "Upgrade, HTTP2-Settings");
                h
            },
            header_count: 4,
            body: &[],
        };
        assert!(is_h2c_upgrade(&req));
    }

    #[test]
    fn test_h2c_upgrade_not_h2c() {
        let req = crate::http::Request {
            method: crate::http::Method::Get,
            path: "/",
            query: None,
            headers: {
                let mut h = [("", ""); crate::http::MAX_HEADERS];
                h[0] = ("Host", "example.com");
                h
            },
            header_count: 1,
            body: &[],
        };
        assert!(!is_h2c_upgrade(&req));
    }

    #[test]
    fn test_h2c_upgrade_response() {
        let resp = h2c_upgrade_response();
        assert!(resp.starts_with(b"HTTP/1.1 101"));
        assert!(resp.windows(4).any(|w| w == b"h2c\r"));
    }

    #[test]
    fn test_server_preface() {
        let settings = Settings::default();
        let preface = server_preface(&settings);
        let (frame, _) = decode_frame(&preface, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(frame.header.frame_type, FRAME_SETTINGS);
        assert_eq!(frame.header.stream_id, 0);
    }
}
