// src/middleware.rs

/// Marker trait for role types used with [`require_role_middleware`].
pub trait Role: PartialEq {}

/// Implemented by claims types that can be checked for a specific role.
///
/// Implement this alongside [`Role`] to enable role-based access with the
/// [`require_role_middleware!`] macro.
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::{Role, RoleCheck};
///
/// #[derive(PartialEq)]
/// enum MyRole { Admin, User }
/// impl Role for MyRole {}
///
/// struct MyClaims { sub: String, role: MyRole }
/// impl RoleCheck<MyRole> for MyClaims {
///     fn has_role(&self, role: &MyRole) -> bool {
///         self.role == *role
///     }
/// }
/// ```
///
/// Then use with the macro:
/// ```rust,ignore
/// require_role_middleware!(require_admin, MyClaims, MyRole::Admin, MyClaims::has_role);
/// ```
pub trait RoleCheck<R: Role> {
    /// Returns `true` if these claims grant the specified `role`.
    fn has_role(&self, role: &R) -> bool;
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
