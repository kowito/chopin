use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

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
