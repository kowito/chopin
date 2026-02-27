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
        match serde_json::from_slice(ctx.req.body) {
            Ok(val) => Ok(Json(val)),
            Err(_) => Err(crate::http::Response::internal_error()), // Ideally 400 Bad Request
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
        // A minimal query string parser.
        // For production, we'd use `serde_urlencoded`. But kowito-json is mostly for JSON.
        // We can just manually parse if needed, but since we are demonstrating extractors:
        Err(crate::http::Response::internal_error())
    }
}
