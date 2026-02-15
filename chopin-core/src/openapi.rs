use utoipa::OpenApi;

use crate::controllers::auth::{AuthResponse, LoginRequest, SignupRequest};
use crate::models::user::UserResponse;

/// Auto-generated OpenAPI documentation for Chopin's built-in auth endpoints.
///
/// This documents the built-in `/api/auth/signup` and `/api/auth/login` routes.
/// Users can merge this with their own OpenAPI spec, or replace it entirely.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chopin API",
        version = "0.2.0",
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
///
/// Use this as a modifier in your own `#[derive(OpenApi)]` struct to
/// automatically add the `bearer_auth` security scheme.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::openapi::SecurityAddon;
/// use utoipa::OpenApi;
///
/// #[derive(OpenApi)]
/// #[openapi(
///     paths(my_handler),
///     security(("bearer_auth" = [])),
///     modifiers(&SecurityAddon)
/// )]
/// pub struct MyApiDoc;
/// ```
pub struct SecurityAddon;

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

/// Merge two OpenAPI specs together.
///
/// Paths, schemas, and tags from `other` are added into `base`.
/// This is useful for combining the built-in Chopin auth docs with
/// your own endpoint documentation.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::openapi::{ApiDoc, merge_openapi};
/// use utoipa::OpenApi;
///
/// #[derive(OpenApi)]
/// #[openapi(
///     paths(controllers::posts::list_posts, controllers::posts::create_post),
///     components(schemas(PostResponse, CreatePostRequest)),
///     tags((name = "posts", description = "Post endpoints"))
/// )]
/// struct MyApiDoc;
///
/// // Merge your docs with the built-in auth docs
/// let merged = merge_openapi(ApiDoc::openapi(), MyApiDoc::openapi());
/// ```
pub fn merge_openapi(
    mut base: utoipa::openapi::OpenApi,
    other: utoipa::openapi::OpenApi,
) -> utoipa::openapi::OpenApi {
    // Merge paths (Paths is not Option in utoipa 5)
    for (path, item) in other.paths.paths {
        base.paths.paths.insert(path, item);
    }

    // Merge components (schemas)
    if let Some(other_components) = other.components {
        if let Some(ref mut base_components) = base.components {
            for (name, schema) in other_components.schemas {
                base_components.schemas.insert(name, schema);
            }
            for (name, scheme) in other_components.security_schemes {
                base_components.security_schemes.insert(name, scheme);
            }
        } else {
            base.components = Some(other_components);
        }
    }

    // Merge tags
    if let Some(other_tags) = other.tags {
        if let Some(ref mut base_tags) = base.tags {
            for tag in other_tags {
                if !base_tags.iter().any(|t| t.name == tag.name) {
                    base_tags.push(tag);
                }
            }
        } else {
            base.tags = Some(other_tags);
        }
    }

    base
}
