// src/json.rs
pub use kowito_json::KJson;
pub use kowito_json::KView;
pub use kowito_json::scanner::Scanner;
pub use kowito_json::serialize::{Serialize, SerializeRaw};

/// A helper to serialize any type that implements `kowito_json::serialize::Serialize`
/// into a standard `Response`. Use this for peak "Schema-JIT" performance.
pub fn to_response<T: Serialize>(val: &T) -> crate::http::Response {
    crate::http::Response::json_fast(val)
}
