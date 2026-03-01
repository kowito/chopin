use chopin_core::{Context, Response};
use chopin_macros::{get, post};
use super::services;

#[get("/todos")]
pub fn list(_ctx: Context) -> Response {
    // TODO: call services::list() and return JSON
    Response::text("list todos")
}

#[get("/todos/:id")]
pub fn get_by_id(ctx: Context) -> Response {
    // ctx.param() extracts the :id path segment
    let _id = ctx.param("id").unwrap_or("0");
    // TODO: call services::get_by_id(_id)
    Response::text("get todos")
}

#[post("/todos")]
pub fn create(_ctx: Context) -> Response {
    // TODO: parse body, call services::create()
    Response::text("create todos")
}
