extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitStr};

#[proc_macro_derive(Model, attributes(model))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut table_name = name.to_string().to_lowercase() + "s"; // Default plural table name
    let mut pk_field = None;
    let mut columns = Vec::new();

    // Parse struct attributes for table_name
    for attr in &input.attrs {
        if attr.path().is_ident("model") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table_name") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse()?;
                    table_name = s.value();
                }
                Ok(())
            });
        }
    }

    let fields = if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields) = &data_struct.fields {
            let mut extracted = Vec::new();
            for f in &fields.named {
                let field_name = match &f.ident {
                    Some(ident) => ident,
                    None => return syn::Error::new_spanned(f, "All fields must have names").to_compile_error().into(),
                };
                let field_name_str = field_name.to_string();
                columns.push(field_name_str);

                // Check for primary_key attribute
                for attr in &f.attrs {
                    if attr.path().is_ident("model") {
                        let _ = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("primary_key") {
                                pk_field = Some(field_name.clone());
                            }
                            Ok(())
                        });
                    }
                }
                
                extracted.push(field_name.clone());
            }
            extracted
        } else {
            return syn::Error::new_spanned(input, "Model can only be derived for structs with named fields").to_compile_error().into();
        }
    } else {
        return syn::Error::new_spanned(input, "Model can only be derived for structs with named fields").to_compile_error().into();
    };

    if pk_field.is_none() {
        if columns.contains(&"id".to_string()) {
            pk_field = Some(syn::Ident::new("id", proc_macro2::Span::call_site()));
        } else {
            return syn::Error::new_spanned(&name, "Model requires a primary key field (e.g., #[model(primary_key)] id) or a field named 'id'").to_compile_error().into();
        }
    }

    let pk_ident = match pk_field {
        Some(ident) => ident,
        None => return syn::Error::new_spanned(&name, "Missing primary key").to_compile_error().into(),
    };
    let pk_name = pk_ident.to_string();
    let field_names_str: Vec<String> = columns;

    let expanded = quote! {
        impl chopin_orm::Model for #name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn primary_key_column() -> &'static str {
                #pk_name
            }

            fn columns() -> &'static [&'static str] {
                &[#(#field_names_str),*]
            }

            fn primary_key_value(&self) -> chopin_pg::PgValue {
                use chopin_pg::types::ToParam;
                self.#pk_ident.to_param()
            }

            fn get_values(&self) -> Vec<chopin_pg::PgValue> {
                use chopin_pg::types::ToParam;
                vec![
                    #(self.#fields.to_param()),*
                ]
            }

            fn set_primary_key(&mut self, value: chopin_pg::PgValue) -> chopin_pg::PgResult<()> {
                self.#pk_ident = chopin_orm::ExtractValue::from_pg_value(value)?;
                Ok(())
            }
        }

        impl chopin_orm::FromRow for #name {
            fn from_row(row: &chopin_pg::Row) -> chopin_pg::PgResult<Self> {
                Ok(Self {
                    #(
                        #fields: chopin_orm::ExtractValue::extract(row, stringify!(#fields))?,
                    )*
                })
            }
        }

        impl #name {
            pub fn find() -> chopin_orm::QueryBuilder<#name> {
                chopin_orm::QueryBuilder::new()
            }
        }
    };

    TokenStream::from(expanded)
}
