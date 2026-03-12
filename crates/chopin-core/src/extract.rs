//! Request extractors for typed data parsing.
//!
//! Use [`Context::extract`] to parse typed data from the request:
//! ```rust,ignore
//! let Json(body) = ctx.extract::<Json<MyPayload>>()?;
//! let Query(params) = ctx.extract::<Query<Pagination>>()?;
//! ```

use crate::http::{Context, Response};
use serde::Deserialize;

/// Trait for types that can be extracted from a request [`Context`].
///
/// Implement this trait to create custom extractors. The framework provides
/// [`Json`] and [`Query`] out of the box.
pub trait FromRequest<'a>: Sized {
    type Error: Into<Response>;

    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error>;
}

/// JSON body extractor.
///
/// Deserializes the request body as JSON into `T`. Returns `400 Bad Request`
/// if the body is not valid JSON or does not match the expected schema.
pub struct Json<T>(pub T);

impl<'a, T> FromRequest<'a> for Json<T>
where
    T: Deserialize<'a>,
{
    type Error = Response;

    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error> {
        match serde_json::from_slice(ctx.req.body) {
            Ok(val) => Ok(Json(val)),
            Err(_) => Err(crate::http::Response::bad_request()), // Malformed JSON → 400
        }
    }
}

/// Query string extractor.
///
/// Parses URL query parameters (e.g. `?page=2&limit=20`) into `T`.
/// Returns `400 Bad Request` if the query string cannot be deserialized.
pub struct Query<T>(pub T);

impl<'a, T> FromRequest<'a> for Query<T>
where
    T: Deserialize<'a>,
{
    type Error = Response;

    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error> {
        let qs = ctx.req.query.unwrap_or("");
        // Parse key=val&key2=val2 without allocating — build a serde deserializer
        match serde_urlencoded::from_str::<T>(qs) {
            Ok(val) => Ok(Query(val)),
            Err(_) => Err(crate::http::Response::bad_request()),
        }
    }
}
