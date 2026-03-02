extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, parse_macro_input};

#[proc_macro_derive(Model, attributes(model))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut table_name = name.to_string().to_lowercase() + "s"; // Default plural table name
    let mut pk_fields = Vec::new();
    let mut generated_fields = Vec::new();
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
                    None => {
                        return syn::Error::new_spanned(f, "All fields must have names")
                            .to_compile_error()
                            .into();
                    }
                };
                let field_name_str = field_name.to_string();
                columns.push(field_name_str.clone());

                let mut is_pk = false;
                let mut is_gen = false;
                // Check for primary_key attribute
                for attr in &f.attrs {
                    if attr.path().is_ident("model") {
                        let _ = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("primary_key") {
                                is_pk = true;
                            }
                            if meta.path.is_ident("generated") {
                                is_gen = true;
                            }
                            Ok(())
                        });
                    }
                }
                
                if is_pk {
                    pk_fields.push(field_name.clone());
                }
                if is_gen {
                    generated_fields.push(field_name.clone());
                }

                extracted.push(field_name.clone());
            }
            extracted
        } else {
            return syn::Error::new_spanned(
                input,
                "Model can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new_spanned(
            input,
            "Model can only be derived for structs with named fields",
        )
        .to_compile_error()
        .into();
    };

    if pk_fields.is_empty() {
        if columns.contains(&"id".to_string()) {
            pk_fields.push(syn::Ident::new("id", proc_macro2::Span::call_site()));
            generated_fields.push(syn::Ident::new("id", proc_macro2::Span::call_site()));
        } else {
            return syn::Error::new_spanned(name, "Model requires at least one primary key field (e.g., #[model(primary_key)] id) or a field named 'id'").to_compile_error().into();
        }
    }

    let field_names_str: Vec<String> = columns.clone();
    let pk_names_str: Vec<String> = pk_fields.iter().map(|i| i.to_string()).collect();
    let gen_names_str: Vec<String> = generated_fields.iter().map(|i| i.to_string()).collect();

    let gen_field_names = generated_fields.clone();

    let expanded = quote! {
        impl chopin_orm::Model for #name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn primary_key_columns() -> &'static [&'static str] {
                &[#(#pk_names_str),*]
            }
            
            fn generated_columns() -> &'static [&'static str] {
                &[#(#gen_names_str),*]
            }

            fn columns() -> &'static [&'static str] {
                &[#(#field_names_str),*]
            }

            fn primary_key_values(&self) -> Vec<chopin_pg::PgValue> {
                use chopin_pg::types::ToSql;
                vec![
                    #(self.#pk_fields.to_sql()),*
                ]
            }

            fn get_values(&self) -> Vec<chopin_pg::PgValue> {
                use chopin_pg::types::ToSql;
                vec![
                    #(self.#fields.to_sql()),*
                ]
            }

            fn set_generated_values(&mut self, mut values: Vec<chopin_pg::PgValue>) -> chopin_orm::OrmResult<()> {
                if values.len() != #gen_names_str.len() {
                    return Err(chopin_orm::OrmError::ModelError("Generated values length mismatch".to_string()));
                }
                let mut iter = values.into_iter();
                #(
                    if let Some(val) = iter.next() {
                        self.#gen_field_names = chopin_orm::ExtractValue::from_pg_value(val)?;
                    }
                )*
                Ok(())
            }
        }

        impl chopin_orm::FromRow for #name {
            fn from_row(row: &chopin_pg::Row) -> chopin_orm::OrmResult<Self> {
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
