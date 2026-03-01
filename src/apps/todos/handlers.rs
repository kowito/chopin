use chopin_core::{Context, Response};
use chopin_macros::{get, post};
use super::services;

#[get("/todos")]
pub fn list(_ctx: Context) -> Response {
    // TODO: call services::list() and return JSON
    Response::ok("list todos")
}

#[get("/todos/:id")]
pub fn get_by_id(_ctx: Context) -> Response {
    // TODO: extract :id param, call services::get_by_id()
    Response::ok("get todos")
}

#[post("/todos")]
pub fn create(_ctx: Context) -> Response {
    // TODO: parse body, call services::create()
    Response::ok("create todos")
}
