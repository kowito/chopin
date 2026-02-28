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
            return Err(crate::http::Response::internal_error()); // Invalid JSON
        }

        match serde_json::from_slice(ctx.req.body) {
            Ok(val) => Ok(Json(val)),
            Err(_) => Err(crate::http::Response::internal_error()),
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
        let _qs = ctx.req.query.unwrap_or("");
        // A minimal query string parser.
        // For production, we'd use `serde_urlencoded`. But kowito-json is mostly for JSON.
        // We can just manually parse if needed, but since we are demonstrating extractors:
        Err(crate::http::Response::internal_error())
    }
}
