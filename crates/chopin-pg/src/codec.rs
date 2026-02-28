//! Zero-copy binary codec for PostgreSQL v3 wire protocol.
//!
//! All encoding writes directly into a caller-provided buffer.
//! All decoding slices directly from the read buffer.

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
pub fn message_complete(buf: &[u8]) -> Option<usize> {
    if buf.len() < 5 {
        return None;
    }
    let length = read_u32(buf, 1) as usize;
    let total = 1 + length; // tag + length-included body
    if buf.len() >= total {
        Some(total)
    } else {
        None
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
        assert_eq!(message_complete(&msg), Some(6));
        assert_eq!(message_complete(&msg[..4]), None); // incomplete
    }
}
