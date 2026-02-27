//! PostgreSQL type system — type OIDs and value conversions.
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
#[derive(Debug, Clone)]
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
        }
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
            oid::FLOAT8 | oid::NUMERIC => {
                Ok(PgValue::Float8(s.parse().map_err(|_| {
                    PgError::TypeConversion("Invalid FLOAT8/NUMERIC".to_string())
                })?))
            }
            oid::JSONB => Ok(PgValue::Jsonb(data.to_vec())),
            oid::JSON => Ok(PgValue::Json(s.to_string())),
            oid::BYTEA => Ok(PgValue::Bytes(decode_bytea_hex(s))),
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
            oid::JSONB => {
                // First byte is version (1), rest is JSON
                if data.len() > 1 {
                    Ok(PgValue::Jsonb(data[1..].to_vec()))
                } else {
                    Ok(PgValue::Jsonb(Vec::new()))
                }
            }
            oid::BYTEA => Ok(PgValue::Bytes(data.to_vec())),
            _ => {
                // Fallback: treat as text
                Ok(PgValue::Text(String::from_utf8_lossy(data).to_string()))
            }
        }
    }
}

/// Convenience trait for converting Rust types to PgValue parameters.
pub trait ToParam {
    fn to_param(&self) -> PgValue;
}

impl ToParam for i32 {
    fn to_param(&self) -> PgValue {
        PgValue::Int4(*self)
    }
}
impl ToParam for i64 {
    fn to_param(&self) -> PgValue {
        PgValue::Int8(*self)
    }
}
impl ToParam for &str {
    fn to_param(&self) -> PgValue {
        PgValue::Text(self.to_string())
    }
}
impl ToParam for String {
    fn to_param(&self) -> PgValue {
        PgValue::Text(self.clone())
    }
}
impl ToParam for bool {
    fn to_param(&self) -> PgValue {
        PgValue::Bool(*self)
    }
}
impl ToParam for f64 {
    fn to_param(&self) -> PgValue {
        PgValue::Float8(*self)
    }
}
impl<T: ToParam> ToParam for Option<T> {
    fn to_param(&self) -> PgValue {
        match self {
            Some(v) => v.to_param(),
            None => PgValue::Null,
        }
    }
}

impl ToParam for PgValue {
    fn to_param(&self) -> PgValue {
        self.clone()
    }
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
