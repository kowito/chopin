//! Re-exports from `kowito-json` for zero-copy, schema-JIT JSON serialisation.
//!
//! For typical use, prefer [`Response::json`](crate::http::Response::json) which
//! accepts any `serde::Serialize` type.  Use [`KJson`] and [`to_response`] when
//! you need the maximum-throughput schema-JIT path.
// src/json.rs
pub use kowito_json::KJson;
pub use kowito_json::KView;
pub use kowito_json::scanner::Scanner;
pub use kowito_json::serialize::{Serialize, SerializeRaw};

/// A helper to serialize any type that implements `kowito_json::serialize::Serialize`
/// into a standard `Response`. Use this for peak "Schema-JIT" performance.
pub fn to_response<T: Serialize>(val: &T) -> crate::http::Response {
    crate::http::Response::json(val)
}
