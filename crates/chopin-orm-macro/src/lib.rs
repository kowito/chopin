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
    let mut has_many_rels = Vec::new(); // stores (related_model_ident, fk_column_name_str)

    // Parse struct attributes for table_name
    for attr in &input.attrs {
        if attr.path().is_ident("model") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table_name") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse()?;
                    table_name = s.value();
                }
                if meta.path.is_ident("has_many") {
                    let mut target_ident: Option<syn::Ident> = None;
                    let mut fk_name = String::new();

                    let _ = meta.parse_nested_meta(|inner| {
                        if inner.path.is_ident("fk") {
                            let value = inner.value()?;
                            let s: LitStr = value.parse()?;
                            fk_name = s.value();
                        } else if target_ident.is_none() {
                            target_ident = inner.path.get_ident().cloned();
                        }
                        Ok(())
                    });

                    if let Some(ident) = target_ident {
                        has_many_rels.push((ident, fk_name));
                    }
                }
                Ok(())
            });
        }
    }

    let mut field_types = Vec::new();
    let mut non_pk_fields = Vec::new();
    let mut non_pk_types = Vec::new();
    let mut belongs_to_fks = Vec::new(); // stores (field_ident, related_model_ident)

    let fields_list = if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(syn_fields) = &data_struct.fields {
            let mut extracted = Vec::new();
            for f in &syn_fields.named {
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
                field_types.push(f.ty.clone());

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
                            if meta.path.is_ident("belongs_to") {
                                let _ = meta.parse_nested_meta(|inner| {
                                    if let Some(ident) = inner.path.get_ident() {
                                        belongs_to_fks.push((field_name.clone(), ident.clone()));
                                    }
                                    Ok(())
                                });
                            }
                            Ok(())
                        });
                    }
                }

                if is_pk {
                    pk_fields.push(field_name.clone());
                    let ty = &f.ty;
                    let ty_str = quote::quote!(#ty).to_string().replace(" ", "");
                    if ty_str == "i32" || ty_str == "i64" {
                        is_gen = true;
                    }
                } else {
                    non_pk_fields.push(field_name.clone());
                    non_pk_types.push(f.ty.clone());
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

    let column_enum_name =
        syn::Ident::new(&format!("{}Column", name), proc_macro2::Span::call_site());
    let _active_model_name = syn::Ident::new(
        &format!("{}ActiveModel", name),
        proc_macro2::Span::call_site(),
    );

    let gen_field_names = generated_fields.clone();
    let gen_fields_len = generated_fields.len();

    let mut column_defs = Vec::new();
    let mut col_names = Vec::new();
    let mut col_types = Vec::new();
    for (i, field_name) in columns.iter().enumerate() {
        let ty = &field_types[i];
        let is_pk = pk_names_str.contains(field_name);
        let is_gen = gen_names_str.contains(field_name);

        let mut not_null = true;
        let mut inner_ty = ty;
        if let syn::Type::Path(type_path) = ty
            && let Some(segment) = type_path.path.segments.last()
            && segment.ident == "Option"
        {
            not_null = false;
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                && let Some(syn::GenericArgument::Type(t)) = args.args.first()
            {
                inner_ty = t;
            }
        }

        let type_str = quote::quote!(#inner_ty).to_string().replace(" ", "");

        let mut sql_type = match type_str.as_str() {
            "i32" if is_gen && is_pk && pk_fields.len() == 1 => "SERIAL PRIMARY KEY".to_string(),
            "i32" if is_gen => "SERIAL".to_string(),
            "i32" => "INT".to_string(),
            "i64" if is_gen && is_pk && pk_fields.len() == 1 => "BIGSERIAL PRIMARY KEY".to_string(),
            "i64" if is_gen => "BIGSERIAL".to_string(),
            "i64" => "BIGINT".to_string(),
            "String" => "TEXT".to_string(),
            "bool" => "BOOLEAN".to_string(),
            "f64" => "DOUBLE PRECISION".to_string(),
            _ => "TEXT".to_string(),
        };

        if is_pk && pk_fields.len() == 1 && !sql_type.contains("PRIMARY KEY") {
            sql_type.push_str(" PRIMARY KEY");
        }

        if not_null && !sql_type.contains("PRIMARY KEY") && !sql_type.contains("SERIAL") {
            sql_type.push_str(" NOT NULL");
        }

        col_names.push(field_name.clone());
        col_types.push(sql_type.clone());
        column_defs.push(format!("{} {}", field_name, sql_type));
    }

    if pk_fields.len() > 1 {
        let pk_csv = pk_names_str.join(", ");
        column_defs.push(format!("PRIMARY KEY ({})", pk_csv));
    }

    let base_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n    {}\n)",
        table_name,
        column_defs.join(",\n    ")
    );

    let fk_fields: Vec<_> = belongs_to_fks.iter().map(|(f, _)| f.clone()).collect();
    let fk_models: Vec<_> = belongs_to_fks.iter().map(|(_, m)| m.clone()).collect();

    let hm_targets: Vec<_> = has_many_rels.iter().map(|(m, _)| m.clone()).collect();
    let hm_fks: Vec<_> = has_many_rels.iter().map(|(_, fk)| fk.clone()).collect();
    let fetch_hm_names: Vec<_> = hm_targets
        .iter()
        .map(|m| {
            syn::Ident::new(
                &format!("fetch_{}s", m.to_string().to_lowercase()),
                proc_macro2::Span::call_site(),
            )
        })
        .collect();
    let fetch_bt_names: Vec<_> = fk_fields
        .iter()
        .map(|f| {
            let fname = f.to_string();
            let base = fname.strip_suffix("_id").unwrap_or(&fname);
            syn::Ident::new(&format!("fetch_{}", base), proc_macro2::Span::call_site())
        })
        .collect();
    let first_pk = pk_fields[0].clone();
    let field_names_join = field_names_str.join(", ");
    let fields_indices: Vec<usize> = (0..columns.len()).collect();

    let expanded = quote! {
        impl chopin_orm::Model for #name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn create_table_stmt() -> String {
                let mut sql = String::from(#base_sql);
                #(
                    sql.pop(); // Remove closing parenthesis
                    sql.pop(); // Remove newline
                    let fk_constraint = format!(",\n    FOREIGN KEY ({}) REFERENCES {} (id)\n)", stringify!(#fk_fields), <#fk_models as chopin_orm::Model>::table_name());
                    sql.push_str(&fk_constraint);
                )*
                sql
            }

            fn column_definitions() -> Vec<(&'static str, &'static str)> {
                vec![
                    #( (#col_names, #col_types) ),*
                ]
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

            fn select_clause() -> &'static str {
                const COLS: &[&str] = &[#(#field_names_str),*];
                const JOINED: &str = #field_names_join;
                JOINED
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
                    #(self.#fields_list.to_sql()),*
                ]
            }

            fn set_generated_values(&mut self, mut values: Vec<chopin_pg::PgValue>) -> chopin_orm::OrmResult<()> {
                if values.len() != #gen_fields_len {
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
                        #fields_list: chopin_orm::ExtractValue::extract_at(row, #fields_indices)?,
                    )*
                })
            }
        }

        impl #name {
            #(
                pub fn #fetch_bt_names(&self, executor: &mut impl chopin_orm::Executor) -> chopin_orm::OrmResult<Option<#fk_models>> {
                    use chopin_pg::types::ToSql;
                    use chopin_pg::types::ToParam;
                    let qb = #fk_models::find().filter((
                        format!("{} = $1", <#fk_models as chopin_orm::Model>::primary_key_columns()[0]),
                        vec![self.#fk_fields.to_param()]
                    ));
                    qb.one(executor)
                }
            )*

            #(
                pub fn #fetch_hm_names(&self, executor: &mut impl chopin_orm::Executor) -> chopin_orm::OrmResult<Vec<#hm_targets>> {
                    use chopin_pg::types::ToSql;
                    use chopin_pg::types::ToParam;
                    let target_pk: chopin_pg::PgValue = self.#first_pk.clone().to_param();
                    let qb = #hm_targets::find().filter((
                        format!("{} = $1", #hm_fks),
                        vec![target_pk]
                    ));
                    qb.all(executor)
                }
            )*
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum #column_enum_name {
            #(#fields_list),*
        }

        impl chopin_orm::builder::ColumnTrait<#name> for #column_enum_name {
            fn column_name(&self) -> &'static str {
                match self {
                    #(Self::#fields_list => #field_names_str),*
                }
            }
        }
    };

    let active_expanded = quote! {};

    let mut belongs_to_field_names = Vec::new();
    let mut belongs_to_related_models = Vec::new();
    for (f, r) in &belongs_to_fks {
        belongs_to_field_names.push(f.clone());
        belongs_to_related_models.push(r.clone());
    }

    let final_expanded = quote! {
        #expanded
        #active_expanded

        #(
            impl chopin_orm::HasForeignKey<#belongs_to_related_models> for #name {
                fn foreign_key_info() -> (&'static str, Vec<(&'static str, &'static str)>) {
                    (<Self as chopin_orm::Model>::table_name(), vec![(stringify!(#belongs_to_field_names), "id")])
                }
            }
        )*
    };

    TokenStream::from(final_expanded)
}
