// src/middleware.rs

/// Implementing `RequireRole` as a zero-allocation middleware.
/// Since Chopin middleware uses static function pointers (`fn(...) -> Response`),
/// we need to generate discrete functions for specific roles if we want to bind them.
pub trait Role: PartialEq {
    // Empty trait to signify a Role Enum or Struct.
}

/// Helper macro to generate a zero-allocation middleware function for a specific role and claim type.
/// The `Claims` type must implement `fn has_role(&self, role: &Role) -> bool`.
#[macro_export]
macro_rules! require_role_middleware {
    ($middleware_name:ident, $claims_type:ty, $role_expr:expr, $has_role_fn:path) => {
        pub fn $middleware_name(
            ctx: chopin_core::http::Context,
            next: chopin_core::router::BoxedHandler,
        ) -> chopin_core::http::Response {
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

            if let Some(auth_val) = auth_header {
                if let Some(token) = auth_val.strip_prefix("Bearer ") {
                    // Quick check thread-local without allocating
                    let is_authorized = $crate::extractor::JWT_MANAGER.with(|m| {
                        if let Some(manager) = m.borrow().as_ref() {
                            if let Ok(claims) = manager.decode::<$claims_type>(token) {
                                return $has_role_fn(&claims, &$role_expr);
                            }
                        }
                        false
                    });

                    if is_authorized {
                        return next(ctx);
                    } else {
                        return chopin_core::http::Response::new(403);
                    }
                }
            }

            chopin_core::http::Response::new(401)
        }
    };
}
