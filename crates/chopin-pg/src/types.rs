//! PostgreSQL type system — type OIDs, value conversions, and ToSql/FromSql traits.
use crate::error::{PgError, PgResult};

/// Well-known PostgreSQL type OIDs.
pub mod oid {
    pub const BOOL: u32 = 16;
    pub const BYTEA: u32 = 17;
    pub const CHAR: u32 = 18;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const TEXT: u32 = 25;
    pub const OID: u32 = 26;
    pub const FLOAT4: u32 = 700;
    pub const FLOAT8: u32 = 701;
    pub const VARCHAR: u32 = 1043;
    pub const DATE: u32 = 1082;
    pub const TIME: u32 = 1083;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const INTERVAL: u32 = 1186;
    pub const NUMERIC: u32 = 1700;
    pub const UUID: u32 = 2950;
    pub const JSONB: u32 = 3802;
    pub const JSON: u32 = 114;
    pub const INET: u32 = 869;
    pub const CIDR: u32 = 650;
    pub const MACADDR: u32 = 829;
    pub const MACADDR8: u32 = 774;
    pub const POINT: u32 = 600;
    pub const LINE: u32 = 628;
    pub const LSEG: u32 = 601;
    pub const BOX: u32 = 603;
    pub const PATH: u32 = 602;
    pub const POLYGON: u32 = 604;
    pub const CIRCLE: u32 = 718;

    // Array types
    pub const BOOL_ARRAY: u32 = 1000;
    pub const INT2_ARRAY: u32 = 1005;
    pub const INT4_ARRAY: u32 = 1007;
    pub const INT8_ARRAY: u32 = 1016;
    pub const TEXT_ARRAY: u32 = 1009;
    pub const FLOAT4_ARRAY: u32 = 1021;
    pub const FLOAT8_ARRAY: u32 = 1022;
    pub const VARCHAR_ARRAY: u32 = 1015;

    // Range types
    pub const INT4RANGE: u32 = 3904;
    pub const INT8RANGE: u32 = 3926;
    pub const NUMRANGE: u32 = 3906;
    pub const TSRANGE: u32 = 3908;
    pub const TSTZRANGE: u32 = 3910;
    pub const DATERANGE: u32 = 3912;
}

/// A PostgreSQL value that can be used as a query parameter or read from a row.
#[derive(Debug, Clone, PartialEq)]
pub enum PgValue {
    Null,
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    Float4(f32),
    Float8(f64),
    Text(String),
    Bytes(Vec<u8>),
    Json(String),
    Jsonb(Vec<u8>),
    /// UUID stored as 16-byte array.
    Uuid([u8; 16]),
    /// Date: days since 2000-01-01 (PostgreSQL epoch).
    Date(i32),
    /// Time: microseconds since midnight.
    Time(i64),
    /// Timestamp: microseconds since 2000-01-01 00:00:00 (PostgreSQL epoch).
    Timestamp(i64),
    /// Timestamptz: microseconds since 2000-01-01 00:00:00 UTC.
    Timestamptz(i64),
    /// Interval: months, days, microseconds.
    Interval { months: i32, days: i32, microseconds: i64 },
    /// Network address (stored as text representation).
    Inet(String),
    /// Numeric (stored as text representation for lossless precision).
    Numeric(String),
    /// MAC address stored as 6 bytes.
    MacAddr([u8; 6]),
    /// 2D point: (x, y).
    Point { x: f64, y: f64 },
    /// Range value (stored as text representation).
    /// Examples: `"[1,10)"`, `"[2024-01-01,2024-12-31]"`, `"empty"`.
    Range(String),
    /// Array of values (homogeneous).
    Array(Vec<PgValue>),
}

impl PgValue {
    /// Encode this value as text-format bytes for use as a query parameter.
    pub fn to_text_bytes(&self) -> Option<Vec<u8>> {
        match self {
            PgValue::Null => None,
            PgValue::Bool(b) => Some(if *b { b"t".to_vec() } else { b"f".to_vec() }),
            PgValue::Int2(v) => Some(v.to_string().into_bytes()),
            PgValue::Int4(v) => Some(v.to_string().into_bytes()),
            PgValue::Int8(v) => Some(v.to_string().into_bytes()),
            PgValue::Float4(v) => Some(v.to_string().into_bytes()),
            PgValue::Float8(v) => Some(v.to_string().into_bytes()),
            PgValue::Text(s) => Some(s.as_bytes().to_vec()),
            PgValue::Bytes(b) => Some(b.clone()),
            PgValue::Json(s) => Some(s.as_bytes().to_vec()),
            PgValue::Jsonb(b) => Some(b.clone()),
            PgValue::Uuid(bytes) => Some(format_uuid(bytes).into_bytes()),
            PgValue::Date(days) => Some(format_date(*days).into_bytes()),
            PgValue::Time(us) => Some(format_time(*us).into_bytes()),
            PgValue::Timestamp(us) => Some(format_timestamp(*us).into_bytes()),
            PgValue::Timestamptz(us) => Some(format_timestamp_tz(*us).into_bytes()),
            PgValue::Interval { months, days, microseconds } => {
                Some(format_interval(*months, *days, *microseconds).into_bytes())
            }
            PgValue::Inet(s) => Some(s.as_bytes().to_vec()),
            PgValue::Numeric(s) => Some(s.as_bytes().to_vec()),
            PgValue::MacAddr(bytes) => {
                Some(format!(
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                ).into_bytes())
            }
            PgValue::Point { x, y } => Some(format!("({},{})", x, y).into_bytes()),
            PgValue::Range(s) => Some(s.as_bytes().to_vec()),
            PgValue::Array(values) => {
                let inner: Vec<String> = values
                    .iter()
                    .map(|v| match v {
                        PgValue::Null => "NULL".to_string(),
                        _ => match v.to_text_bytes() {
                            Some(b) => {
                                let s = String::from_utf8_lossy(&b).to_string();
                                escape_array_element(&s)
                            }
                            None => "NULL".to_string(),
                        },
                    })
                    .collect();
                Some(format!("{{{}}}", inner.join(",")).into_bytes())
            }
        }
    }

    /// Encode this value as binary-format bytes for use as a query parameter.
    ///
    /// Returns `None` for Null; `Some(bytes)` for everything else.
    /// The binary format matches what PostgreSQL expects when the
    /// parameter format code is 1 (binary).
    pub fn to_binary_bytes(&self) -> Option<Vec<u8>> {
        match self {
            PgValue::Null => None,
            PgValue::Bool(b) => Some(vec![if *b { 1 } else { 0 }]),
            PgValue::Int2(v) => Some(v.to_be_bytes().to_vec()),
            PgValue::Int4(v) => Some(v.to_be_bytes().to_vec()),
            PgValue::Int8(v) => Some(v.to_be_bytes().to_vec()),
            PgValue::Float4(v) => Some(v.to_be_bytes().to_vec()),
            PgValue::Float8(v) => Some(v.to_be_bytes().to_vec()),
            PgValue::Text(s) => Some(s.as_bytes().to_vec()),
            PgValue::Bytes(b) => Some(b.clone()),
            PgValue::Json(s) => Some(s.as_bytes().to_vec()),
            PgValue::Jsonb(b) => {
                // Prefix with version byte (1) for binary JSONB
                let mut buf = Vec::with_capacity(1 + b.len());
                buf.push(1);
                buf.extend_from_slice(b);
                Some(buf)
            }
            PgValue::Uuid(bytes) => Some(bytes.to_vec()),
            PgValue::Date(days) => Some(days.to_be_bytes().to_vec()),
            PgValue::Time(us) => Some(us.to_be_bytes().to_vec()),
            PgValue::Timestamp(us) | PgValue::Timestamptz(us) => Some(us.to_be_bytes().to_vec()),
            PgValue::Interval { months, days, microseconds } => {
                let mut buf = Vec::with_capacity(16);
                buf.extend_from_slice(&microseconds.to_be_bytes());
                buf.extend_from_slice(&days.to_be_bytes());
                buf.extend_from_slice(&months.to_be_bytes());
                Some(buf)
            }
            PgValue::Inet(s) => encode_inet_binary(s).ok(),
            PgValue::Numeric(s) => {
                // NUMERIC is complex in binary — fall back to text encoding
                Some(s.as_bytes().to_vec())
            }
            PgValue::MacAddr(bytes) => Some(bytes.to_vec()),
            PgValue::Point { x, y } => {
                let mut buf = Vec::with_capacity(16);
                buf.extend_from_slice(&x.to_be_bytes());
                buf.extend_from_slice(&y.to_be_bytes());
                Some(buf)
            }
            PgValue::Range(s) => {
                // Range binary encoding is complex — use text
                Some(s.as_bytes().to_vec())
            }
            PgValue::Array(values) => {
                // Use text array format for encoding — binary array encoding
                // requires knowing the element OID which PgValue doesn't carry.
                let inner: Vec<String> = values
                    .iter()
                    .map(|v| match v {
                        PgValue::Null => "NULL".to_string(),
                        _ => match v.to_text_bytes() {
                            Some(b) => {
                                let s = String::from_utf8_lossy(&b).to_string();
                                escape_array_element(&s)
                            }
                            None => "NULL".to_string(),
                        },
                    })
                    .collect();
                Some(format!("{{{}}}", inner.join(",")).into_bytes())
            }
        }
    }

    /// Determine if this value should be sent as binary or text format.
    ///
    /// Returns `true` for types that have an efficient binary encoding
    /// (scalars, dates, etc.), `false` for types best sent as text
    /// (arrays, numeric, inet).
    pub fn prefers_binary(&self) -> bool {
        matches!(
            self,
            PgValue::Bool(_)
                | PgValue::Int2(_)
                | PgValue::Int4(_)
                | PgValue::Int8(_)
                | PgValue::Float4(_)
                | PgValue::Float8(_)
                | PgValue::Bytes(_)
                | PgValue::Uuid(_)
                | PgValue::Date(_)
                | PgValue::Time(_)
                | PgValue::Timestamp(_)
                | PgValue::Timestamptz(_)
                | PgValue::Interval { .. }
                | PgValue::Jsonb(_)
                | PgValue::MacAddr(_)
                | PgValue::Point { .. }
        )
    }

    /// Parse a text-format column value based on its type OID.
    pub fn from_text(type_oid: u32, data: &[u8]) -> PgResult<Self> {
        let s = std::str::from_utf8(data)
            .map_err(|_| PgError::TypeConversion("Invalid UTF-8".to_string()))?;
        match type_oid {
            oid::BOOL => Ok(PgValue::Bool(s == "t" || s == "true" || s == "1")),
            oid::INT2 => {
                Ok(PgValue::Int2(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid INT2".to_string())
                })?))
            }
            oid::INT4 | oid::OID => {
                Ok(PgValue::Int4(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid INT4/OID".to_string())
                })?))
            }
            oid::INT8 => {
                Ok(PgValue::Int8(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid INT8".to_string())
                })?))
            }
            oid::FLOAT4 => {
                Ok(PgValue::Float4(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid FLOAT4".to_string())
                })?))
            }
            oid::FLOAT8 => {
                Ok(PgValue::Float8(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid FLOAT8".to_string())
                })?))
            }
            oid::NUMERIC => Ok(PgValue::Numeric(s.to_string())),
            oid::JSONB => Ok(PgValue::Jsonb(data.to_vec())),
            oid::JSON => Ok(PgValue::Json(s.to_string())),
            oid::BYTEA => Ok(PgValue::Bytes(decode_bytea_hex(s))),
            oid::UUID => Ok(PgValue::Uuid(parse_uuid_text(s)?)),
            oid::DATE => Ok(PgValue::Date(parse_date_text(s)?)),
            oid::TIME => Ok(PgValue::Time(parse_time_text(s)?)),
            oid::TIMESTAMP => Ok(PgValue::Timestamp(parse_timestamp_text(s)?)),
            oid::TIMESTAMPTZ => Ok(PgValue::Timestamptz(parse_timestamp_text(s)?)),
            oid::INTERVAL => {
                let (months, days, us) = parse_interval_text(s)?;
                Ok(PgValue::Interval { months, days, microseconds: us })
            }
            oid::INET | oid::CIDR => Ok(PgValue::Inet(s.to_string())),
            oid::MACADDR => {
                // Parse "xx:xx:xx:xx:xx:xx" text format
                let bytes = parse_macaddr_text(s)?;
                Ok(PgValue::MacAddr(bytes))
            }
            oid::POINT => {
                // Parse "(x,y)" text format
                let (x, y) = parse_point_text(s)?;
                Ok(PgValue::Point { x, y })
            }
            oid::INT4RANGE | oid::INT8RANGE | oid::NUMRANGE
            | oid::TSRANGE | oid::TSTZRANGE | oid::DATERANGE => {
                Ok(PgValue::Range(s.to_string()))
            }
            _ => Ok(PgValue::Text(s.to_string())),
        }
    }

    /// Parse a binary-format column value based on its type OID.
    pub fn from_binary(type_oid: u32, data: &[u8]) -> PgResult<Self> {
        match type_oid {
            oid::BOOL => Ok(PgValue::Bool(data.first().is_some_and(|&b| b != 0))),
            oid::INT2 if data.len() >= 2 => {
                Ok(PgValue::Int2(i16::from_be_bytes([data[0], data[1]])))
            }
            oid::INT4 | oid::OID if data.len() >= 4 => Ok(PgValue::Int4(i32::from_be_bytes([
                data[0], data[1], data[2], data[3],
            ]))),
            oid::INT8 if data.len() >= 8 => Ok(PgValue::Int8(i64::from_be_bytes([
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            ]))),
            oid::FLOAT4 if data.len() >= 4 => Ok(PgValue::Float4(f32::from_be_bytes([
                data[0], data[1], data[2], data[3],
            ]))),
            oid::FLOAT8 if data.len() >= 8 => Ok(PgValue::Float8(f64::from_be_bytes([
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            ]))),
            oid::UUID if data.len() >= 16 => {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&data[..16]);
                Ok(PgValue::Uuid(bytes))
            }
            oid::DATE if data.len() >= 4 => {
                Ok(PgValue::Date(i32::from_be_bytes([data[0], data[1], data[2], data[3]])))
            }
            oid::TIME if data.len() >= 8 => {
                Ok(PgValue::Time(i64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ])))
            }
            oid::TIMESTAMP | oid::TIMESTAMPTZ if data.len() >= 8 => {
                let us = i64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                if type_oid == oid::TIMESTAMPTZ {
                    Ok(PgValue::Timestamptz(us))
                } else {
                    Ok(PgValue::Timestamp(us))
                }
            }
            oid::INTERVAL if data.len() >= 16 => {
                let microseconds = i64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                let days = i32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                let months = i32::from_be_bytes([data[12], data[13], data[14], data[15]]);
                Ok(PgValue::Interval { months, days, microseconds })
            }
            oid::JSONB => {
                // First byte is version (1), rest is JSON
                if data.len() > 1 {
                    Ok(PgValue::Jsonb(data[1..].to_vec()))
                } else {
                    Ok(PgValue::Jsonb(Vec::new()))
                }
            }
            oid::BYTEA => Ok(PgValue::Bytes(data.to_vec())),
            oid::INET | oid::CIDR => {
                // Binary format: family(1) + mask(1) + is_cidr(1) + addr_len(1) + addr bytes
                if data.len() < 4 {
                    return Err(PgError::TypeConversion("INET/CIDR binary too short".into()));
                }
                let family = data[0];
                let mask = data[1];
                // data[2] = is_cidr flag (0 = INET, 1 = CIDR)
                let addr_len = data[3] as usize;
                if data.len() < 4 + addr_len {
                    return Err(PgError::TypeConversion("INET/CIDR address truncated".into()));
                }
                let addr_bytes = &data[4..4 + addr_len];
                let addr_str = match family {
                    // AF_INET
                    2 if addr_len == 4 => {
                        format!("{}.{}.{}.{}", addr_bytes[0], addr_bytes[1], addr_bytes[2], addr_bytes[3])
                    }
                    // AF_INET6
                    3 if addr_len == 16 => {
                        format_ipv6(addr_bytes)
                    }
                    _ => {
                        return Err(PgError::TypeConversion(format!(
                            "Unknown INET family: {}", family
                        )));
                    }
                };
                // Include mask for CIDR or non-default masks
                let default_mask = if family == 2 { 32 } else { 128 };
                if mask != default_mask || type_oid == oid::CIDR {
                    Ok(PgValue::Inet(format!("{}/{}", addr_str, mask)))
                } else {
                    Ok(PgValue::Inet(addr_str))
                }
            }
            oid::NUMERIC => {
                // Binary NUMERIC format:
                //   ndigits(u16) + weight(i16) + sign(u16) + dscale(u16) + digits(u16 * ndigits)
                // Each digit is a base-10000 value.
                if data.len() < 8 {
                    return Err(PgError::TypeConversion("NUMERIC binary too short".into()));
                }
                let ndigits = u16::from_be_bytes([data[0], data[1]]) as usize;
                let weight = i16::from_be_bytes([data[2], data[3]]);
                let sign = u16::from_be_bytes([data[4], data[5]]);
                let dscale = u16::from_be_bytes([data[6], data[7]]) as usize;

                if data.len() < 8 + ndigits * 2 {
                    return Err(PgError::TypeConversion("NUMERIC binary truncated".into()));
                }

                let mut digits = Vec::with_capacity(ndigits);
                for i in 0..ndigits {
                    let off = 8 + i * 2;
                    digits.push(u16::from_be_bytes([data[off], data[off + 1]]));
                }

                Ok(PgValue::Numeric(format_numeric_binary(weight, sign, dscale, &digits)))
            }
            oid::MACADDR if data.len() >= 6 => {
                let mut bytes = [0u8; 6];
                bytes.copy_from_slice(&data[..6]);
                Ok(PgValue::MacAddr(bytes))
            }
            oid::POINT if data.len() >= 16 => {
                let x = f64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                let y = f64::from_be_bytes([
                    data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
                ]);
                Ok(PgValue::Point { x, y })
            }
            oid::BOOL_ARRAY | oid::INT2_ARRAY | oid::INT4_ARRAY | oid::INT8_ARRAY
            | oid::FLOAT4_ARRAY | oid::FLOAT8_ARRAY | oid::TEXT_ARRAY | oid::VARCHAR_ARRAY => {
                parse_binary_array(data)
            }
            _ => {
                // Fallback: treat as text
                Ok(PgValue::Text(String::from_utf8_lossy(data).to_string()))
            }
        }
    }

    /// Try to extract as i32.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            PgValue::Int4(v) => Some(*v),
            PgValue::Int2(v) => Some(*v as i32),
            _ => None,
        }
    }

    /// Try to extract as i64.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PgValue::Int8(v) => Some(*v),
            PgValue::Int4(v) => Some(*v as i64),
            PgValue::Int2(v) => Some(*v as i64),
            _ => None,
        }
    }

    /// Try to extract as &str.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PgValue::Text(s) => Some(s),
            PgValue::Json(s) => Some(s),
            PgValue::Inet(s) => Some(s),
            PgValue::Numeric(s) => Some(s),
            _ => None,
        }
    }

    /// Try to extract as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PgValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to extract as f64.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PgValue::Float8(v) => Some(*v),
            PgValue::Float4(v) => Some(*v as f64),
            PgValue::Int4(v) => Some(*v as f64),
            PgValue::Int8(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Returns true if this is a Null value.
    pub fn is_null(&self) -> bool {
        matches!(self, PgValue::Null)
    }
}

// ─── ToSql / FromSql Traits ──────────────────────────────────

/// Trait for converting Rust types to PostgreSQL parameter values.
/// Replaces the older `ToParam` — provides the same functionality with
/// a more standard name and the ability to specify the OID.
pub trait ToSql {
    /// Convert this value to a PgValue for use as a query parameter.
    fn to_sql(&self) -> PgValue;

    /// The PostgreSQL type OID this value maps to (0 = let the server decide).
    fn type_oid(&self) -> u32 {
        0
    }
}

/// Trait for converting PostgreSQL values to Rust types.
pub trait FromSql: Sized {
    /// Convert a PgValue to this Rust type.
    fn from_sql(value: &PgValue) -> PgResult<Self>;
}

// ─── ToSql Implementations ───────────────────────────────────

impl ToSql for i16 {
    fn to_sql(&self) -> PgValue { PgValue::Int2(*self) }
    fn type_oid(&self) -> u32 { oid::INT2 }
}

impl ToSql for i32 {
    fn to_sql(&self) -> PgValue { PgValue::Int4(*self) }
    fn type_oid(&self) -> u32 { oid::INT4 }
}

impl ToSql for i64 {
    fn to_sql(&self) -> PgValue { PgValue::Int8(*self) }
    fn type_oid(&self) -> u32 { oid::INT8 }
}

impl ToSql for f32 {
    fn to_sql(&self) -> PgValue { PgValue::Float4(*self) }
    fn type_oid(&self) -> u32 { oid::FLOAT4 }
}

impl ToSql for f64 {
    fn to_sql(&self) -> PgValue { PgValue::Float8(*self) }
    fn type_oid(&self) -> u32 { oid::FLOAT8 }
}

impl ToSql for bool {
    fn to_sql(&self) -> PgValue { PgValue::Bool(*self) }
    fn type_oid(&self) -> u32 { oid::BOOL }
}

impl ToSql for &str {
    fn to_sql(&self) -> PgValue { PgValue::Text(self.to_string()) }
    fn type_oid(&self) -> u32 { oid::TEXT }
}

impl ToSql for String {
    fn to_sql(&self) -> PgValue { PgValue::Text(self.clone()) }
    fn type_oid(&self) -> u32 { oid::TEXT }
}

impl ToSql for &[u8] {
    fn to_sql(&self) -> PgValue { PgValue::Bytes(self.to_vec()) }
    fn type_oid(&self) -> u32 { oid::BYTEA }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> PgValue { PgValue::Bytes(self.clone()) }
    fn type_oid(&self) -> u32 { oid::BYTEA }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> PgValue {
        match self {
            Some(v) => v.to_sql(),
            None => PgValue::Null,
        }
    }
}

impl ToSql for PgValue {
    fn to_sql(&self) -> PgValue {
        self.clone()
    }
}

// ─── Array ToSql Implementations ──────────────────────────────

impl ToSql for Vec<i16> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT2_ARRAY }
}

impl ToSql for Vec<i32> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT4_ARRAY }
}

impl ToSql for Vec<i64> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT8_ARRAY }
}

impl ToSql for Vec<f32> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::FLOAT4_ARRAY }
}

impl ToSql for Vec<f64> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::FLOAT8_ARRAY }
}

impl ToSql for Vec<bool> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::BOOL_ARRAY }
}

impl ToSql for Vec<String> {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::TEXT_ARRAY }
}

impl ToSql for &[i16] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT2_ARRAY }
}

impl ToSql for &[i32] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT4_ARRAY }
}

impl ToSql for &[i64] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::INT8_ARRAY }
}

impl ToSql for &[f32] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::FLOAT4_ARRAY }
}

impl ToSql for &[f64] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::FLOAT8_ARRAY }
}

impl ToSql for &[bool] {
    fn to_sql(&self) -> PgValue { PgValue::Array(self.iter().map(|v| v.to_sql()).collect()) }
    fn type_oid(&self) -> u32 { oid::BOOL_ARRAY }
}

// ─── Network Type ToSql Implementations ───────────────────────

impl ToSql for std::net::IpAddr {
    fn to_sql(&self) -> PgValue { PgValue::Inet(self.to_string()) }
    fn type_oid(&self) -> u32 { oid::INET }
}

impl ToSql for std::net::Ipv4Addr {
    fn to_sql(&self) -> PgValue { PgValue::Inet(self.to_string()) }
    fn type_oid(&self) -> u32 { oid::INET }
}

impl ToSql for std::net::Ipv6Addr {
    fn to_sql(&self) -> PgValue { PgValue::Inet(self.to_string()) }
    fn type_oid(&self) -> u32 { oid::INET }
}

// ─── MacAddr / Point ToSql Implementations ────────────────────

impl ToSql for [u8; 6] {
    fn to_sql(&self) -> PgValue { PgValue::MacAddr(*self) }
    fn type_oid(&self) -> u32 { oid::MACADDR }
}

impl ToSql for (f64, f64) {
    fn to_sql(&self) -> PgValue { PgValue::Point { x: self.0, y: self.1 } }
    fn type_oid(&self) -> u32 { oid::POINT }
}

// ─── FromSql Implementations ─────────────────────────────────

impl FromSql for i16 {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Int2(v) => Ok(*v),
            PgValue::Text(s) => s.parse().map_err(|_| PgError::TypeConversion("Not an i16".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to i16".into())),
        }
    }
}

impl FromSql for i32 {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Int4(v) => Ok(*v),
            PgValue::Int2(v) => Ok(*v as i32),
            PgValue::Text(s) => s.parse().map_err(|_| PgError::TypeConversion("Not an i32".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to i32".into())),
        }
    }
}

impl FromSql for i64 {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Int8(v) => Ok(*v),
            PgValue::Int4(v) => Ok(*v as i64),
            PgValue::Int2(v) => Ok(*v as i64),
            PgValue::Text(s) => s.parse().map_err(|_| PgError::TypeConversion("Not an i64".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to i64".into())),
        }
    }
}

impl FromSql for f32 {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Float4(v) => Ok(*v),
            PgValue::Text(s) => s.parse().map_err(|_| PgError::TypeConversion("Not an f32".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to f32".into())),
        }
    }
}

impl FromSql for f64 {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Float8(v) => Ok(*v),
            PgValue::Float4(v) => Ok(*v as f64),
            PgValue::Int4(v) => Ok(*v as f64),
            PgValue::Int8(v) => Ok(*v as f64),
            PgValue::Text(s) => s.parse().map_err(|_| PgError::TypeConversion("Not an f64".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to f64".into())),
        }
    }
}

impl FromSql for bool {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Bool(v) => Ok(*v),
            PgValue::Text(s) => Ok(s == "t" || s == "true" || s == "1"),
            _ => Err(PgError::TypeConversion("Cannot convert to bool".into())),
        }
    }
}

impl FromSql for String {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Text(s) => Ok(s.clone()),
            PgValue::Json(s) => Ok(s.clone()),
            PgValue::Inet(s) => Ok(s.clone()),
            PgValue::Numeric(s) => Ok(s.clone()),
            PgValue::Int2(v) => Ok(v.to_string()),
            PgValue::Int4(v) => Ok(v.to_string()),
            PgValue::Int8(v) => Ok(v.to_string()),
            PgValue::Float4(v) => Ok(v.to_string()),
            PgValue::Float8(v) => Ok(v.to_string()),
            PgValue::Bool(v) => Ok(v.to_string()),
            PgValue::Uuid(b) => Ok(format_uuid(b)),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to String".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to String".into())),
        }
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_sql(value).map(Some)
        }
    }
}

impl FromSql for Vec<u8> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Bytes(b) => Ok(b.clone()),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<u8>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<u8>".into())),
        }
    }
}

impl FromSql for [u8; 16] {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Uuid(b) => Ok(*b),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to [u8; 16]".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to [u8; 16]".into())),
        }
    }
}

// ─── Array FromSql Implementations ────────────────────────────

impl FromSql for Vec<i16> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| i16::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<i16>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<i16>".into())),
        }
    }
}

impl FromSql for Vec<i32> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| i32::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<i32>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<i32>".into())),
        }
    }
}

impl FromSql for Vec<i64> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| i64::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<i64>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<i64>".into())),
        }
    }
}

impl FromSql for Vec<f32> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| f32::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<f32>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<f32>".into())),
        }
    }
}

impl FromSql for Vec<f64> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| f64::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<f64>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<f64>".into())),
        }
    }
}

impl FromSql for Vec<bool> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| bool::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<bool>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<bool>".into())),
        }
    }
}

impl FromSql for Vec<String> {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Array(arr) => arr.iter().map(|v| String::from_sql(v)).collect(),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Vec<String>".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Vec<String>".into())),
        }
    }
}

// ─── Network Type FromSql Implementations ─────────────────────

impl FromSql for std::net::IpAddr {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Inet(s) => {
                let addr_str = s.split('/').next().unwrap_or(s);
                addr_str.parse().map_err(|_| PgError::TypeConversion(format!("Invalid IP address: {}", s)))
            }
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to IpAddr".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to IpAddr".into())),
        }
    }
}

impl FromSql for std::net::Ipv4Addr {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Inet(s) => {
                let addr_str = s.split('/').next().unwrap_or(s);
                addr_str.parse().map_err(|_| PgError::TypeConversion(format!("Invalid IPv4 address: {}", s)))
            }
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Ipv4Addr".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Ipv4Addr".into())),
        }
    }
}

impl FromSql for std::net::Ipv6Addr {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Inet(s) => {
                let addr_str = s.split('/').next().unwrap_or(s);
                addr_str.parse().map_err(|_| PgError::TypeConversion(format!("Invalid IPv6 address: {}", s)))
            }
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to Ipv6Addr".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to Ipv6Addr".into())),
        }
    }
}

// ─── MacAddr / Point FromSql ──────────────────────────────────

impl FromSql for [u8; 6] {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::MacAddr(bytes) => Ok(*bytes),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to [u8; 6]".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to [u8; 6] (MacAddr)".into())),
        }
    }
}

impl FromSql for (f64, f64) {
    fn from_sql(value: &PgValue) -> PgResult<Self> {
        match value {
            PgValue::Point { x, y } => Ok((*x, *y)),
            PgValue::Null => Err(PgError::TypeConversion("Cannot convert NULL to (f64, f64)".into())),
            _ => Err(PgError::TypeConversion("Cannot convert to (f64, f64) (Point)".into())),
        }
    }
}

// ─── Backward Compatibility ──────────────────────────────────

/// Convenience trait for converting Rust types to PgValue parameters.
/// Kept for backward compatibility — prefer `ToSql` for new code.
pub trait ToParam {
    fn to_param(&self) -> PgValue;
}

impl<T: ToSql> ToParam for T {
    fn to_param(&self) -> PgValue {
        self.to_sql()
    }
}

// ─── MacAddr / Point Text Parsing ─────────────────────────────

/// Parse a MAC address from text format "xx:xx:xx:xx:xx:xx" or "xx-xx-xx-xx-xx-xx".
fn parse_macaddr_text(s: &str) -> PgResult<[u8; 6]> {
    let parts: Vec<&str> = if s.contains(':') {
        s.split(':').collect()
    } else if s.contains('-') {
        s.split('-').collect()
    } else {
        return Err(PgError::TypeConversion(format!("Invalid MAC address format: {}", s)));
    };
    if parts.len() != 6 {
        return Err(PgError::TypeConversion(format!("Invalid MAC address: {}", s)));
    }
    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| PgError::TypeConversion(format!("Invalid MAC address hex: {}", part)))?;
    }
    Ok(bytes)
}

/// Parse a point from text format "(x,y)".
fn parse_point_text(s: &str) -> PgResult<(f64, f64)> {
    let trimmed = s.trim();
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    let comma = inner.find(',')
        .ok_or_else(|| PgError::TypeConversion(format!("Invalid point format: {}", s)))?;
    let x: f64 = inner[..comma].trim().parse()
        .map_err(|_| PgError::TypeConversion(format!("Invalid point x: {}", &inner[..comma])))?;
    let y: f64 = inner[comma + 1..].trim().parse()
        .map_err(|_| PgError::TypeConversion(format!("Invalid point y: {}", &inner[comma + 1..])))?;
    Ok((x, y))
}

// ─── Binary NUMERIC Formatting ────────────────────────────────

/// Format a PostgreSQL binary NUMERIC value to a decimal string.
///
/// PostgreSQL stores NUMERIC as base-10000 digits with a weight
/// (exponent in base-10000), a sign flag, and a display scale.
fn format_numeric_binary(weight: i16, sign: u16, dscale: usize, digits: &[u16]) -> String {
    // Special values
    const _NUMERIC_POS: u16 = 0x0000;
    const NUMERIC_NEG: u16 = 0x4000;
    const NUMERIC_NAN: u16 = 0xC000;
    const NUMERIC_PINF: u16 = 0xD000;
    const NUMERIC_NINF: u16 = 0xF000;

    match sign {
        NUMERIC_NAN => return "NaN".to_string(),
        NUMERIC_PINF => return "Infinity".to_string(),
        NUMERIC_NINF => return "-Infinity".to_string(),
        _ => {}
    }

    if digits.is_empty() {
        // Zero — respect dscale
        return if dscale > 0 {
            format!("0.{}", "0".repeat(dscale))
        } else {
            "0".to_string()
        };
    }

    // Build the full base-10000 digit string
    // weight = number of base-10000 digits before the decimal point minus 1
    // So with weight=1, the first 2 digit groups (indices 0..=1) are before the decimal.

    let mut result = String::with_capacity(digits.len() * 4 + 4);

    if sign == NUMERIC_NEG {
        result.push('-');
    }

    // Integer part: digit groups 0..=weight
    let int_groups = (weight + 1).max(0) as usize;

    if int_groups == 0 {
        result.push('0');
    } else {
        for i in 0..int_groups {
            let d = if i < digits.len() { digits[i] } else { 0 };
            if i == 0 {
                // First group: no leading zeros
                result.push_str(&d.to_string());
            } else {
                // Subsequent groups: pad to 4 digits
                result.push_str(&format!("{:04}", d));
            }
        }
    }

    // Fractional part
    if dscale > 0 {
        result.push('.');
        let mut frac_chars = 0;
        let frac_start = int_groups;
        let mut i = frac_start;
        while frac_chars < dscale {
            let d = if i < digits.len() { digits[i] } else { 0 };
            let s = format!("{:04}", d);
            for ch in s.chars() {
                if frac_chars >= dscale {
                    break;
                }
                result.push(ch);
                frac_chars += 1;
            }
            i += 1;
        }
    }

    result
}

// ─── Binary Array Parsing ─────────────────────────────────────

/// Parse a PostgreSQL binary array value.
///
/// Binary array format:
///   ndim (i32) + flags (i32) + element_oid (i32)
///   for each dimension: len (i32) + lower_bound (i32)
///   for each element: len (i32) + data (len bytes), or len=-1 for NULL
fn parse_binary_array(data: &[u8]) -> PgResult<PgValue> {
    if data.len() < 12 {
        return Err(PgError::TypeConversion("Binary array too short".into()));
    }

    let ndim = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    // flags at offset 4 (has_null indicator — we handle NULLs inline)
    let element_oid = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

    if ndim == 0 {
        return Ok(PgValue::Array(Vec::new()));
    }
    if ndim != 1 {
        // We only support 1-dimensional arrays
        return Err(PgError::TypeConversion(
            format!("Unsupported array dimensions: {}", ndim),
        ));
    }

    let mut pos = 12;
    // Dimension length and lower bound
    if data.len() < pos + 8 {
        return Err(PgError::TypeConversion("Binary array dimension truncated".into()));
    }
    let num_elements = i32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;
    let _lower_bound = i32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    pos += 4;

    let mut values = Vec::with_capacity(num_elements);
    for _ in 0..num_elements {
        if data.len() < pos + 4 {
            return Err(PgError::TypeConversion("Binary array element truncated".into()));
        }
        let elem_len = i32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        if elem_len < 0 {
            values.push(PgValue::Null);
        } else {
            let elem_len = elem_len as usize;
            if data.len() < pos + elem_len {
                return Err(PgError::TypeConversion("Binary array element data truncated".into()));
            }
            let elem_data = &data[pos..pos + elem_len];
            values.push(PgValue::from_binary(element_oid, elem_data)?);
            pos += elem_len;
        }
    }

    Ok(PgValue::Array(values))
}

// ─── UUID Formatting/Parsing ─────────────────────────────────

/// Format a 16-byte UUID as a string: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
fn format_uuid(bytes: &[u8; 16]) -> String {
    fn hex(b: u8) -> (char, char) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        (HEX[(b >> 4) as usize] as char, HEX[(b & 0xf) as usize] as char)
    }
    let mut s = String::with_capacity(36);
    for (i, &b) in bytes.iter().enumerate() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            s.push('-');
        }
        let (hi, lo) = hex(b);
        s.push(hi);
        s.push(lo);
    }
    s
}

/// Parse a UUID from text format.
fn parse_uuid_text(s: &str) -> PgResult<[u8; 16]> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return Err(PgError::TypeConversion(format!("Invalid UUID: {}", s)));
    }
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| PgError::TypeConversion(format!("Invalid UUID hex: {}", s)))?;
    }
    Ok(bytes)
}

// ─── Date/Time Formatting/Parsing ────────────────────────────

/// PostgreSQL epoch: 2000-01-01. Days from Unix epoch (1970-01-01) to PG epoch.
const PG_EPOCH_DAYS: i32 = 10957;

/// Format a PostgreSQL date (days since 2000-01-01) as YYYY-MM-DD.
fn format_date(days: i32) -> String {
    let (y, m, d) = days_to_ymd(days + PG_EPOCH_DAYS);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Format a PostgreSQL time (microseconds since midnight) as HH:MM:SS.ffffff.
fn format_time(us: i64) -> String {
    let total_secs = us / 1_000_000;
    let frac = us % 1_000_000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if frac > 0 {
        format!("{:02}:{:02}:{:02}.{:06}", h, m, s, frac)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}

/// Format a PostgreSQL timestamp as YYYY-MM-DD HH:MM:SS.ffffff.
fn format_timestamp(us: i64) -> String {
    let total_days = (us / 86_400_000_000) as i32;
    let time_us = us % 86_400_000_000;
    let (time_us, total_days) = if time_us < 0 {
        (time_us + 86_400_000_000, total_days - 1)
    } else {
        (time_us, total_days)
    };
    let date = format_date(total_days);
    let time = format_time(time_us);
    format!("{} {}", date, time)
}

/// Format a PostgreSQL timestamptz.
fn format_timestamp_tz(us: i64) -> String {
    format!("{}+00", format_timestamp(us))
}

/// Format a PostgreSQL interval.
fn format_interval(months: i32, days: i32, us: i64) -> String {
    let mut parts = Vec::new();
    if months != 0 {
        let years = months / 12;
        let mons = months % 12;
        if years != 0 {
            parts.push(format!("{} year{}", years, if years.abs() != 1 { "s" } else { "" }));
        }
        if mons != 0 {
            parts.push(format!("{} mon{}", mons, if mons.abs() != 1 { "s" } else { "" }));
        }
    }
    if days != 0 {
        parts.push(format!("{} day{}", days, if days.abs() != 1 { "s" } else { "" }));
    }
    if us != 0 || parts.is_empty() {
        parts.push(format_time(us));
    }
    parts.join(" ")
}

/// Parse YYYY-MM-DD to PG days since 2000-01-01.
fn parse_date_text(s: &str) -> PgResult<i32> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(PgError::TypeConversion(format!("Invalid date: {}", s)));
    }
    let y: i32 = parts[0].parse().map_err(|_| PgError::TypeConversion("Bad year".into()))?;
    let m: u32 = parts[1].parse().map_err(|_| PgError::TypeConversion("Bad month".into()))?;
    let d: u32 = parts[2].parse().map_err(|_| PgError::TypeConversion("Bad day".into()))?;
    Ok(ymd_to_days(y, m, d) - PG_EPOCH_DAYS)
}

/// Parse HH:MM:SS[.ffffff] to microseconds since midnight.
fn parse_time_text(s: &str) -> PgResult<i64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return Err(PgError::TypeConversion(format!("Invalid time: {}", s)));
    }
    let h: i64 = parts[0].parse().map_err(|_| PgError::TypeConversion("Bad hour".into()))?;
    let m: i64 = parts[1].parse().map_err(|_| PgError::TypeConversion("Bad minute".into()))?;
    let (s_int, frac) = if parts.len() > 2 {
        parse_secs_frac(parts[2])?
    } else {
        (0, 0)
    };
    Ok(h * 3_600_000_000 + m * 60_000_000 + s_int * 1_000_000 + frac)
}

/// Parse a timestamp text: "YYYY-MM-DD HH:MM:SS[.ffffff][+/-TZ]"
fn parse_timestamp_text(s: &str) -> PgResult<i64> {
    // Strip timezone suffix for basic parsing
    let s = s.trim_end_matches("+00").trim_end_matches("+00:00");
    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(PgError::TypeConversion(format!("Invalid timestamp: {}", s)));
    }
    let date_days = parse_date_text(parts[0])?;
    let time_us = parse_time_text(parts[1])?;
    Ok(date_days as i64 * 86_400_000_000 + time_us)
}

/// Parse a PostgreSQL interval text representation.
fn parse_interval_text(s: &str) -> PgResult<(i32, i32, i64)> {
    // Simple parser for common formats like "1 year 2 mons 3 days 04:05:06"
    let mut months = 0i32;
    let mut days = 0i32;
    let mut microseconds = 0i64;

    let parts: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].contains(':') {
            // Time component
            microseconds = parse_time_text(parts[i])?;
            i += 1;
        } else if i + 1 < parts.len() {
            let val: i32 = parts[i]
                .parse()
                .map_err(|_| PgError::TypeConversion(format!("Invalid interval: {}", s)))?;
            let unit = parts[i + 1].to_lowercase();
            if unit.starts_with("year") {
                months += val * 12;
            } else if unit.starts_with("mon") {
                months += val;
            } else if unit.starts_with("day") {
                days += val;
            } else if unit.starts_with("hour") {
                microseconds += val as i64 * 3_600_000_000;
            } else if unit.starts_with("min") {
                microseconds += val as i64 * 60_000_000;
            } else if unit.starts_with("sec") {
                microseconds += val as i64 * 1_000_000;
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    Ok((months, days, microseconds))
}

/// Parse seconds with optional fractional part.
fn parse_secs_frac(s: &str) -> PgResult<(i64, i64)> {
    if let Some((int_s, frac_s)) = s.split_once('.') {
        let int_val: i64 = int_s.parse().map_err(|_| PgError::TypeConversion("Bad seconds".into()))?;
        // Pad or truncate fractional part to 6 digits
        let frac_str = if frac_s.len() >= 6 { &frac_s[..6] } else { frac_s };
        let frac_val: i64 = frac_str
            .parse()
            .map_err(|_| PgError::TypeConversion("Bad fractional seconds".into()))?;
        let padding = 10i64.pow(6 - frac_str.len() as u32);
        Ok((int_val, frac_val * padding))
    } else {
        let int_val: i64 = s.parse().map_err(|_| PgError::TypeConversion("Bad seconds".into()))?;
        Ok((int_val, 0))
    }
}

// ─── INET / Array Helpers ────────────────────────────────────

/// Format 16 bytes as an IPv6 address (abbreviated).
fn format_ipv6(bytes: &[u8]) -> String {
    // Build 8 groups of 16-bit values
    let mut groups = [0u16; 8];
    for i in 0..8 {
        groups[i] = u16::from_be_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
    }
    // Simple formatting (no zero-compression for correctness)
    groups
        .iter()
        .map(|g| format!("{:x}", g))
        .collect::<Vec<_>>()
        .join(":")
}

/// Encode an INET/CIDR text value into PostgreSQL binary format.
///
/// Accepts strings like `"192.168.1.0/24"`, `"10.0.0.1"`,
/// `"::1"`, `"2001:db8::/32"`.
pub fn encode_inet_binary(s: &str) -> PgResult<Vec<u8>> {
    let (addr_part, mask) = if let Some((a, m)) = s.split_once('/') {
        let mask: u8 = m
            .parse()
            .map_err(|_| PgError::TypeConversion(format!("Invalid mask: {}", m)))?;
        (a, Some(mask))
    } else {
        (s, None)
    };

    // Try IPv4 first
    if let Some(bytes) = parse_ipv4(addr_part) {
        let mask = mask.unwrap_or(32);
        Ok(vec![2, mask, 0, 4, bytes[0], bytes[1], bytes[2], bytes[3]])
    } else if let Some(bytes) = parse_ipv6(addr_part) {
        let mask = mask.unwrap_or(128);
        let mut buf = vec![3, mask, 0, 16];
        buf.extend_from_slice(&bytes);
        Ok(buf)
    } else {
        Err(PgError::TypeConversion(format!("Invalid IP address: {}", s)))
    }
}

/// Parse an IPv4 dotted-decimal string into 4 bytes.
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = part.parse().ok()?;
    }
    Some(bytes)
}

/// Parse an IPv6 address string into 16 bytes.
/// Supports `::` abbreviation.
fn parse_ipv6(s: &str) -> Option<[u8; 16]> {
    let mut bytes = [0u8; 16];

    if s == "::" {
        return Some(bytes); // all zeros
    }

    // Split on `::`
    let (left, right) = if let Some((l, r)) = s.split_once("::") {
        (l, r)
    } else {
        (s, "")
    };

    let left_groups: Vec<&str> = if left.is_empty() {
        Vec::new()
    } else {
        left.split(':').collect()
    };
    let right_groups: Vec<&str> = if right.is_empty() {
        Vec::new()
    } else {
        right.split(':').collect()
    };

    let left_count = left_groups.len();
    let right_count = right_groups.len();

    if s.contains("::") {
        if left_count + right_count > 8 {
            return None;
        }
    } else if left_count != 8 {
        return None;
    }

    // Fill from left
    for (i, group) in left_groups.iter().enumerate() {
        let val = u16::from_str_radix(group, 16).ok()?;
        bytes[i * 2] = (val >> 8) as u8;
        bytes[i * 2 + 1] = val as u8;
    }

    // Fill from right (anchored to the end)
    let right_start = 8 - right_count;
    for (i, group) in right_groups.iter().enumerate() {
        let val = u16::from_str_radix(group, 16).ok()?;
        let idx = (right_start + i) * 2;
        bytes[idx] = (val >> 8) as u8;
        bytes[idx + 1] = val as u8;
    }

    Some(bytes)
}

/// Escape an array element for PostgreSQL text array format.
///
/// PostgreSQL rules:
/// - `NULL` is unquoted.
/// - If the element contains `{`, `}`, `"`, `,`, `\`, or whitespace, or is
///   empty, wrap it in double quotes and escape embedded `"` and `\` by
///   doubling / backslash-escaping.
fn escape_array_element(s: &str) -> String {
    if s.is_empty() || s.eq_ignore_ascii_case("null") || needs_array_quoting(s) {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for ch in s.chars() {
            if ch == '"' || ch == '\\' {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

/// Returns true if the string needs quoting inside a PG array literal.
fn needs_array_quoting(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '{' | '}' | ',' | '"' | '\\') || c.is_whitespace())
}

// ─── Calendar Helpers ────────────────────────────────────────

/// Convert year/month/day to days since Unix epoch (1970-01-01).
fn ymd_to_days(y: i32, m: u32, d: u32) -> i32 {
    // Use the algorithm from http://howardhinnant.github.io/date_algorithms.html
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe as i32 - 719468) as i32
}

/// Convert days since Unix epoch to year/month/day.
fn days_to_ymd(days: i32) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Decode PostgreSQL hex-format bytea (\\x prefix).
fn decode_bytea_hex(s: &str) -> Vec<u8> {
    if let Some(hex) = s.strip_prefix("\\x") {
        let mut result = Vec::with_capacity(hex.len() / 2);
        let bytes = hex.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            let hi = hex_digit(bytes[i]);
            let lo = hex_digit(bytes[i + 1]);
            result.push((hi << 4) | lo);
            i += 2;
        }
        result
    } else {
        s.as_bytes().to_vec()
    }
}

fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_roundtrip() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let bytes = parse_uuid_text(uuid_str).unwrap();
        let formatted = format_uuid(&bytes);
        assert_eq!(formatted, uuid_str);
    }

    #[test]
    fn test_date_roundtrip() {
        let s = "2024-03-15";
        let days = parse_date_text(s).unwrap();
        let formatted = format_date(days);
        assert_eq!(formatted, s);
    }

    #[test]
    fn test_time_roundtrip() {
        let s = "14:30:45";
        let us = parse_time_text(s).unwrap();
        let formatted = format_time(us);
        assert_eq!(formatted, s);
    }

    #[test]
    fn test_time_with_frac() {
        let s = "14:30:45.123456";
        let us = parse_time_text(s).unwrap();
        let formatted = format_time(us);
        assert_eq!(formatted, s);
    }

    #[test]
    fn test_to_sql_i32() {
        assert_eq!(42i32.to_sql(), PgValue::Int4(42));
    }

    #[test]
    fn test_from_sql_i32() {
        let val = PgValue::Int4(42);
        assert_eq!(i32::from_sql(&val).unwrap(), 42);
    }

    #[test]
    fn test_from_sql_option() {
        let null = PgValue::Null;
        assert_eq!(Option::<i32>::from_sql(&null).unwrap(), None);
        let val = PgValue::Int4(42);
        assert_eq!(Option::<i32>::from_sql(&val).unwrap(), Some(42));
    }

    #[test]
    fn test_pg_epoch() {
        // 2000-01-01 should be day 0 in PG
        assert_eq!(parse_date_text("2000-01-01").unwrap(), 0);
        // 2000-01-02 should be day 1
        assert_eq!(parse_date_text("2000-01-02").unwrap(), 1);
    }

    // ─── INET binary format tests ─────────────────────────────

    #[test]
    fn test_inet_binary_ipv4() {
        // Binary: family=2, mask=32, is_cidr=0, len=4, addr=192.168.1.1
        let data = vec![2, 32, 0, 4, 192, 168, 1, 1];
        let val = PgValue::from_binary(oid::INET, &data).unwrap();
        assert_eq!(val, PgValue::Inet("192.168.1.1".to_string()));
    }

    #[test]
    fn test_inet_binary_ipv4_cidr() {
        // CIDR: 10.0.0.0/8
        let data = vec![2, 8, 1, 4, 10, 0, 0, 0];
        let val = PgValue::from_binary(oid::CIDR, &data).unwrap();
        assert_eq!(val, PgValue::Inet("10.0.0.0/8".to_string()));
    }

    #[test]
    fn test_inet_binary_ipv6() {
        // ::1 in binary: family=3, mask=128, is_cidr=0, len=16
        let mut data = vec![3, 128, 0, 16];
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let val = PgValue::from_binary(oid::INET, &data).unwrap();
        assert_eq!(val, PgValue::Inet("0:0:0:0:0:0:0:1".to_string()));
    }

    #[test]
    fn test_encode_inet_binary_ipv4() {
        let encoded = encode_inet_binary("192.168.1.1").unwrap();
        assert_eq!(encoded, vec![2, 32, 0, 4, 192, 168, 1, 1]);
    }

    #[test]
    fn test_encode_inet_binary_ipv4_cidr() {
        let encoded = encode_inet_binary("10.0.0.0/8").unwrap();
        assert_eq!(encoded, vec![2, 8, 0, 4, 10, 0, 0, 0]);
    }

    #[test]
    fn test_encode_inet_binary_ipv6_loopback() {
        let encoded = encode_inet_binary("::1").unwrap();
        assert_eq!(encoded.len(), 20); // 4 header + 16 addr
        assert_eq!(encoded[0], 3);   // AF_INET6
        assert_eq!(encoded[1], 128); // /128
        assert_eq!(encoded[19], 1);  // last byte = 1
    }

    // ─── Array escaping tests ─────────────────────────────────

    #[test]
    fn test_array_simple() {
        let arr = PgValue::Array(vec![
            PgValue::Int4(1),
            PgValue::Int4(2),
            PgValue::Int4(3),
        ]);
        let bytes = arr.to_text_bytes().unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "{1,2,3}");
    }

    #[test]
    fn test_array_with_null() {
        let arr = PgValue::Array(vec![
            PgValue::Text("hello".to_string()),
            PgValue::Null,
            PgValue::Text("world".to_string()),
        ]);
        let bytes = arr.to_text_bytes().unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "{hello,NULL,world}");
    }

    #[test]
    fn test_array_escaping_special_chars() {
        let arr = PgValue::Array(vec![
            PgValue::Text("hello world".to_string()),   // contains space
            PgValue::Text("a,b".to_string()),            // contains comma
            PgValue::Text("say \"hi\"".to_string()),     // contains quotes
        ]);
        let bytes = arr.to_text_bytes().unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(s, r#"{"hello world","a,b","say \"hi\""}"#);
    }

    #[test]
    fn test_array_escaping_empty_string() {
        let arr = PgValue::Array(vec![PgValue::Text("".to_string())]);
        let bytes = arr.to_text_bytes().unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), r#"{""}"#);
    }

    #[test]
    fn test_array_escaping_null_string() {
        // A text value that literally says "NULL" should be quoted
        let arr = PgValue::Array(vec![PgValue::Text("NULL".to_string())]);
        let bytes = arr.to_text_bytes().unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), r#"{"NULL"}"#);
    }

    // ─── IPv6 parse / format tests ────────────────────────────

    #[test]
    fn test_parse_ipv6_loopback() {
        let bytes = parse_ipv6("::1").unwrap();
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn test_parse_ipv6_full() {
        let bytes = parse_ipv6("2001:db8:0:0:0:0:0:1").unwrap();
        assert_eq!(bytes[0], 0x20);
        assert_eq!(bytes[1], 0x01);
        assert_eq!(bytes[2], 0x0d);
        assert_eq!(bytes[3], 0xb8);
        assert_eq!(bytes[15], 1);
    }

    #[test]
    fn test_parse_ipv6_abbreviated() {
        let bytes = parse_ipv6("2001:db8::1").unwrap();
        assert_eq!(bytes[0], 0x20);
        assert_eq!(bytes[1], 0x01);
        assert_eq!(bytes[15], 1);
    }

    // ─── Sprint 1: FromSql for Vec<u8> and [u8; 16] ──────────

    #[test]
    fn test_from_sql_vec_u8() {
        let val = PgValue::Bytes(vec![1, 2, 3]);
        assert_eq!(Vec::<u8>::from_sql(&val).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_from_sql_vec_u8_null() {
        let val = PgValue::Null;
        assert!(Vec::<u8>::from_sql(&val).is_err());
    }

    #[test]
    fn test_from_sql_uuid_bytes() {
        let bytes = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let val = PgValue::Uuid(bytes);
        assert_eq!(<[u8; 16]>::from_sql(&val).unwrap(), bytes);
    }

    #[test]
    fn test_from_sql_uuid_bytes_null() {
        let val = PgValue::Null;
        assert!(<[u8; 16]>::from_sql(&val).is_err());
    }

    // ─── Sprint 1: Array ToSql / FromSql ──────────────────────

    #[test]
    fn test_to_sql_vec_i32() {
        let arr = vec![1i32, 2, 3];
        assert_eq!(
            arr.to_sql(),
            PgValue::Array(vec![PgValue::Int4(1), PgValue::Int4(2), PgValue::Int4(3)])
        );
    }

    #[test]
    fn test_to_sql_vec_i32_type_oid() {
        let arr = vec![1i32, 2, 3];
        assert_eq!(arr.type_oid(), oid::INT4_ARRAY);
    }

    #[test]
    fn test_from_sql_vec_i32() {
        let val = PgValue::Array(vec![PgValue::Int4(1), PgValue::Int4(2), PgValue::Int4(3)]);
        assert_eq!(Vec::<i32>::from_sql(&val).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_to_sql_vec_string() {
        let arr = vec!["hello".to_string(), "world".to_string()];
        assert_eq!(
            arr.to_sql(),
            PgValue::Array(vec![
                PgValue::Text("hello".to_string()),
                PgValue::Text("world".to_string())
            ])
        );
    }

    #[test]
    fn test_from_sql_vec_string() {
        let val = PgValue::Array(vec![
            PgValue::Text("hello".to_string()),
            PgValue::Text("world".to_string()),
        ]);
        assert_eq!(
            Vec::<String>::from_sql(&val).unwrap(),
            vec!["hello".to_string(), "world".to_string()]
        );
    }

    #[test]
    fn test_to_sql_vec_bool() {
        let arr = vec![true, false, true];
        assert_eq!(
            arr.to_sql(),
            PgValue::Array(vec![PgValue::Bool(true), PgValue::Bool(false), PgValue::Bool(true)])
        );
    }

    #[test]
    fn test_from_sql_vec_bool() {
        let val = PgValue::Array(vec![PgValue::Bool(true), PgValue::Bool(false)]);
        assert_eq!(Vec::<bool>::from_sql(&val).unwrap(), vec![true, false]);
    }

    #[test]
    fn test_to_sql_vec_f64() {
        let arr = vec![1.5f64, 2.5];
        assert_eq!(
            arr.to_sql(),
            PgValue::Array(vec![PgValue::Float8(1.5), PgValue::Float8(2.5)])
        );
    }

    #[test]
    fn test_from_sql_vec_f64() {
        let val = PgValue::Array(vec![PgValue::Float8(1.5), PgValue::Float8(2.5)]);
        assert_eq!(Vec::<f64>::from_sql(&val).unwrap(), vec![1.5, 2.5]);
    }

    #[test]
    fn test_to_sql_slice_i32() {
        let arr: &[i32] = &[10, 20, 30];
        assert_eq!(
            arr.to_sql(),
            PgValue::Array(vec![PgValue::Int4(10), PgValue::Int4(20), PgValue::Int4(30)])
        );
    }

    #[test]
    fn test_from_sql_vec_i64() {
        let val = PgValue::Array(vec![PgValue::Int8(100), PgValue::Int8(200)]);
        assert_eq!(Vec::<i64>::from_sql(&val).unwrap(), vec![100i64, 200]);
    }

    #[test]
    fn test_from_sql_empty_array() {
        let val = PgValue::Array(vec![]);
        assert_eq!(Vec::<i32>::from_sql(&val).unwrap(), Vec::<i32>::new());
    }

    // ─── Sprint 1: Network type ToSql / FromSql ──────────────

    #[test]
    fn test_to_sql_ipaddr_v4() {
        use std::net::{IpAddr, Ipv4Addr};
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(ip.to_sql(), PgValue::Inet("192.168.1.1".to_string()));
        assert_eq!(ip.type_oid(), oid::INET);
    }

    #[test]
    fn test_to_sql_ipaddr_v6() {
        use std::net::{IpAddr, Ipv6Addr};
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert_eq!(ip.to_sql(), PgValue::Inet("::1".to_string()));
    }

    #[test]
    fn test_to_sql_ipv4addr() {
        use std::net::Ipv4Addr;
        let ip = Ipv4Addr::new(10, 0, 0, 1);
        assert_eq!(ip.to_sql(), PgValue::Inet("10.0.0.1".to_string()));
    }

    #[test]
    fn test_to_sql_ipv6addr() {
        use std::net::Ipv6Addr;
        let ip = Ipv6Addr::LOCALHOST;
        assert_eq!(ip.to_sql(), PgValue::Inet("::1".to_string()));
    }

    #[test]
    fn test_from_sql_ipaddr() {
        use std::net::{IpAddr, Ipv4Addr};
        let val = PgValue::Inet("192.168.1.1".to_string());
        let ip: IpAddr = IpAddr::from_sql(&val).unwrap();
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
    }

    #[test]
    fn test_from_sql_ipaddr_with_cidr() {
        use std::net::{IpAddr, Ipv4Addr};
        // Should strip the CIDR mask when converting to IpAddr
        let val = PgValue::Inet("10.0.0.0/8".to_string());
        let ip: IpAddr = IpAddr::from_sql(&val).unwrap();
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)));
    }

    #[test]
    fn test_from_sql_ipv4addr() {
        use std::net::Ipv4Addr;
        let val = PgValue::Inet("10.0.0.1".to_string());
        let ip = Ipv4Addr::from_sql(&val).unwrap();
        assert_eq!(ip, Ipv4Addr::new(10, 0, 0, 1));
    }

    #[test]
    fn test_from_sql_ipv6addr() {
        use std::net::Ipv6Addr;
        let val = PgValue::Inet("::1".to_string());
        let ip = Ipv6Addr::from_sql(&val).unwrap();
        assert_eq!(ip, Ipv6Addr::LOCALHOST);
    }

    #[test]
    fn test_from_sql_ipaddr_null() {
        use std::net::IpAddr;
        let val = PgValue::Null;
        assert!(IpAddr::from_sql(&val).is_err());
    }

    // ─── Sprint 3: Binary encoding/decoding ───────────────────

    #[test]
    fn test_to_binary_bytes_bool() {
        assert_eq!(PgValue::Bool(true).to_binary_bytes(), Some(vec![1]));
        assert_eq!(PgValue::Bool(false).to_binary_bytes(), Some(vec![0]));
    }

    #[test]
    fn test_to_binary_bytes_int2() {
        let val = PgValue::Int2(256);
        assert_eq!(val.to_binary_bytes(), Some(vec![1, 0]));
    }

    #[test]
    fn test_to_binary_bytes_int4() {
        let val = PgValue::Int4(0x01020304);
        assert_eq!(val.to_binary_bytes(), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_to_binary_bytes_int8() {
        let val = PgValue::Int8(1);
        assert_eq!(val.to_binary_bytes(), Some(vec![0, 0, 0, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn test_to_binary_bytes_float4() {
        let val = PgValue::Float4(1.0);
        assert_eq!(val.to_binary_bytes(), Some(1.0_f32.to_be_bytes().to_vec()));
    }

    #[test]
    fn test_to_binary_bytes_float8() {
        let val = PgValue::Float8(3.14);
        assert_eq!(val.to_binary_bytes(), Some(3.14_f64.to_be_bytes().to_vec()));
    }

    #[test]
    fn test_to_binary_bytes_uuid() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let val = PgValue::Uuid(bytes);
        assert_eq!(val.to_binary_bytes(), Some(bytes.to_vec()));
    }

    #[test]
    fn test_to_binary_bytes_date() {
        // 2000-01-01 = day 0
        let val = PgValue::Date(0);
        assert_eq!(val.to_binary_bytes(), Some(vec![0, 0, 0, 0]));
    }

    #[test]
    fn test_to_binary_bytes_interval() {
        let val = PgValue::Interval { months: 1, days: 2, microseconds: 3_000_000 };
        let mut expected = Vec::new();
        expected.extend_from_slice(&3_000_000_i64.to_be_bytes());
        expected.extend_from_slice(&2_i32.to_be_bytes());
        expected.extend_from_slice(&1_i32.to_be_bytes());
        assert_eq!(val.to_binary_bytes(), Some(expected));
    }

    #[test]
    fn test_to_binary_bytes_jsonb() {
        let val = PgValue::Jsonb(b"{}".to_vec());
        assert_eq!(val.to_binary_bytes(), Some(vec![1, b'{', b'}']));
    }

    #[test]
    fn test_to_binary_bytes_null() {
        assert_eq!(PgValue::Null.to_binary_bytes(), None);
    }

    #[test]
    fn test_to_binary_bytes_text() {
        let val = PgValue::Text("hello".to_string());
        assert_eq!(val.to_binary_bytes(), Some(b"hello".to_vec()));
    }

    #[test]
    fn test_prefers_binary() {
        assert!(PgValue::Int4(1).prefers_binary());
        assert!(PgValue::Bool(true).prefers_binary());
        assert!(PgValue::Float8(1.0).prefers_binary());
        assert!(PgValue::Uuid([0; 16]).prefers_binary());
        assert!(!PgValue::Text("hi".into()).prefers_binary());
        assert!(!PgValue::Numeric("1.23".into()).prefers_binary());
        assert!(!PgValue::Array(vec![]).prefers_binary());
        assert!(!PgValue::Inet("127.0.0.1".into()).prefers_binary());
    }

    #[test]
    fn test_from_binary_numeric_zero() {
        // ndigits=0, weight=0, sign=0 (pos), dscale=0
        let data = [0u8, 0, 0, 0, 0, 0, 0, 0];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("0".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_simple_integer() {
        // Value: 42
        // ndigits=1, weight=0, sign=0(pos), dscale=0, digit=42
        let data = [
            0, 1, // ndigits = 1
            0, 0, // weight = 0
            0, 0, // sign = positive
            0, 0, // dscale = 0
            0, 42, // digit[0] = 42
        ];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("42".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_negative() {
        // Value: -42
        // ndigits=1, weight=0, sign=0x4000(neg), dscale=0, digit=42
        let data = [
            0, 1,       // ndigits = 1
            0, 0,       // weight = 0
            0x40, 0x00, // sign = NUMERIC_NEG
            0, 0,       // dscale = 0
            0, 42,      // digit[0] = 42
        ];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("-42".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_with_decimal() {
        // Value: 1.23
        // ndigits=2, weight=0, sign=0(pos), dscale=2
        // digit[0]=1 (integer part), digit[1]=2300 (.2300 → 2 decimal places)
        let data = [
            0, 2,    // ndigits = 2
            0, 0,    // weight = 0
            0, 0,    // sign = positive
            0, 2,    // dscale = 2
            0, 1,    // digit[0] = 1
            0x08, 0xFC, // digit[1] = 2300
        ];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("1.23".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_nan() {
        // NaN: ndigits=0, weight=0, sign=0xC000, dscale=0
        let data = [0, 0, 0, 0, 0xC0, 0x00, 0, 0];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("NaN".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_zero_with_scale() {
        // 0.00: ndigits=0, weight=0, sign=0, dscale=2
        let data = [0, 0, 0, 0, 0, 0, 0, 2];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("0.00".to_string()));
    }

    #[test]
    fn test_from_binary_numeric_large() {
        // Value: 10000 (weight=1, digit=1 → 1*10000^(1+1-1) = 1*10000 = 10000)
        // Actually: weight=1 means 2 digit groups before decimal
        // digit[0]=1 → "1" then pad 4 zeros for next group = "10000"
        let data = [
            0, 1,    // ndigits = 1
            0, 1,    // weight = 1
            0, 0,    // sign = positive
            0, 0,    // dscale = 0
            0, 1,    // digit[0] = 1
        ];
        let val = PgValue::from_binary(oid::NUMERIC, &data).unwrap();
        assert_eq!(val, PgValue::Numeric("10000".to_string()));
    }

    #[test]
    fn test_from_binary_array_i32() {
        // Binary array: 1 dimension, no nulls, element OID = INT4 (23)
        // dimension: len=3, lower_bound=1
        // elements: 10, 20, 30
        let mut data = Vec::new();
        data.extend_from_slice(&1_i32.to_be_bytes());   // ndim = 1
        data.extend_from_slice(&0_i32.to_be_bytes());   // flags = 0
        data.extend_from_slice(&23_u32.to_be_bytes());  // element OID = INT4
        data.extend_from_slice(&3_i32.to_be_bytes());   // dim length = 3
        data.extend_from_slice(&1_i32.to_be_bytes());   // lower bound = 1
        // element 0: 10
        data.extend_from_slice(&4_i32.to_be_bytes());   // len = 4
        data.extend_from_slice(&10_i32.to_be_bytes());  // value = 10
        // element 1: 20
        data.extend_from_slice(&4_i32.to_be_bytes());
        data.extend_from_slice(&20_i32.to_be_bytes());
        // element 2: 30
        data.extend_from_slice(&4_i32.to_be_bytes());
        data.extend_from_slice(&30_i32.to_be_bytes());

        let val = PgValue::from_binary(oid::INT4_ARRAY, &data).unwrap();
        assert_eq!(val, PgValue::Array(vec![
            PgValue::Int4(10),
            PgValue::Int4(20),
            PgValue::Int4(30),
        ]));
    }

    #[test]
    fn test_from_binary_array_with_null() {
        // Binary array with a NULL element
        let mut data = Vec::new();
        data.extend_from_slice(&1_i32.to_be_bytes());   // ndim = 1
        data.extend_from_slice(&1_i32.to_be_bytes());   // flags = has_null
        data.extend_from_slice(&23_u32.to_be_bytes());  // element OID = INT4
        data.extend_from_slice(&2_i32.to_be_bytes());   // dim length = 2
        data.extend_from_slice(&1_i32.to_be_bytes());   // lower bound = 1
        // element 0: 42
        data.extend_from_slice(&4_i32.to_be_bytes());
        data.extend_from_slice(&42_i32.to_be_bytes());
        // element 1: NULL
        data.extend_from_slice(&(-1_i32).to_be_bytes());

        let val = PgValue::from_binary(oid::INT4_ARRAY, &data).unwrap();
        assert_eq!(val, PgValue::Array(vec![
            PgValue::Int4(42),
            PgValue::Null,
        ]));
    }

    #[test]
    fn test_from_binary_array_empty() {
        // ndim=0 → empty array
        let mut data = Vec::new();
        data.extend_from_slice(&0_i32.to_be_bytes());   // ndim = 0
        data.extend_from_slice(&0_i32.to_be_bytes());   // flags = 0
        data.extend_from_slice(&23_u32.to_be_bytes());  // element OID

        let val = PgValue::from_binary(oid::INT4_ARRAY, &data).unwrap();
        assert_eq!(val, PgValue::Array(Vec::new()));
    }

    #[test]
    fn test_from_binary_array_bool() {
        let mut data = Vec::new();
        data.extend_from_slice(&1_i32.to_be_bytes());   // ndim = 1
        data.extend_from_slice(&0_i32.to_be_bytes());   // flags
        data.extend_from_slice(&16_u32.to_be_bytes());  // element OID = BOOL
        data.extend_from_slice(&2_i32.to_be_bytes());   // dim length = 2
        data.extend_from_slice(&1_i32.to_be_bytes());   // lower bound
        // true
        data.extend_from_slice(&1_i32.to_be_bytes());   // len = 1
        data.push(1);
        // false
        data.extend_from_slice(&1_i32.to_be_bytes());
        data.push(0);

        let val = PgValue::from_binary(oid::BOOL_ARRAY, &data).unwrap();
        assert_eq!(val, PgValue::Array(vec![PgValue::Bool(true), PgValue::Bool(false)]));
    }

    #[test]
    fn test_from_binary_array_float8() {
        let mut data = Vec::new();
        data.extend_from_slice(&1_i32.to_be_bytes());
        data.extend_from_slice(&0_i32.to_be_bytes());
        data.extend_from_slice(&701_u32.to_be_bytes()); // FLOAT8
        data.extend_from_slice(&2_i32.to_be_bytes());
        data.extend_from_slice(&1_i32.to_be_bytes());
        // 1.5
        data.extend_from_slice(&8_i32.to_be_bytes());
        data.extend_from_slice(&1.5_f64.to_be_bytes());
        // 2.5
        data.extend_from_slice(&8_i32.to_be_bytes());
        data.extend_from_slice(&2.5_f64.to_be_bytes());

        let val = PgValue::from_binary(oid::FLOAT8_ARRAY, &data).unwrap();
        assert_eq!(val, PgValue::Array(vec![PgValue::Float8(1.5), PgValue::Float8(2.5)]));
    }

    #[test]
    fn test_binary_roundtrip_int4() {
        let original = PgValue::Int4(12345);
        let bytes = original.to_binary_bytes().unwrap();
        let decoded = PgValue::from_binary(oid::INT4, &bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_binary_roundtrip_float8() {
        let original = PgValue::Float8(std::f64::consts::PI);
        let bytes = original.to_binary_bytes().unwrap();
        let decoded = PgValue::from_binary(oid::FLOAT8, &bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_binary_roundtrip_uuid() {
        let original = PgValue::Uuid([0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let bytes = original.to_binary_bytes().unwrap();
        let decoded = PgValue::from_binary(oid::UUID, &bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_binary_roundtrip_interval() {
        let original = PgValue::Interval { months: 13, days: 5, microseconds: 7_200_000_000 };
        let bytes = original.to_binary_bytes().unwrap();
        let decoded = PgValue::from_binary(oid::INTERVAL, &bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_binary_roundtrip_bool() {
        for b in [true, false] {
            let original = PgValue::Bool(b);
            let bytes = original.to_binary_bytes().unwrap();
            let decoded = PgValue::from_binary(oid::BOOL, &bytes).unwrap();
            assert_eq!(original, decoded);
        }
    }

    // ─── MacAddr Tests ────────────────────────────────────────────

    #[test]
    fn test_macaddr_text_roundtrip() {
        let mac = [0x08, 0x00, 0x2b, 0x01, 0x02, 0x03];
        let val = PgValue::MacAddr(mac);
        let text = val.to_text_bytes().unwrap();
        assert_eq!(std::str::from_utf8(&text).unwrap(), "08:00:2b:01:02:03");
        let decoded = PgValue::from_text(oid::MACADDR, b"08:00:2b:01:02:03").unwrap();
        assert_eq!(decoded, PgValue::MacAddr(mac));
    }

    #[test]
    fn test_macaddr_binary_roundtrip() {
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let val = PgValue::MacAddr(mac);
        let bytes = val.to_binary_bytes().unwrap();
        assert_eq!(bytes, vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        let decoded = PgValue::from_binary(oid::MACADDR, &bytes).unwrap();
        assert_eq!(decoded, PgValue::MacAddr(mac));
    }

    #[test]
    fn test_macaddr_dash_format() {
        let decoded = parse_macaddr_text("08-00-2b-01-02-03").unwrap();
        assert_eq!(decoded, [0x08, 0x00, 0x2b, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_macaddr_invalid() {
        assert!(parse_macaddr_text("not_a_mac").is_err());
        assert!(parse_macaddr_text("08:00:2b").is_err()); // too few
        assert!(parse_macaddr_text("08:00:2b:01:02:03:04").is_err()); // too many
        assert!(parse_macaddr_text("ZZ:00:2b:01:02:03").is_err()); // bad hex
    }

    #[test]
    fn test_macaddr_tosql_fromsql() {
        let mac: [u8; 6] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB];
        let val = mac.to_sql();
        assert!(matches!(val, PgValue::MacAddr(_)));
        assert_eq!(mac.type_oid(), oid::MACADDR);
        let decoded: [u8; 6] = FromSql::from_sql(&val).unwrap();
        assert_eq!(decoded, mac);
    }

    // ─── Point Tests ──────────────────────────────────────────────

    #[test]
    fn test_point_text_roundtrip() {
        let val = PgValue::Point { x: 1.5, y: -2.5 };
        let text = val.to_text_bytes().unwrap();
        assert_eq!(std::str::from_utf8(&text).unwrap(), "(1.5,-2.5)");
        let decoded = PgValue::from_text(oid::POINT, b"(1.5,-2.5)").unwrap();
        match decoded {
            PgValue::Point { x, y } => {
                assert!((x - 1.5).abs() < f64::EPSILON);
                assert!((y - (-2.5)).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Point"),
        }
    }

    #[test]
    fn test_point_binary_roundtrip() {
        let val = PgValue::Point { x: 3.14, y: 2.72 };
        let bytes = val.to_binary_bytes().unwrap();
        assert_eq!(bytes.len(), 16);
        let decoded = PgValue::from_binary(oid::POINT, &bytes).unwrap();
        match decoded {
            PgValue::Point { x, y } => {
                assert!((x - 3.14).abs() < f64::EPSILON);
                assert!((y - 2.72).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Point"),
        }
    }

    #[test]
    fn test_point_tosql_fromsql() {
        let pt: (f64, f64) = (10.0, 20.0);
        let val = pt.to_sql();
        assert!(matches!(val, PgValue::Point { .. }));
        assert_eq!(pt.type_oid(), oid::POINT);
        let decoded: (f64, f64) = FromSql::from_sql(&val).unwrap();
        assert!((decoded.0 - 10.0).abs() < f64::EPSILON);
        assert!((decoded.1 - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_point_parse_without_parens() {
        let (x, y) = parse_point_text("3.5,7.2").unwrap();
        assert!((x - 3.5).abs() < f64::EPSILON);
        assert!((y - 7.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_point_parse_with_spaces() {
        let (x, y) = parse_point_text("( 3.5 , 7.2 )").unwrap();
        assert!((x - 3.5).abs() < f64::EPSILON);
        assert!((y - 7.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_point_parse_invalid() {
        assert!(parse_point_text("nope").is_err());
        assert!(parse_point_text("(1.0)").is_err());
    }

    // ─── Range Tests ──────────────────────────────────────────────

    #[test]
    fn test_range_text_roundtrip() {
        let val = PgValue::Range("[1,10)".to_string());
        let text = val.to_text_bytes().unwrap();
        assert_eq!(std::str::from_utf8(&text).unwrap(), "[1,10)");
    }

    #[test]
    fn test_range_from_text_int4range() {
        let decoded = PgValue::from_text(oid::INT4RANGE, b"[1,10)").unwrap();
        assert_eq!(decoded, PgValue::Range("[1,10)".to_string()));
    }

    #[test]
    fn test_range_from_text_tsrange() {
        let decoded = PgValue::from_text(oid::TSRANGE, b"[2024-01-01,2024-12-31]").unwrap();
        assert_eq!(decoded, PgValue::Range("[2024-01-01,2024-12-31]".to_string()));
    }

    #[test]
    fn test_range_empty() {
        let decoded = PgValue::from_text(oid::INT4RANGE, b"empty").unwrap();
        assert_eq!(decoded, PgValue::Range("empty".to_string()));
    }

    #[test]
    fn test_range_all_oid_variants() {
        for oid_val in [oid::INT4RANGE, oid::INT8RANGE, oid::NUMRANGE, oid::TSRANGE, oid::TSTZRANGE, oid::DATERANGE] {
            let decoded = PgValue::from_text(oid_val, b"[1,10)").unwrap();
            assert!(matches!(decoded, PgValue::Range(_)));
        }
    }

    // ─── Unix Socket Config Tests ─────────────────────────────────

    #[test]
    fn test_pgconfig_with_socket_dir() {
        use crate::connection::PgConfig;
        let config = PgConfig::new("localhost", 5432, "user", "pass", "mydb")
            .with_socket_dir("/var/run/postgresql");
        assert_eq!(config.socket_dir, Some("/var/run/postgresql".to_string()));
    }

    #[test]
    fn test_pgconfig_from_url_unix_query_param() {
        use crate::connection::PgConfig;
        let config = PgConfig::from_url("postgres://user:pass@/mydb?host=/var/run/postgresql").unwrap();
        assert_eq!(config.socket_dir, Some("/var/run/postgresql".to_string()));
        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "user");
    }

    #[test]
    fn test_pgconfig_from_url_percent_encoded() {
        use crate::connection::PgConfig;
        let config = PgConfig::from_url("postgres://user:pass@%2Fvar%2Frun%2Fpostgresql/mydb").unwrap();
        assert_eq!(config.socket_dir, Some("/var/run/postgresql".to_string()));
        assert_eq!(config.database, "mydb");
    }

    #[test]
    fn test_pgconfig_from_url_tcp_unchanged() {
        use crate::connection::PgConfig;
        let config = PgConfig::from_url("postgres://user:pass@localhost:5433/mydb").unwrap();
        assert!(config.socket_dir.is_none());
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5433);
        assert_eq!(config.database, "mydb");
    }

    #[test]
    fn test_macaddr_prefers_binary() {
        let val = PgValue::MacAddr([0; 6]);
        assert!(val.prefers_binary());
    }

    #[test]
    fn test_point_prefers_binary() {
        let val = PgValue::Point { x: 0.0, y: 0.0 };
        assert!(val.prefers_binary());
    }
}
