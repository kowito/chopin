// src/extractor.rs
use crate::jwt::JwtManager;
use chopin_core::extract::FromRequest;
use chopin_core::http::{Context, Response};
use serde::Deserialize;

pub struct Auth<T> {
    pub claims: T,
}

// We rely on a global or context-provided JwtManager.
// For shared-nothing, the easiest way to provide the JwtManager is via a thread-local,
// but since this is an extractor, let's look at how chopin handles state.
// Wait, `Context` does not have an `extensions` or `state` map right now.
// For now, let's use a thread-local for the `JwtManager`.
// The user starts the server, and they would initialize the thread-local token verifier.

thread_local! {
    pub static JWT_MANAGER: std::cell::RefCell<Option<JwtManager>> = const { std::cell::RefCell::new(None) };
}

pub fn init_jwt_manager(manager: JwtManager) {
    JWT_MANAGER.with(|m| *m.borrow_mut() = Some(manager));
}

impl<'a, T> FromRequest<'a> for Auth<T>
where
    T: for<'de> Deserialize<'de> + 'static,
{
    type Error = Response;

    // `Response` is intentionally the error type here (HTTP 401 / 500 short-circuits).
    // The size increase comes from the inline header slab in `Headers`; the allocation
    // pattern is unchanged at the call site.
    #[allow(clippy::result_large_err)]
    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error> {
        let auth_header = {
            let mut found = None;
            for i in 0..ctx.req.header_count as usize {
                let (k, v) = ctx.req.headers[i];
                if k.eq_ignore_ascii_case("Authorization") {
                    found = Some(v);
                    break;
                }
            }
            found
        };

        if let Some(auth_val) = auth_header
            && let Some(token) = auth_val.strip_prefix("Bearer ")
        {
            let claims = JWT_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref() {
                    manager.decode::<T>(token).map_err(|_| Response::new(401))
                } else {
                    // Manager not initialized
                    Err(Response::server_error())
                }
            })?;

            return Ok(Auth { claims });
        }

        Err(Response::new(401))
    }
}
