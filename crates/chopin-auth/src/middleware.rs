// src/middleware.rs

/// Marker trait for role types used with [`require_role_middleware`].
pub trait Role: PartialEq {}

/// Implemented by claims types that can be checked for a specific role.
pub trait RoleCheck<R: Role> {
    /// Returns `true` if these claims grant the specified `role`.
    fn has_role(&self, role: &R) -> bool;
}

/// Implemented by claims types that carry OAuth 2.0 scopes.
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::ScopeCheck;
///
/// struct MyClaims { sub: String, scope: String }
/// impl ScopeCheck for MyClaims {
///     fn has_scope(&self, scope: &str) -> bool {
///         self.scope.split(' ').any(|s| s == scope)
///     }
/// }
/// ```
pub trait ScopeCheck {
    /// Returns `true` if the claims include the specified scope.
    fn has_scope(&self, scope: &str) -> bool;
}

/// Generate a zero-allocation middleware function that requires a specific role.
///
/// The generated function reads the `Authorization: Bearer <token>` header,
/// decodes the JWT using the global [`JwtManager`], and calls `$has_role_fn` on
/// the decoded claims. Responds with:
/// - `401` – missing or invalid token.
/// - `403` – authenticated but wrong role.
///
/// # Requirements
/// - [`init_jwt_manager`](crate::extractor::init_jwt_manager) must have been called before the server starts.
/// - `$claims_type` must implement [`HasJti`](crate::jwt::HasJti) (empty impl is fine).
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::{Role, require_role_middleware};
///
/// #[derive(PartialEq)]
/// enum MyRole { Admin, User }
/// impl Role for MyRole {}
///
/// require_role_middleware!(require_admin, MyClaims, MyRole::Admin, MyClaims::has_role);
/// // then: router.middleware(require_admin)
/// ```
#[macro_export]
macro_rules! require_role_middleware {
    ($middleware_name:ident, $claims_type:ty, $role_expr:expr, $has_role_fn:path) => {
        pub fn $middleware_name(
            ctx: chopin_core::http::Context,
            next: chopin_core::router::BoxedHandler,
        ) -> chopin_core::http::Response {
            // Extract the Authorization header.
            let token = (0..ctx.req.header_count as usize).find_map(|i| {
                let (k, v) = ctx.req.headers[i];
                if k.eq_ignore_ascii_case("Authorization") {
                    v.strip_prefix("Bearer ")
                } else {
                    None
                }
            });

            let Some(token) = token else {
                return chopin_core::http::Response::new(401);
            };

            let Some(manager) = $crate::extractor::GLOBAL_JWT_MANAGER.get() else {
                return chopin_core::http::Response::server_error();
            };

            match manager.decode::<$claims_type>(token) {
                Ok(claims) if $has_role_fn(&claims, &$role_expr) => next(ctx),
                Ok(_) => chopin_core::http::Response::new(403),
                Err(_) => chopin_core::http::Response::new(401),
            }
        }
    };
}

/// Generate a middleware function that requires a specific OAuth 2.0 scope.
///
/// The generated function reads `Authorization: Bearer <token>`, decodes the JWT,
/// and checks `ScopeCheck::has_scope` on the decoded claims. Responds with:
/// - `401` – missing, invalid, or expired token.
/// - `403` – authenticated but insufficient scope.
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::require_scope_middleware;
///
/// require_scope_middleware!(require_read_users, MyClaims, "read:users");
/// // then: router.middleware(require_read_users)
/// ```
#[macro_export]
macro_rules! require_scope_middleware {
    ($middleware_name:ident, $claims_type:ty, $scope:expr) => {
        pub fn $middleware_name(
            ctx: chopin_core::http::Context,
            next: chopin_core::router::BoxedHandler,
        ) -> chopin_core::http::Response {
            let token = (0..ctx.req.header_count as usize).find_map(|i| {
                let (k, v) = ctx.req.headers[i];
                if k.eq_ignore_ascii_case("Authorization") {
                    v.strip_prefix("Bearer ")
                } else {
                    None
                }
            });

            let Some(token) = token else {
                return chopin_core::http::Response::new(401);
            };

            let Some(manager) = $crate::extractor::GLOBAL_JWT_MANAGER.get() else {
                return chopin_core::http::Response::server_error();
            };

            match manager.decode::<$claims_type>(token) {
                Ok(claims) => {
                    if $crate::ScopeCheck::has_scope(&claims, $scope) {
                        next(ctx)
                    } else {
                        chopin_core::http::Response::new(403)
                    }
                }
                Err(_) => chopin_core::http::Response::new(401),
            }
        }
    };
}
