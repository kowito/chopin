//! Procedural macros for Chopin framework's authentication and authorization system.
//!
//! Provides Django-inspired attribute macros for declarative access control:
//!
//! - `#[login_required]` — Requires a valid JWT token (authenticated user)
//! - `#[permission_required("codename")]` — Requires a specific permission
//!
//! # How it works
//!
//! These macros transform handler functions by injecting Chopin's auth extractors:
//!
//! - `#[login_required]` adds an `AuthUser` extractor as the first parameter
//! - `#[permission_required]` adds a `PermissionGuard` extractor and inserts
//!   a permission check at the beginning of the handler body
//!
//! # Examples
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//!
//! #[login_required]
//! async fn profile(State(state): State<AppState>) -> Result<ApiResponse<User>, ChopinError> {
//!     // `__chopin_auth` is available: the authenticated user's AuthUser extractor
//!     ApiResponse::success(get_user(&state.db, &__chopin_auth.0).await?)
//! }
//!
//! #[permission_required("can_edit_posts")]
//! async fn edit_post(
//!     State(state): State<AppState>,
//!     Path(id): Path<i32>,
//!     Json(body): Json<EditPost>,
//! ) -> Result<ApiResponse<Post>, ChopinError> {
//!     // Permission is checked before handler body executes.
//!     // `__chopin_guard` is available: the PermissionGuard extractor
//!     ApiResponse::success(update_post(&state.db, id, body).await?)
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// Require a valid JWT token (authenticated user) to access this handler.
///
/// Injects a `chopin_core::extractors::AuthUser` extractor as the first parameter.
/// The extracted user is available as `__chopin_auth` in the handler body.
///
/// # Example
///
/// ```rust,ignore
/// #[login_required]
/// async fn my_profile(
///     State(state): State<AppState>,
/// ) -> Result<ApiResponse<UserProfile>, ChopinError> {
///     let user_id = &__chopin_auth.0;
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn login_required(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    // Get the function name and existing parameters
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let output = &sig.output;
    let existing_params = &sig.inputs;
    let asyncness = &sig.asyncness;

    let expanded = quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            __chopin_auth: chopin_core::extractors::AuthUser,
            #existing_params
        ) #output
        #block
    };

    expanded.into()
}

/// Require a specific permission to access this handler.
///
/// Injects a `chopin_core::extractors::PermissionGuard` extractor and checks
/// the specified permission before the handler body executes.
///
/// The permission guard is available as `__chopin_guard` in the handler body,
/// providing access to `user_id()`, `role()`, `permissions()`, and more.
///
/// # Example
///
/// ```rust,ignore
/// #[permission_required("can_edit_posts")]
/// async fn edit_post(
///     State(state): State<AppState>,
///     Path(id): Path<i32>,
/// ) -> Result<ApiResponse<Post>, ChopinError> {
///     let user_id = __chopin_guard.user_id();
///     // ...
/// }
///
/// // Multiple permissions (require all):
/// #[permission_required("can_edit_posts", "can_publish")]
/// async fn publish_post(...) -> ... { }
/// ```
#[proc_macro_attribute]
pub fn permission_required(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // Parse permission name(s) from attribute
    let permissions = parse_permissions(attr);

    if permissions.is_empty() {
        return syn::Error::new_spanned(
            &input.sig.ident,
            "permission_required requires at least one permission codename",
        )
        .to_compile_error()
        .into();
    }

    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let output = &sig.output;
    let existing_params = &sig.inputs;
    let asyncness = &sig.asyncness;

    // Generate permission check code
    let permission_check = if permissions.len() == 1 {
        let perm = &permissions[0];
        quote! {
            __chopin_guard.require(#perm)?;
        }
    } else {
        let perm_array: Vec<_> = permissions.iter().map(|p| quote! { #p }).collect();
        quote! {
            __chopin_guard.require_all(&[#(#perm_array),*])?;
        }
    };

    let stmts = &block.stmts;

    let expanded = quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            __chopin_guard: chopin_core::extractors::PermissionGuard,
            #existing_params
        ) #output {
            #permission_check
            #(#stmts)*
        }
    };

    expanded.into()
}

/// Parse comma-separated string literals from the attribute token stream.
fn parse_permissions(attr: TokenStream) -> Vec<String> {
    let attr2: proc_macro2::TokenStream = attr.into();

    // Try to parse as comma-separated string literals
    let mut permissions = Vec::new();
    let mut tokens = attr2.into_iter().peekable();

    while let Some(token) = tokens.next() {
        if let proc_macro2::TokenTree::Literal(lit) = token {
            if let Ok(lit_str) = syn::parse2::<LitStr>(proc_macro2::TokenTree::Literal(lit).into())
            {
                permissions.push(lit_str.value());
            }
        }
        // Skip commas
        if let Some(proc_macro2::TokenTree::Punct(p)) = tokens.peek() {
            if p.as_char() == ',' {
                tokens.next();
            }
        }
    }

    permissions
}
