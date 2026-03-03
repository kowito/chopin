// src/extractor.rs
use std::sync::OnceLock;

use crate::jwt::{AuthError, HasJti, JwtManager};
use chopin_core::extract::FromRequest;
use chopin_core::http::{Context, Response};
use serde::Deserialize;

// ─── ErrorHandler ───────────────────────────────────────────────────────────

/// Convert an [`AuthError`] into an HTTP [`Response`].
///
/// A default implementation is provided that returns an empty 401 for
/// token errors and an empty 500 for internal errors, preserving
/// backward-compatible behaviour.
///
/// Register a custom handler once at startup with [`set_error_handler`].
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::extractor::{ErrorHandler, set_error_handler};
/// use chopin_auth::jwt::AuthError;
/// use chopin_core::http::Response;
///
/// struct JsonErrors;
/// impl ErrorHandler for JsonErrors {
///     fn handle(&self, err: AuthError) -> Response {
///         let body = format!(r#"{{"error":"{err}"}}");
///         Response::json(401, &body)
///     }
/// }
///
/// set_error_handler(JsonErrors);
/// ```
pub trait ErrorHandler: Send + Sync {
    /// Convert `err` into an HTTP response that will be returned to the client.
    fn handle(&self, err: AuthError) -> Response;
}

struct DefaultErrorHandler;
impl ErrorHandler for DefaultErrorHandler {
    fn handle(&self, err: AuthError) -> Response {
        match err {
            AuthError::Expired | AuthError::Revoked | AuthError::InvalidToken(_) => {
                Response::new(401)
            }
            _ => Response::server_error(),
        }
    }
}

static GLOBAL_ERROR_HANDLER: OnceLock<Box<dyn ErrorHandler>> = OnceLock::new();

/// Register a custom [`ErrorHandler`] used by all [`Auth`] extractors.
///
/// Call this **once** before starting the server, after (or alongside)
/// [`init_jwt_manager`]. If never called, the default handler returns empty
/// 401/500 responses.
///
/// Panics if called more than once.
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::extractor::set_error_handler;
/// set_error_handler(MyJsonErrorHandler);
/// ```
pub fn set_error_handler(handler: impl ErrorHandler + 'static) {
    if GLOBAL_ERROR_HANDLER.set(Box::new(handler)).is_err() {
        panic!("ErrorHandler already set — call set_error_handler only once");
    }
}

#[inline]
fn dispatch_error(err: AuthError) -> Response {
    match GLOBAL_ERROR_HANDLER.get() {
        Some(h) => h.handle(err),
        None => DefaultErrorHandler.handle(err),
    }
}

// ─── Global manager ──────────────────────────────────────────────────────────

/// The global [`JwtManager`] shared across all threads.
///
/// Initialise it once at startup with [`init_jwt_manager`] before the server
/// starts accepting requests.
pub static GLOBAL_JWT_MANAGER: OnceLock<JwtManager> = OnceLock::new();

/// Initialise the global [`JwtManager`].
///
/// Call this **once** before starting the server. Panics if called more than once.
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::{JwtManager, init_jwt_manager};
/// init_jwt_manager(JwtManager::new(b"my-secret"));
/// ```
pub fn init_jwt_manager(manager: JwtManager) {
    if GLOBAL_JWT_MANAGER.set(manager).is_err() {
        panic!("JwtManager already initialised — call init_jwt_manager only once");
    }
}

// ─── Auth extractor ─────────────────────────────────────────────────────────

/// A request extractor that validates the `Authorization: Bearer <token>` header
/// and resolves to the decoded claims `T`.
///
/// `T` must implement both [`Deserialize`] and [`HasJti`]. Types that do not use
/// revocation can satisfy [`HasJti`] with a one-line empty impl.
///
/// # Responses on failure
/// - `401` – missing header, invalid/expired/revoked token.
/// - `500` – the global [`JwtManager`] was not initialised.
pub struct Auth<T> {
    pub claims: T,
}

impl<'a, T> FromRequest<'a> for Auth<T>
where
    T: for<'de> Deserialize<'de> + HasJti + 'static,
{
    type Error = Response;

    // `Response` is intentionally the error type here (HTTP 401/500 short-circuits).
    #[allow(clippy::result_large_err)]
    fn from_request(ctx: &'a Context<'a>) -> Result<Self, Self::Error> {
        // Extract the Authorization header.
        let auth_header = (0..ctx.req.header_count as usize).find_map(|i| {
            let (k, v) = ctx.req.headers[i];
            k.eq_ignore_ascii_case("Authorization").then_some(v)
        });

        let token = auth_header
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Response::new(401))?;

        let manager = GLOBAL_JWT_MANAGER
            .get()
            .ok_or_else(Response::server_error)?;

        let claims = manager.decode::<T>(token).map_err(dispatch_error)?;

        Ok(Auth { claims })
    }
}
