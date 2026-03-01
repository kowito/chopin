// src/extract.rs
use crate::http::{Context, Response};
use serde::Deserialize;

pub trait FromRequest<'a>: Sized {
    type Error: Into<Response>;

    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error>;
}

pub struct Json<T>(pub T);

impl<'a, T> FromRequest<'a> for Json<T>
where
    T: Deserialize<'a>,
{
    type Error = Response;

    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error> {
        // Use kowito-json's fast scanner for validation before deserializing
        let scanner = crate::json::Scanner::new(ctx.req.body);
        let mut tape = [0u32; 1024]; // Stack-allocated tape
        let tokens = scanner.scan(&mut tape);
        if tokens == 0 && !ctx.req.body.is_empty() {
            return Err(crate::http::Response::bad_request()); // Invalid JSON → 400
        }

        match serde_json::from_slice(ctx.req.body) {
            Ok(val) => Ok(Json(val)),
            Err(_) => Err(crate::http::Response::bad_request()), // Malformed JSON → 400
        }
    }
}

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
