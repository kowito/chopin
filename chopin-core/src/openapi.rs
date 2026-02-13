use utoipa::OpenApi;

use crate::controllers::auth::{AuthResponse, LoginRequest, SignupRequest};
use crate::models::user::UserResponse;

/// Auto-generated OpenAPI documentation for Chopin.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chopin API",
        version = "0.1.0",
        description = "Chopin: High-fidelity engineering for the modern virtuoso."
    ),
    paths(
        crate::controllers::auth::signup,
        crate::controllers::auth::login,
    ),
    components(
        schemas(
            SignupRequest,
            LoginRequest,
            AuthResponse,
            UserResponse,
        )
    ),
    tags(
        (name = "auth", description = "Authentication endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

/// Add JWT Bearer security scheme to the OpenAPI spec.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}
