use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, ItemFn, parse_macro_input};

// ─── #[derive(IntoResponse)] ─────────────────────────────────────────────────

/// Derive `From<YourError> for chopin_core::http::Response` for an error enum.
///
/// Each variant must be annotated with `#[status(N)]` where `N` is the HTTP
/// status code.  Variants without `#[status]` default to `500`.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_macros::IntoResponse;
///
/// #[derive(IntoResponse)]
/// pub enum PostError {
///     #[status(404)] NotFound(i32),
///     #[status(422)] Validation(String),
///     #[status(500)] Db(chopin_orm::OrmError),
/// }
///
/// // Handlers can now use `?` directly:
/// #[get("/posts/:id")]
/// fn show(ctx: Context) -> Response {
///     let id: i32 = ctx.param_parse("id")?;
///     let post = services::get(id)?;   // PostError converts → Response
///     Response::json(&post)
/// }
/// ```
#[proc_macro_derive(IntoResponse, attributes(status))]
pub fn derive_into_response(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let data_enum = match &input.data {
        Data::Enum(e) => e,
        _ => {
            return syn::Error::new_spanned(name, "#[derive(IntoResponse)] only works on enums")
                .to_compile_error()
                .into();
        }
    };

    let arms = data_enum.variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        // Read #[status(N)], default 500.
        let status_code: u16 = variant
            .attrs
            .iter()
            .find_map(|attr| {
                if !attr.path().is_ident("status") {
                    return None;
                }
                attr.parse_args::<syn::LitInt>()
                    .ok()?
                    .base10_parse()
                    .ok()
            })
            .unwrap_or(500u16);

        match &variant.fields {
            Fields::Unit => quote! {
                #name::#variant_name => ::chopin_core::http::Response::new(#status_code),
            },
            Fields::Unnamed(_) => quote! {
                #name::#variant_name(..) => ::chopin_core::http::Response::new(#status_code),
            },
            Fields::Named(_) => quote! {
                #name::#variant_name { .. } => ::chopin_core::http::Response::new(#status_code),
            },
        }
    });

    TokenStream::from(quote! {
        impl From<#name> for ::chopin_core::http::Response {
            fn from(e: #name) -> Self {
                match e {
                    #(#arms)*
                }
            }
        }
    })
}

#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Get", attr, item)
}

#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Post", attr, item)
}

#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Put", attr, item)
}

#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Delete", attr, item)
}

#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Patch", attr, item)
}

#[proc_macro_attribute]
pub fn head(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Head", attr, item)
}

#[proc_macro_attribute]
pub fn options(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Options", attr, item)
}

#[proc_macro_attribute]
pub fn trace(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Trace", attr, item)
}

#[proc_macro_attribute]
pub fn connect(attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_route("Connect", attr, item)
}

// ─── #[require_role] ──────────────────────────────────────────────────────────

/// Inline role guard that wraps a Chopin handler with a JWT + RBAC check.
///
/// Returns `401` for missing/invalid tokens and `403` for insufficient role.
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_macros::{get, require_role};
/// use chopin_auth::{Role, StandardClaims};
///
/// #[derive(Debug, Clone, PartialEq)]
/// enum MyRole { Admin, User }
/// impl Role for MyRole {}
///
/// type Claims = StandardClaims<MyRole>;
///
/// // Place #[require_role] ABOVE #[get] so it wraps the handler body
/// // before the route is registered in the inventory.
/// #[require_role(Claims, MyRole::Admin)]
/// #[get("/admin/dashboard")]
/// pub fn admin_dashboard(ctx: chopin_core::Context) -> chopin_core::Response {
///     ctx.json(&"welcome, admin")
/// }
/// ```
///
/// # Requirements
/// - `chopin_auth` must be a dependency of the consuming crate.
/// - [`chopin_auth::init_jwt_manager`] must be called before the server starts.
/// - The claims type must implement [`chopin_auth::HasJti`].
/// - The claims type must implement [`chopin_auth::middleware::RoleCheck<R>`]
///   for the given role type (satisfied automatically by [`chopin_auth::StandardClaims<R>`]).
#[proc_macro_attribute]
pub fn require_role(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RequireRoleArgs);
    let mut func = parse_macro_input!(item as ItemFn);

    let claims_type = &args.claims_type;
    let role_expr = &args.role_expr;
    let ctx_ident = first_param_ident(&func);

    let original_stmts = func.block.stmts.clone();

    let new_block: syn::Block = syn::parse_quote! {
        {
            let __chopin_token = (0..#ctx_ident.req.header_count as usize)
                .find_map(|__ci| {
                    let (__ck, __cv) = #ctx_ident.req.headers[__ci];
                    if __ck.eq_ignore_ascii_case("Authorization") {
                        __cv.strip_prefix("Bearer ")
                    } else {
                        None
                    }
                });
            let Some(__chopin_token) = __chopin_token else {
                return ::chopin_core::http::Response::new(401);
            };
            let Some(__chopin_mgr) = ::chopin_auth::extractor::GLOBAL_JWT_MANAGER.get() else {
                return ::chopin_core::http::Response::server_error();
            };
            let __chopin_claims = match __chopin_mgr.decode::<#claims_type>(__chopin_token) {
                ::std::result::Result::Ok(__c) => __c,
                ::std::result::Result::Err(_) => {
                    return ::chopin_core::http::Response::new(401);
                }
            };
            if !::chopin_auth::middleware::RoleCheck::has_role(&__chopin_claims, &#role_expr) {
                return ::chopin_core::http::Response::new(403);
            }
            #(#original_stmts)*
        }
    };

    func.block = Box::new(new_block);
    TokenStream::from(quote! { #func })
}

// ─── #[require_scope] ─────────────────────────────────────────────────────────

/// Inline OAuth 2.0 scope guard that wraps a Chopin handler with a JWT + scope check.
///
/// Returns `401` for missing/invalid tokens and `403` for insufficient scope.
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_macros::{get, require_scope};
/// use chopin_auth::StandardClaims;
///
/// type Claims = StandardClaims<()>;
///
/// // Place #[require_scope] ABOVE #[get].
/// #[require_scope(Claims, "read:reports")]
/// #[get("/reports")]
/// pub fn list_reports(ctx: chopin_core::Context) -> chopin_core::Response {
///     ctx.json(&"reports")
/// }
/// ```
///
/// # Requirements
/// - `chopin_auth` must be a dependency of the consuming crate.
/// - [`chopin_auth::init_jwt_manager`] must be called before the server starts.
/// - The claims type must implement [`chopin_auth::middleware::ScopeCheck`]
///   (satisfied automatically by [`chopin_auth::StandardClaims<R>`]).
#[proc_macro_attribute]
pub fn require_scope(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RequireScopeArgs);
    let mut func = parse_macro_input!(item as ItemFn);

    let claims_type = &args.claims_type;
    let scope = &args.scope;
    let ctx_ident = first_param_ident(&func);

    let original_stmts = func.block.stmts.clone();

    let new_block: syn::Block = syn::parse_quote! {
        {
            let __chopin_token = (0..#ctx_ident.req.header_count as usize)
                .find_map(|__ci| {
                    let (__ck, __cv) = #ctx_ident.req.headers[__ci];
                    if __ck.eq_ignore_ascii_case("Authorization") {
                        __cv.strip_prefix("Bearer ")
                    } else {
                        None
                    }
                });
            let Some(__chopin_token) = __chopin_token else {
                return ::chopin_core::http::Response::new(401);
            };
            let Some(__chopin_mgr) = ::chopin_auth::extractor::GLOBAL_JWT_MANAGER.get() else {
                return ::chopin_core::http::Response::server_error();
            };
            let __chopin_claims = match __chopin_mgr.decode::<#claims_type>(__chopin_token) {
                ::std::result::Result::Ok(__c) => __c,
                ::std::result::Result::Err(_) => {
                    return ::chopin_core::http::Response::new(401);
                }
            };
            if !::chopin_auth::middleware::ScopeCheck::has_scope(&__chopin_claims, #scope) {
                return ::chopin_core::http::Response::new(403);
            }
            #(#original_stmts)*
        }
    };

    func.block = Box::new(new_block);
    TokenStream::from(quote! { #func })
}

// ─── Argument parsers ─────────────────────────────────────────────────────────

struct RequireRoleArgs {
    claims_type: syn::Type,
    role_expr: syn::Expr,
}

impl syn::parse::Parse for RequireRoleArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let claims_type = input.parse::<syn::Type>()?;
        input.parse::<syn::Token![,]>()?;
        let role_expr = input.parse::<syn::Expr>()?;
        Ok(RequireRoleArgs {
            claims_type,
            role_expr,
        })
    }
}

struct RequireScopeArgs {
    claims_type: syn::Type,
    scope: syn::LitStr,
}

impl syn::parse::Parse for RequireScopeArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let claims_type = input.parse::<syn::Type>()?;
        input.parse::<syn::Token![,]>()?;
        let scope = input.parse::<syn::LitStr>()?;
        Ok(RequireScopeArgs { claims_type, scope })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the identifier of the first function parameter (typically `ctx`).
fn first_param_ident(func: &ItemFn) -> proc_macro2::Ident {
    func.sig
        .inputs
        .first()
        .and_then(|arg| match arg {
            syn::FnArg::Typed(pt) => match pt.pat.as_ref() {
                syn::Pat::Ident(pi) => Some(pi.ident.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| syn::Ident::new("ctx", proc_macro2::Span::call_site()))
}

fn generate_route(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as syn::LitStr).value();
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let method_ident = syn::Ident::new(method, proc_macro2::Span::call_site());

    // Extract doc comments
    let mut docs = Vec::new();
    for attr in &input_fn.attrs {
        if attr.path().is_ident("doc")
            && let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            docs.push(s.value().trim().to_string());
        }
    }

    let summary = docs.first().cloned().unwrap_or_default();
    let description = if docs.len() > 1 {
        docs[1..].join("\n")
    } else {
        String::new()
    };

    let expanded = quote! {
        #input_fn

        ::chopin_core::inventory::submit! {
            ::chopin_core::RouteDef {
                method: ::chopin_core::http::Method::#method_ident,
                path: #path,
                handler: #fn_name,
                summary: #summary,
                description: #description,
            }
        }
    };

    TokenStream::from(expanded)
}
