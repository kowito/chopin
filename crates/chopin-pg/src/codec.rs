//! Zero-copy binary codec for PostgreSQL v3 wire protocol.
//!
//! All encoding writes directly into a caller-provided buffer.
//! All decoding slices directly from the read buffer.

use crate::error::PgError;
use crate::protocol::*;

/// Maximum size of a single PG message we'll accept (16 MB).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

// ─── Encoding (Frontend → Server) ─────────────────────────────

/// Encode a StartupMessage into `buf`. Returns bytes written.
///
/// Format: Int32(len) Int32(196608=v3.0) { CString(param) CString(value) }* \0
pub fn encode_startup(
    buf: &mut [u8],
    user: &str,
    database: &str,
    params: &[(&str, &str)],
) -> usize {
    let mut pos = 4; // reserve length prefix

    // Protocol version 3.0
    put_i32(buf, pos, 196608);
    pos += 4;

    // user
    pos += put_cstring(buf, pos, "user");
    pos += put_cstring(buf, pos, user);

    // database
    pos += put_cstring(buf, pos, "database");
    pos += put_cstring(buf, pos, database);

    // extra params
    for (k, v) in params {
        pos += put_cstring(buf, pos, k);
        pos += put_cstring(buf, pos, v);
    }

    // terminator
    buf[pos] = 0;
    pos += 1;

    // write length
    put_i32(buf, 0, pos as i32);
    pos
}

/// Encode a PasswordMessage (cleartext or md5 response).
pub fn encode_password(buf: &mut [u8], password: &str) -> usize {
    let mut pos = 0;
    buf[pos] = b'p';
    pos += 1;
    let len = 4 + password.len() + 1; // length includes self + string + null
    put_i32(buf, pos, len as i32);
    pos += 4;
    pos += put_cstring(buf, pos, password);
    pos
}

/// Encode a SASLInitialResponse message.
pub fn encode_sasl_initial(buf: &mut [u8], mechanism: &str, data: &[u8]) -> usize {
    let mut pos = 0;
    buf[pos] = b'p';
    pos += 1;

    // length placeholder
    let len_pos = pos;
    pos += 4;

    // mechanism name (C string)
    pos += put_cstring(buf, pos, mechanism);

    // client-first-message length
    put_i32(buf, pos, data.len() as i32);
    pos += 4;

    // client-first-message data
    buf[pos..pos + data.len()].copy_from_slice(data);
    pos += data.len();

    // fill in length
    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode a SASLResponse message (client-final-message).
pub fn encode_sasl_response(buf: &mut [u8], data: &[u8]) -> usize {
    let mut pos = 0;
    buf[pos] = b'p';
    pos += 1;

    let len = 4 + data.len();
    put_i32(buf, pos, len as i32);
    pos += 4;

    buf[pos..pos + data.len()].copy_from_slice(data);
    pos += data.len();
    pos
}

/// Encode a Simple Query message ('Q').
pub fn encode_query(buf: &mut [u8], sql: &str) -> usize {
    let mut pos = 0;
    buf[pos] = b'Q';
    pos += 1;
    let len = 4 + sql.len() + 1;
    put_i32(buf, pos, len as i32);
    pos += 4;
    pos += put_cstring(buf, pos, sql);
    pos
}

/// Encode a Parse message ('P') for the extended query protocol.
pub fn encode_parse(buf: &mut [u8], stmt_name: &str, sql: &str, param_oids: &[i32]) -> usize {
    let mut pos = 0;
    buf[pos] = b'P';
    pos += 1;

    let len_pos = pos;
    pos += 4;

    pos += put_cstring(buf, pos, stmt_name);
    pos += put_cstring(buf, pos, sql);

    // number of parameter type OIDs
    put_i16(buf, pos, param_oids.len() as i16);
    pos += 2;
    for &oid in param_oids {
        put_i32(buf, pos, oid);
        pos += 4;
    }

    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode a Bind message ('B').
pub fn encode_bind(
    buf: &mut [u8],
    portal: &str,
    stmt_name: &str,
    param_formats: &[i16],
    param_values: &[Option<&[u8]>],
    result_formats: &[i16],
) -> usize {
    let mut pos = 0;
    buf[pos] = b'B';
    pos += 1;

    let len_pos = pos;
    pos += 4;

    pos += put_cstring(buf, pos, portal);
    pos += put_cstring(buf, pos, stmt_name);

    // parameter format codes
    put_i16(buf, pos, param_formats.len() as i16);
    pos += 2;
    for &f in param_formats {
        put_i16(buf, pos, f);
        pos += 2;
    }

    // parameter values
    put_i16(buf, pos, param_values.len() as i16);
    pos += 2;
    for val in param_values {
        match val {
            Some(data) => {
                put_i32(buf, pos, data.len() as i32);
                pos += 4;
                buf[pos..pos + data.len()].copy_from_slice(data);
                pos += data.len();
            }
            None => {
                put_i32(buf, pos, -1); // NULL
                pos += 4;
            }
        }
    }

    // result column format codes
    put_i16(buf, pos, result_formats.len() as i16);
    pos += 2;
    for &f in result_formats {
        put_i16(buf, pos, f);
        pos += 2;
    }

    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode a Describe message ('D').
pub fn encode_describe(buf: &mut [u8], target: DescribeTarget, name: &str) -> usize {
    let mut pos = 0;
    buf[pos] = b'D';
    pos += 1;

    let len_pos = pos;
    pos += 4;

    buf[pos] = match target {
        DescribeTarget::Statement => b'S',
        DescribeTarget::Portal => b'P',
    };
    pos += 1;

    pos += put_cstring(buf, pos, name);

    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode an Execute message ('E').
pub fn encode_execute(buf: &mut [u8], portal: &str, max_rows: i32) -> usize {
    let mut pos = 0;
    buf[pos] = b'E';
    pos += 1;

    let len_pos = pos;
    pos += 4;

    pos += put_cstring(buf, pos, portal);
    put_i32(buf, pos, max_rows);
    pos += 4;

    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode a Sync message ('S').
pub fn encode_sync(buf: &mut [u8]) -> usize {
    buf[0] = b'S';
    put_i32(buf, 1, 4);
    5
}

/// Encode a Flush message ('H').
pub fn encode_flush(buf: &mut [u8]) -> usize {
    buf[0] = b'H';
    put_i32(buf, 1, 4);
    5
}

/// Encode a Terminate message ('X').
pub fn encode_terminate(buf: &mut [u8]) -> usize {
    buf[0] = b'X';
    put_i32(buf, 1, 4);
    5
}

/// Encode a Close message ('C').
pub fn encode_close(buf: &mut [u8], target: CloseTarget, name: &str) -> usize {
    let mut pos = 0;
    buf[pos] = b'C';
    pos += 1;

    let len_pos = pos;
    pos += 4;

    buf[pos] = match target {
        CloseTarget::Statement => b'S',
        CloseTarget::Portal => b'P',
    };
    pos += 1;
    pos += put_cstring(buf, pos, name);

    put_i32(buf, len_pos, (pos - len_pos) as i32);
    pos
}

/// Encode a CopyData message ('d').
pub fn encode_copy_data(buf: &mut [u8], data: &[u8]) -> usize {
    let mut pos = 0;
    buf[pos] = b'd';
    pos += 1;
    let len = 4 + data.len();
    put_i32(buf, pos, len as i32);
    pos += 4;
    buf[pos..pos + data.len()].copy_from_slice(data);
    pos += data.len();
    pos
}

/// Encode a CopyDone message ('c').
pub fn encode_copy_done(buf: &mut [u8]) -> usize {
    buf[0] = b'c';
    put_i32(buf, 1, 4);
    5
}

/// Encode a CopyFail message ('f').
///
/// Sent by the frontend to abort a COPY FROM STDIN operation.
/// The `reason` string is included in the server's error response.
pub fn encode_copy_fail(buf: &mut [u8], reason: &str) -> usize {
    let mut pos = 0;
    buf[pos] = b'f';
    pos += 1;
    let len = 4 + reason.len() + 1;
    put_i32(buf, pos, len as i32);
    pos += 4;
    pos += put_cstring(buf, pos, reason);
    pos
}

// ─── Decoding (Server → Frontend) ─────────────────────────────

/// A decoded backend message header.
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub tag: BackendTag,
    pub length: u32, // length including 4-byte self but excluding tag byte
}

/// Try to read a complete message header from `buf`.
/// Returns None if not enough data is available.
pub fn decode_header(buf: &[u8]) -> Option<MessageHeader> {
    if buf.len() < 5 {
        return None;
    }
    let tag = BackendTag::from(buf[0]);
    let length = read_u32(buf, 1);
    Some(MessageHeader { tag, length })
}

/// Check if a complete message is available in `buf`.
///
/// Returns:
/// - `Ok(Some(n))` — a complete message of `n` bytes is ready.
/// - `Ok(None)` — not enough data yet; caller should read more.
/// - `Err(PgError::BufferOverflow)` — the message length field exceeds
///   `MAX_MESSAGE_SIZE`; the connection must be closed.
pub fn message_complete(buf: &[u8]) -> Result<Option<usize>, PgError> {
    if buf.len() < 5 {
        return Ok(None);
    }
    let length = read_u32(buf, 1) as usize;
    if length > MAX_MESSAGE_SIZE {
        return Err(PgError::BufferOverflow);
    }
    let total = 1 + length; // tag + length-included body
    if buf.len() >= total {
        Ok(Some(total))
    } else {
        Ok(None)
    }
}

/// Read an i32 from a backend message body.
pub fn read_i32(buf: &[u8], offset: usize) -> i32 {
    i32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Read a u32 from a backend message body.
pub fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Read an i16 from a backend message body.
pub fn read_i16(buf: &[u8], offset: usize) -> i16 {
    i16::from_be_bytes([buf[offset], buf[offset + 1]])
}

/// Read a C-string from `buf[offset..]`. Returns the string slice and bytes consumed (including null).
pub fn read_cstring(buf: &[u8], offset: usize) -> (&str, usize) {
    let start = offset;
    let mut end = start;
    while end < buf.len() && buf[end] != 0 {
        end += 1;
    }
    let s = std::str::from_utf8(&buf[start..end]).unwrap_or("");
    (s, end - start + 1) // +1 for null terminator
}

/// Parse an ErrorResponse or NoticeResponse message body.
/// Returns a list of (field_type, value) pairs.
pub fn parse_error_fields(body: &[u8]) -> Vec<(u8, String)> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while pos < body.len() {
        let field_type = body[pos];
        pos += 1;
        if field_type == 0 {
            break;
        }
        let (value, consumed) = read_cstring(body, pos);
        fields.push((field_type, value.to_string()));
        pos += consumed;
    }
    fields
}

/// Parse a RowDescription message body.
/// Returns column descriptors: (name, table_oid, col_attr, type_oid, type_size, type_modifier, format_code)
pub fn parse_row_description(body: &[u8]) -> Vec<ColumnDesc> {
    let num_fields = read_i16(body, 0) as usize;
    let mut columns = Vec::with_capacity(num_fields);
    let mut pos = 2;

    for _ in 0..num_fields {
        let (name, consumed) = read_cstring(body, pos);
        pos += consumed;

        let table_oid = read_i32(body, pos) as u32;
        pos += 4;
        let col_attr = read_i16(body, pos);
        pos += 2;
        let type_oid = read_i32(body, pos) as u32;
        pos += 4;
        let type_size = read_i16(body, pos);
        pos += 2;
        let type_modifier = read_i32(body, pos);
        pos += 4;
        let format_code = FormatCode::from(read_i16(body, pos));
        pos += 2;

        columns.push(ColumnDesc {
            name: name.to_string(),
            table_oid,
            col_attr,
            type_oid,
            type_size,
            type_modifier,
            format_code,
        });
    }
    columns
}

/// Parse a DataRow message body. Returns column byte slices.
/// Each column is Option<&[u8]> where None = SQL NULL.
pub fn parse_data_row(body: &[u8]) -> Vec<Option<&[u8]>> {
    let num_columns = read_i16(body, 0) as usize;
    let mut columns = Vec::with_capacity(num_columns);
    let mut pos = 2;

    for _ in 0..num_columns {
        let len = read_i32(body, pos);
        pos += 4;
        if len < 0 {
            columns.push(None); // NULL
        } else {
            let len = len as usize;
            columns.push(Some(&body[pos..pos + len]));
            pos += len;
        }
    }
    columns
}

/// A column descriptor from RowDescription.
#[derive(Debug, Clone)]
pub struct ColumnDesc {
    pub name: String,
    pub table_oid: u32,
    pub col_attr: i16,
    pub type_oid: u32,
    pub type_size: i16,
    pub type_modifier: i32,
    pub format_code: FormatCode,
}

// ─── Helper Functions ──────────────────────────────────────────

fn put_i32(buf: &mut [u8], offset: usize, value: i32) {
    let bytes = value.to_be_bytes();
    buf[offset..offset + 4].copy_from_slice(&bytes);
}

fn put_i16(buf: &mut [u8], offset: usize, value: i16) {
    let bytes = value.to_be_bytes();
    buf[offset..offset + 2].copy_from_slice(&bytes);
}

fn put_cstring(buf: &mut [u8], offset: usize, s: &str) -> usize {
    let bytes = s.as_bytes();
    buf[offset..offset + bytes.len()].copy_from_slice(bytes);
    buf[offset + bytes.len()] = 0;
    bytes.len() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PgError;
    use crate::protocol::{BackendTag, CloseTarget, DescribeTarget, FormatCode};

    #[test]
    fn test_startup_encoding() {
        let mut buf = [0u8; 256];
        let n = encode_startup(&mut buf, "postgres", "mydb", &[]);
        assert!(n > 0);
        // Check protocol version at offset 4
        assert_eq!(read_i32(&buf, 4), 196608);
    }

    #[test]
    fn test_query_encoding() {
        let mut buf = [0u8; 256];
        let n = encode_query(&mut buf, "SELECT 1");
        assert_eq!(buf[0], b'Q');
        assert!(n > 5);
    }

    #[test]
    fn test_sync_encoding() {
        let mut buf = [0u8; 8];
        let n = encode_sync(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(buf[0], b'S');
    }

    #[test]
    fn test_message_complete() {
        // tag(1) + length(4) = 5 bytes minimum
        let msg = [b'Z', 0, 0, 0, 5, b'I'];
        assert_eq!(message_complete(&msg).unwrap(), Some(6));
        assert_eq!(message_complete(&msg[..4]).unwrap(), None); // incomplete
    }

    #[test]
    fn test_message_complete_rejects_oversized() {
        // Length field = MAX_MESSAGE_SIZE + 1 → should return Err(BufferOverflow)
        let huge_len = (MAX_MESSAGE_SIZE + 1) as u32;
        let mut msg = [0u8; 6];
        msg[0] = b'D';
        msg[1..5].copy_from_slice(&huge_len.to_be_bytes());
        assert!(matches!(
            message_complete(&msg),
            Err(PgError::BufferOverflow)
        ));
    }

    #[test]
    fn test_copy_fail_encoding() {
        let mut buf = [0u8; 256];
        let n = encode_copy_fail(&mut buf, "abort test");
        assert_eq!(buf[0], b'f');
        // length = 4 + len("abort test") + 1 = 15
        assert_eq!(read_i32(&buf, 1), 15);
        assert_eq!(n, 1 + 15); // tag + length-included body
    }

    // ─── Extended Query Protocol Encoding ────────────────────────────────────

    #[test]
    fn test_parse_encoding_tag_and_name() {
        let mut buf = [0u8; 256];
        let n = encode_parse(&mut buf, "s0", "SELECT $1", &[23]); // 23 = INT4 OID
        assert!(n > 0);
        assert_eq!(buf[0], b'P');
        // cstring "s0\0SELECT $1\0" + i16(1) + i32(23)
        // length = 4 + 3 + 10 + 2 + 4 = 23
        let length = read_i32(&buf, 1);
        assert_eq!(length as usize, n - 1); // length field excludes the tag byte
        assert!(n < 256, "shouldn't exceed buffer");
    }

    #[test]
    fn test_parse_encoding_anonymous_no_oids() {
        let mut buf = [0u8; 128];
        let n = encode_parse(&mut buf, "", "SELECT 1", &[]);
        assert_eq!(buf[0], b'P');
        assert!(n > 5);
    }

    #[test]
    fn test_bind_encoding_no_params() {
        let mut buf = [0u8; 256];
        let n = encode_bind(&mut buf, "", "s0", &[], &[], &[1]); // binary results
        assert_eq!(buf[0], b'B');
        let length = read_i32(&buf, 1) as usize;
        assert_eq!(length, n - 1);
        // result format count should be 1
        // find it: after portal(""\0) + stmt("s0\0") + param_format_count(i16) + param_count(i16)
        // = 1 + 3 + 2 + 2 = 8 bytes after tag+length
        let result_format_count = read_i16(&buf, 1 + 4 + 1 + 3 + 2 + 2);
        assert_eq!(result_format_count, 1);
    }

    #[test]
    fn test_bind_encoding_null_param() {
        let mut buf = [0u8; 256];
        let n = encode_bind(&mut buf, "", "s0", &[0], &[None], &[]);
        assert_eq!(buf[0], b'B');
        assert!(n > 5);
        // The NULL param should encode -1 as i32
        // Layout: tag(1) + len(4) + portal(""\0=1) + stmt("s0\0"=3)
        //       + param_fmt_cnt(2) + 1 fmt_code(2) + param_val_cnt(2) = 15
        let null_marker = read_i32(&buf, 15);
        assert_eq!(null_marker, -1, "NULL param must encode as -1");
    }

    #[test]
    fn test_bind_encoding_with_text_param() {
        let mut buf = [0u8; 256];
        let value = b"hello";
        let n = encode_bind(&mut buf, "", "s0", &[0], &[Some(value)], &[]);
        assert_eq!(buf[0], b'B');
        assert!(n > 5);
    }

    #[test]
    fn test_execute_encoding() {
        let mut buf = [0u8; 64];
        let n = encode_execute(&mut buf, "", 0); // unlimited rows
        assert_eq!(buf[0], b'E');
        let length = read_i32(&buf, 1) as usize;
        assert_eq!(length, n - 1);
        // max_rows = 0 is at the end
        let max_rows = read_i32(&buf, n - 4);
        assert_eq!(max_rows, 0);
    }

    #[test]
    fn test_execute_encoding_with_max_rows() {
        let mut buf = [0u8; 64];
        let n = encode_execute(&mut buf, "", 100);
        assert_eq!(buf[0], b'E');
        let max_rows = read_i32(&buf, n - 4);
        assert_eq!(max_rows, 100);
    }

    #[test]
    fn test_describe_statement_encoding() {
        let mut buf = [0u8; 64];
        let n = encode_describe(&mut buf, DescribeTarget::Statement, "s0");
        assert_eq!(buf[0], b'D');
        let length = read_i32(&buf, 1) as usize;
        assert_eq!(length, n - 1);
        // Target byte: 'S' for Statement
        assert_eq!(buf[5], b'S');
        // Statement name 's0\0' starts at offset 6
        assert_eq!(&buf[6..9], b"s0\0");
    }

    #[test]
    fn test_describe_portal_encoding() {
        let mut buf = [0u8; 64];
        let n = encode_describe(&mut buf, DescribeTarget::Portal, "myportal");
        assert_eq!(buf[0], b'D');
        assert_eq!(buf[5], b'P');
        assert!(n > 5);
    }

    #[test]
    fn test_close_statement_encoding() {
        let mut buf = [0u8; 64];
        let n = encode_close(&mut buf, CloseTarget::Statement, "s7");
        assert_eq!(buf[0], b'C');
        let length = read_i32(&buf, 1) as usize;
        assert_eq!(length, n - 1);
        assert_eq!(buf[5], b'S');
        assert_eq!(&buf[6..9], b"s7\0");
    }

    #[test]
    fn test_close_portal_encoding() {
        let mut buf = [0u8; 64];
        let n = encode_close(&mut buf, CloseTarget::Portal, "");
        assert_eq!(buf[0], b'C');
        assert_eq!(buf[5], b'P');
        assert!(n > 0);
    }

    #[test]
    fn test_terminate_encoding() {
        let mut buf = [0u8; 8];
        let n = encode_terminate(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(buf[0], b'X');
        assert_eq!(read_i32(&buf, 1), 4);
    }

    #[test]
    fn test_flush_encoding() {
        let mut buf = [0u8; 8];
        let n = encode_flush(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(buf[0], b'H');
        assert_eq!(read_i32(&buf, 1), 4);
    }

    #[test]
    fn test_copy_data_encoding() {
        let mut buf = [0u8; 64];
        let data = b"col1\tcol2\n";
        let n = encode_copy_data(&mut buf, data);
        assert_eq!(buf[0], b'd');
        let length = read_i32(&buf, 1) as usize;
        assert_eq!(length, 4 + data.len());
        assert_eq!(n, 1 + length);
        assert_eq!(&buf[5..5 + data.len()], data);
    }

    #[test]
    fn test_copy_done_encoding() {
        let mut buf = [0u8; 8];
        let n = encode_copy_done(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(buf[0], b'c');
        assert_eq!(read_i32(&buf, 1), 4);
    }

    // ─── Decoding ─────────────────────────────────────────────────────────────

    #[test]
    fn test_decode_header_basic() {
        let msg = [b'Z', 0, 0, 0, 5, b'I']; // ReadyForQuery
        let hdr = decode_header(&msg).unwrap();
        assert_eq!(hdr.tag, BackendTag::ReadyForQuery);
        assert_eq!(hdr.length, 5);
    }

    #[test]
    fn test_decode_header_too_short() {
        let msg = [b'Z', 0, 0]; // only 3 bytes
        assert!(decode_header(&msg).is_none());
    }

    #[test]
    fn test_message_complete_exact_size() {
        // tag(1) + len(4) = 5-byte header, body = 1 byte → total = 6
        let msg = [b'Z', 0, 0, 0, 5, b'I'];
        assert_eq!(message_complete(&msg).unwrap(), Some(6));
    }

    #[test]
    fn test_message_complete_one_byte_short() {
        let msg = [b'Z', 0, 0, 0, 5]; // header says 5 bytes body, but we only have 4 (no body)
        assert_eq!(message_complete(&msg).unwrap(), None);
    }

    #[test]
    fn test_message_complete_needs_exactly_5_bytes() {
        // 4 bytes → None
        assert_eq!(message_complete(&[b'Z', 0, 0, 0]).unwrap(), None);
        // 5 bytes with length=4 (empty body) → Some(5)
        let msg = [b'C', 0, 0, 0, 4]; // CommandComplete with no text
        assert_eq!(message_complete(&msg).unwrap(), Some(5));
    }

    #[test]
    fn test_message_complete_large_but_valid_payload() {
        // Build a 10-byte payload message
        let payload = [0u8; 10];
        let mut msg = vec![b'D', 0, 0, 0, 14]; // length = 4 + 10 = 14
        msg.extend_from_slice(&payload);
        assert_eq!(message_complete(&msg).unwrap(), Some(15)); // 1 + 14
    }

    #[test]
    fn test_parse_data_row_all_non_null() {
        // DataRow with 2 columns: "hello" and "42"
        // Format: i16(num_cols) | i32(len1) bytes1 | i32(len2) bytes2
        let mut body = vec![];
        body.extend_from_slice(&2i16.to_be_bytes()); // 2 columns
        body.extend_from_slice(&5i32.to_be_bytes()); // col0 len = 5
        body.extend_from_slice(b"hello");
        body.extend_from_slice(&2i32.to_be_bytes()); // col1 len = 2
        body.extend_from_slice(b"42");
        let cols = parse_data_row(&body);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0], Some(b"hello" as &[u8]));
        assert_eq!(cols[1], Some(b"42" as &[u8]));
    }

    #[test]
    fn test_parse_data_row_with_null() {
        // DataRow with 2 columns: NULL and "value"
        let mut body = vec![];
        body.extend_from_slice(&2i16.to_be_bytes());
        body.extend_from_slice(&(-1i32).to_be_bytes()); // NULL
        body.extend_from_slice(&5i32.to_be_bytes());
        body.extend_from_slice(b"value");
        let cols = parse_data_row(&body);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0], None);
        assert_eq!(cols[1], Some(b"value" as &[u8]));
    }

    #[test]
    fn test_parse_data_row_empty_row() {
        let mut body = vec![];
        body.extend_from_slice(&0i16.to_be_bytes()); // 0 columns
        let cols = parse_data_row(&body);
        assert_eq!(cols.len(), 0);
    }

    #[test]
    fn test_parse_row_description_single_column() {
        // Build a RowDescription body for 1 column "id" INT4 (OID=23, size=4)
        let mut body = vec![];
        body.extend_from_slice(&1i16.to_be_bytes()); // num_fields = 1
        body.extend_from_slice(b"id\0"); // name + null terminator
        body.extend_from_slice(&0i32.to_be_bytes()); // table_oid = 0
        body.extend_from_slice(&0i16.to_be_bytes()); // col_attr = 0
        body.extend_from_slice(&23i32.to_be_bytes()); // type_oid = INT4
        body.extend_from_slice(&4i16.to_be_bytes()); // type_size = 4
        body.extend_from_slice(&(-1i32).to_be_bytes()); // type_modifier = -1
        body.extend_from_slice(&0i16.to_be_bytes()); // format_code = text
        let cols = parse_row_description(&body);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "id");
        assert_eq!(cols[0].type_oid, 23);
        assert_eq!(cols[0].type_size, 4);
        assert!(matches!(cols[0].format_code, FormatCode::Text));
    }

    #[test]
    fn test_parse_row_description_binary_format() {
        let mut body = vec![];
        body.extend_from_slice(&1i16.to_be_bytes());
        body.extend_from_slice(b"score\0");
        body.extend_from_slice(&0i32.to_be_bytes());
        body.extend_from_slice(&0i16.to_be_bytes());
        body.extend_from_slice(&701i32.to_be_bytes()); // FLOAT8 OID
        body.extend_from_slice(&8i16.to_be_bytes());
        body.extend_from_slice(&(-1i32).to_be_bytes());
        body.extend_from_slice(&1i16.to_be_bytes()); // format = binary
        let cols = parse_row_description(&body);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "score");
        assert!(matches!(cols[0].format_code, FormatCode::Binary));
    }

    #[test]
    fn test_parse_error_fields_basic() {
        // Severity='S', Code='C', Message='M', terminator='\0'
        let mut body = vec![];
        body.push(b'S');
        body.extend_from_slice(b"ERROR\0");
        body.push(b'C');
        body.extend_from_slice(b"42601\0");
        body.push(b'M');
        body.extend_from_slice(b"syntax error\0");
        body.push(0); // terminator
        let fields = parse_error_fields(&body);
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], (b'S', "ERROR".to_string()));
        assert_eq!(fields[1], (b'C', "42601".to_string()));
        assert_eq!(fields[2], (b'M', "syntax error".to_string()));
    }

    #[test]
    fn test_parse_error_fields_empty() {
        let body = [0u8]; // just the terminator
        let fields = parse_error_fields(&body);
        assert!(fields.is_empty());
    }

    // ─── Helper read functions ────────────────────────────────────────────────

    #[test]
    fn test_read_i32_big_endian() {
        let buf = [0x00, 0x01, 0x86, 0xA0u8]; // 100000
        assert_eq!(read_i32(&buf, 0), 100_000);
    }

    #[test]
    fn test_read_i32_negative() {
        let buf = (-1i32).to_be_bytes();
        assert_eq!(read_i32(&buf, 0), -1);
    }

    #[test]
    fn test_read_i16() {
        let buf = [0x01, 0x00u8]; // 256
        assert_eq!(read_i16(&buf, 0), 256);
    }

    #[test]
    fn test_read_u32() {
        let buf = 0xFF_FF_FF_FFu32.to_be_bytes();
        assert_eq!(read_u32(&buf, 0), 0xFF_FF_FF_FF);
    }

    #[test]
    fn test_read_cstring_normal() {
        let buf = b"hello\0world";
        let (s, consumed) = read_cstring(buf, 0);
        assert_eq!(s, "hello");
        assert_eq!(consumed, 6); // 5 chars + null
    }

    #[test]
    fn test_read_cstring_empty() {
        let buf = b"\0rest";
        let (s, consumed) = read_cstring(buf, 0);
        assert_eq!(s, "");
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_read_cstring_with_offset() {
        let buf = b"skip\0name\0";
        let (s, consumed) = read_cstring(buf, 5);
        assert_eq!(s, "name");
        assert_eq!(consumed, 5); // 4 chars + null
    }
}
