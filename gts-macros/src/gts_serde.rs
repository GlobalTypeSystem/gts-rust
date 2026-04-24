//! Serde-related code generation for `#[derive(GtsSchema)]`.
//!
//! Phase 3: Generates serde impls for generic base structs, GtsSerialize/GtsDeserialize
//! for nested structs, direct-serde blocking, and unit struct handling.

use proc_macro2::TokenStream;
use quote::quote;

use crate::extend_where_clause;
use crate::get_serde_rename;
use crate::gts_attrs::GtsAttrs;
use crate::gts_codegen::{SerdeDefault, SerdeFieldInfo};
use crate::gts_field_attrs::FieldGtsAttrs;

/// Generate all serde-related code.
pub fn generate(
    input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> TokenStream {
    let is_unit_struct =
        matches!(&input.data, syn::Data::Struct(ds) if matches!(&ds.fields, syn::Fields::Unit));
    let is_nested = attrs.extends.is_some();
    let has_generic = input.generics.type_params().count() > 0;

    let mut tokens = TokenStream::new();

    if is_nested {
        // Nested structs: GtsSerialize + GtsDeserialize + direct-serde blocking.
        // Blocking is absolute — nested structs must never produce standalone JSON
        // (they would omit the base envelope). Testing paths go through the base struct.
        if is_unit_struct {
            tokens.extend(gen_nested_unit_struct_serde(input));
        } else {
            tokens.extend(gen_nested_struct_serde(input, attrs, field_attrs));
        }
        tokens.extend(gen_no_direct_serde(input));
    } else if has_generic {
        // Root generic structs: custom Serialize/Deserialize impls
        if is_unit_struct {
            tokens.extend(gen_base_unit_struct_serde(input));
        } else {
            tokens.extend(gen_base_generic_serde(input, field_attrs));
        }
    } else if is_unit_struct {
        // Root non-generic unit structs: custom Serialize/Deserialize
        tokens.extend(gen_base_unit_struct_serde(input));
    }
    // Root non-generic, non-unit structs: user derives Serialize/Deserialize themselves.

    tokens
}

// ---------------------------------------------------------------------------
// Root generic struct: custom Serialize/Deserialize impls
// ---------------------------------------------------------------------------

#[allow(clippy::cognitive_complexity)]
fn gen_base_generic_serde(
    input: &syn::DeriveInput,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> TokenStream {
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

    // Use the struct's own generics (with existing bounds intact) + GtsSchema
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
    }

    let generic_param_name = input
        .generics
        .type_params()
        .next()
        .map(|tp| tp.ident.to_string())
        .expect("has_generic is true");

    let fields_info = collect_field_info(field_attrs, Some(&generic_param_name));
    let num_fields = fields_info.len();

    // Serialize impl
    let ser_field_calls: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let name = &f.serialize_name;
            if f.is_generic {
                quote! { state.serialize_field(#name, &::gts::GtsSerializeWrapper(&self.#ident))?; }
            } else {
                quote! { state.serialize_field(#name, &self.#ident)?; }
            }
        })
        .collect();

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let gp_ident: syn::Ident = syn::parse_str(&generic_param_name).expect("valid ident");

    // Serialize where: existing bounds + GtsSerialize
    let ser_where = extend_where_clause(
        where_clause,
        std::iter::once(syn::parse_quote!(#gp_ident: ::gts::GtsSerialize)),
    );

    let serialize_impl = quote! {
        impl #impl_generics serde::Serialize for #struct_name #ty_generics #ser_where {
            fn serialize<__S>(&self, serializer: __S) -> Result<__S::Ok, __S::Error>
            where
                __S: serde::Serializer,
            {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct(#struct_name_str, #num_fields)?;
                #(#ser_field_calls)*
                state.end()
            }
        }
    };

    // Deserialize impl
    let field_idents: Vec<_> = fields_info.iter().map(|f| &f.ident).collect();
    let field_names: Vec<_> = fields_info
        .iter()
        .map(|f| f.serialize_name.as_str())
        .collect();

    let field_visit_code: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let name = &f.serialize_name;
            if f.is_generic {
                quote! {
                    Field::#ident => {
                        if #ident.is_some() {
                            return Err(serde::de::Error::duplicate_field(#name));
                        }
                        let wrapper: ::gts::GtsDeserializeWrapper<_> = map.next_value()?;
                        #ident = Some(wrapper.0);
                    }
                }
            } else {
                quote! {
                    Field::#ident => {
                        if #ident.is_some() {
                            return Err(serde::de::Error::duplicate_field(#name));
                        }
                        #ident = Some(map.next_value()?);
                    }
                }
            }
        })
        .collect();

    let field_resolve_stmts: Vec<_> = fields_info.iter().map(resolve_field_stmt).collect();

    // Per-field serde rename for the field identifier enum
    let field_rename_attrs: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let name = &f.serialize_name;
            quote! { #[serde(rename = #name)] }
        })
        .collect();

    // Deserialize where: existing bounds + GtsDeserialize<'de>
    let de_where = extend_where_clause(
        where_clause,
        std::iter::once(syn::parse_quote!(#gp_ident: ::gts::GtsDeserialize<'de>)),
    );

    // For the StructVisitor, we need the struct's bounds as a where clause
    // (since ty_generics doesn't carry bounds)
    let gts_schema_where = build_visitor_where_clause(&generics, where_clause);

    // Extract type params with their bounds for impl<'de, ...>
    let type_params_with_bounds: Vec<_> = generics.type_params().collect();

    let deserialize_impl = quote! {
        impl<'de, #(#type_params_with_bounds),*> serde::Deserialize<'de> for #struct_name #ty_generics #de_where {
            fn deserialize<__D>(deserializer: __D) -> Result<Self, __D::Error>
            where
                __D: serde::Deserializer<'de>,
            {
                use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
                use std::fmt;

                #[allow(non_camel_case_types)]
                #[derive(serde::Deserialize)]
                #[serde(field_identifier)]
                enum Field {
                    #(#field_rename_attrs #field_idents,)*
                    #[serde(other)]
                    Unknown,
                }

                struct StructVisitor #ty_generics (std::marker::PhantomData<fn() -> #struct_name #ty_generics>) #gts_schema_where;

                impl<'de, #(#type_params_with_bounds),*> Visitor<'de> for StructVisitor #ty_generics #de_where {
                    type Value = #struct_name #ty_generics;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str(concat!("struct ", #struct_name_str))
                    }

                    fn visit_map<__M>(self, mut map: __M) -> Result<Self::Value, __M::Error>
                    where
                        __M: MapAccess<'de>,
                    {
                        #(let mut #field_idents: Option<_> = None;)*

                        while let Some(key) = map.next_key::<Field>()? {
                            match key {
                                #(#field_visit_code)*
                                Field::Unknown => {
                                    let _: serde::de::IgnoredAny = map.next_value()?;
                                }
                            }
                        }

                        #(#field_resolve_stmts)*

                        Ok(#struct_name {
                            #(#field_idents,)*
                        })
                    }
                }

                const FIELDS: &[&str] = &[#(#field_names,)*];
                deserializer.deserialize_struct(
                    #struct_name_str,
                    FIELDS,
                    StructVisitor(std::marker::PhantomData),
                )
            }
        }
    };

    quote! {
        #serialize_impl
        #deserialize_impl
    }
}

// ---------------------------------------------------------------------------
// Nested struct: GtsSerialize + GtsDeserialize
// ---------------------------------------------------------------------------

fn gen_nested_struct_serde(
    input: &syn::DeriveInput,
    _attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> TokenStream {
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
    }

    let generic_param_name = input
        .generics
        .type_params()
        .next()
        .map(|tp| tp.ident.to_string());

    let fields_info = collect_field_info(field_attrs, generic_param_name.as_ref());
    let num_fields = fields_info.len();

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // GtsSerialize
    let ser_field_calls: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let name = &f.serialize_name;
            if f.is_generic {
                quote! { state.serialize_field(#name, &::gts::GtsSerializeWrapper(&self.#ident))?; }
            } else {
                quote! { state.serialize_field(#name, &self.#ident)?; }
            }
        })
        .collect();

    let gts_serialize_where = build_where_clause_for_gts(
        &generics,
        where_clause,
        generic_param_name.as_ref(),
        "::gts::GtsSerialize",
    );

    let gts_serialize_impl = quote! {
        impl #impl_generics ::gts::GtsSerialize for #struct_name #ty_generics #gts_serialize_where {
            fn gts_serialize<__S>(&self, serializer: __S) -> Result<__S::Ok, __S::Error>
            where
                __S: serde::Serializer,
            {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct(#struct_name_str, #num_fields)?;
                #(#ser_field_calls)*
                state.end()
            }
        }
    };

    // GtsDeserialize
    let field_idents: Vec<_> = fields_info.iter().map(|f| &f.ident).collect();
    let field_names: Vec<_> = fields_info
        .iter()
        .map(|f| f.serialize_name.as_str())
        .collect();

    let field_visit_code: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let name = &f.serialize_name;
            if f.is_generic {
                quote! {
                    Field::#ident => {
                        if #ident.is_some() {
                            return Err(serde::de::Error::duplicate_field(#name));
                        }
                        let wrapper: ::gts::GtsDeserializeWrapper<_> = map.next_value()?;
                        #ident = Some(wrapper.0);
                    }
                }
            } else {
                quote! {
                    Field::#ident => {
                        if #ident.is_some() {
                            return Err(serde::de::Error::duplicate_field(#name));
                        }
                        #ident = Some(map.next_value()?);
                    }
                }
            }
        })
        .collect();

    let field_resolve_stmts: Vec<_> = fields_info.iter().map(resolve_field_stmt).collect();

    // Per-field serde rename for the field identifier enum (fixes the rename_all bug)
    let field_rename_attrs: Vec<_> = fields_info
        .iter()
        .map(|f| {
            let name = &f.serialize_name;
            quote! { #[serde(rename = #name)] }
        })
        .collect();

    let type_param_idents: Vec<_> = generics.type_params().map(|p| &p.ident).collect();
    let de_impl_generics = if type_param_idents.is_empty() {
        quote! { 'de }
    } else {
        quote! { 'de, #(#type_param_idents: ::gts::GtsSchema),* }
    };

    let gts_deserialize_where = build_where_clause_for_gts(
        &generics,
        where_clause,
        generic_param_name.as_ref(),
        "::gts::GtsDeserialize<'de>",
    );

    let gts_schema_where =
        build_where_clause_for_gts(&generics, where_clause, generic_param_name.as_ref(), "");

    let gts_deserialize_impl = quote! {
        impl<#de_impl_generics> ::gts::GtsDeserialize<'de> for #struct_name #ty_generics #gts_deserialize_where {
            fn gts_deserialize<__D>(deserializer: __D) -> Result<Self, __D::Error>
            where
                __D: serde::Deserializer<'de>,
            {
                use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
                use std::fmt;

                #[allow(non_camel_case_types)]
                #[derive(serde::Deserialize)]
                #[serde(field_identifier)]
                enum Field {
                    #(#field_rename_attrs #field_idents,)*
                    #[serde(other)]
                    Unknown,
                }

                struct StructVisitor #ty_generics (std::marker::PhantomData<fn() -> #struct_name #ty_generics>) #gts_schema_where;

                impl<#de_impl_generics> Visitor<'de> for StructVisitor #ty_generics #gts_deserialize_where {
                    type Value = #struct_name #ty_generics;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str(concat!("struct ", #struct_name_str))
                    }

                    fn visit_map<__M>(self, mut map: __M) -> Result<Self::Value, __M::Error>
                    where
                        __M: MapAccess<'de>,
                    {
                        #(let mut #field_idents: Option<_> = None;)*

                        while let Some(key) = map.next_key::<Field>()? {
                            match key {
                                #(#field_visit_code)*
                                Field::Unknown => {
                                    let _: serde::de::IgnoredAny = map.next_value()?;
                                }
                            }
                        }

                        #(#field_resolve_stmts)*

                        Ok(#struct_name {
                            #(#field_idents,)*
                        })
                    }
                }

                const FIELDS: &[&str] = &[#(#field_names,)*];
                deserializer.deserialize_struct(
                    #struct_name_str,
                    FIELDS,
                    StructVisitor(std::marker::PhantomData),
                )
            }
        }
    };

    quote! {
        #gts_serialize_impl
        #gts_deserialize_impl
    }
}

// ---------------------------------------------------------------------------
// Unit struct serde impls
// ---------------------------------------------------------------------------

fn gen_base_unit_struct_serde(input: &syn::DeriveInput) -> TokenStream {
    let struct_name = &input.ident;
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics serde::Serialize for #struct_name #ty_generics #where_clause {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(0))?;
                map.end()
            }
        }

        impl<'de> serde::Deserialize<'de> for #struct_name #where_clause {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                use serde::de::{Visitor, MapAccess};
                use std::fmt;

                struct UnitStructVisitor;

                impl<'de> Visitor<'de> for UnitStructVisitor {
                    type Value = #struct_name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("unit struct")
                    }

                    fn visit_map<M>(self, _map: M) -> Result<Self::Value, M::Error>
                    where
                        M: MapAccess<'de>,
                    {
                        Ok(#struct_name)
                    }

                    fn visit_unit<E>(self) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(#struct_name)
                    }
                }

                deserializer.deserialize_any(UnitStructVisitor)
            }
        }
    }
}

fn gen_nested_unit_struct_serde(input: &syn::DeriveInput) -> TokenStream {
    let struct_name = &input.ident;

    quote! {
        impl ::gts::GtsSerialize for #struct_name {
            fn gts_serialize<__S>(&self, serializer: __S) -> Result<__S::Ok, __S::Error>
            where
                __S: serde::Serializer,
            {
                use serde::ser::SerializeMap;
                let map = serializer.serialize_map(Some(0))?;
                map.end()
            }
        }

        impl<'de> ::gts::GtsDeserialize<'de> for #struct_name {
            fn gts_deserialize<__D>(deserializer: __D) -> Result<Self, __D::Error>
            where
                __D: serde::Deserializer<'de>,
            {
                use serde::de::{Deserializer, MapAccess, Visitor};
                use std::fmt;

                struct UnitVisitor;

                impl<'de> Visitor<'de> for UnitVisitor {
                    type Value = #struct_name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("unit struct")
                    }

                    fn visit_map<__M>(self, _map: __M) -> Result<Self::Value, __M::Error>
                    where
                        __M: MapAccess<'de>,
                    {
                        Ok(#struct_name)
                    }

                    fn visit_unit<__E>(self) -> Result<Self::Value, __E>
                    where
                        __E: serde::de::Error,
                    {
                        Ok(#struct_name)
                    }
                }

                deserializer.deserialize_any(UnitVisitor)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Direct serde blocking for nested structs
// ---------------------------------------------------------------------------

fn gen_no_direct_serde(input: &syn::DeriveInput) -> TokenStream {
    let struct_name = &input.ident;
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::gts::GtsNoDirectSerialize for #struct_name #ty_generics #where_clause {}
        impl #impl_generics ::gts::GtsNoDirectDeserialize for #struct_name #ty_generics #where_clause {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect field info needed for serialize/deserialize code generation.
fn collect_field_info(
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
    generic_param_name: Option<&String>,
) -> Vec<SerdeFieldInfo> {
    field_attrs
        .iter()
        .filter_map(|(field, _)| {
            let ident = field.ident.as_ref()?;
            let serialize_name = get_serde_rename(field).unwrap_or_else(|| ident.to_string());
            let is_generic = generic_param_name.is_some_and(|gp| {
                let field_type = &field.ty;
                let field_type_str = quote::quote!(#field_type).to_string().replace(' ', "");
                field_type_str == *gp
            });
            let default = detect_serde_default(field, is_option_type(&field.ty));
            Some(SerdeFieldInfo {
                ident: ident.clone(),
                serialize_name,
                is_generic,
                default,
            })
        })
        .collect()
}

/// True if the outermost type is `Option` (the common single-segment form).
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(tp) = ty
        && tp.qself.is_none()
        && let Some(last) = tp.path.segments.last()
    {
        return last.ident == "Option";
    }
    false
}

/// Determine how a missing field should be handled during deserialization.
///
/// Mirrors serde's resolution: `Option<T>` defaults to `None`, `#[serde(default)]`
/// falls back to `T::default()`, and `#[serde(default = "path")]` calls `path()`.
fn detect_serde_default(field: &syn::Field, option_type: bool) -> SerdeDefault {
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let Ok(metas) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        ) else {
            continue;
        };
        for meta in &metas {
            match meta {
                syn::Meta::Path(p) if p.is_ident("default") => return SerdeDefault::Plain,
                syn::Meta::NameValue(nv) if nv.path.is_ident("default") => {
                    if let syn::Expr::Lit(el) = &nv.value
                        && let syn::Lit::Str(s) = &el.lit
                        && let Ok(path) = s.parse::<syn::Path>()
                    {
                        return SerdeDefault::Named(path);
                    }
                }
                _ => {}
            }
        }
    }
    if option_type {
        SerdeDefault::OptionType
    } else {
        SerdeDefault::None
    }
}

/// Produce the per-field statement that resolves an `Option<_>` accumulator into the
/// final field value, honoring serde's missing-field semantics.
fn resolve_field_stmt(info: &SerdeFieldInfo) -> TokenStream {
    let ident = &info.ident;
    let name = info.serialize_name.as_str();
    match &info.default {
        SerdeDefault::None => quote! {
            let #ident = #ident
                .ok_or_else(|| serde::de::Error::missing_field(#name))?;
        },
        SerdeDefault::OptionType => quote! {
            let #ident = #ident.flatten();
        },
        SerdeDefault::Plain => quote! {
            let #ident = #ident.unwrap_or_default();
        },
        SerdeDefault::Named(path) => quote! {
            let #ident = #ident.unwrap_or_else(#path);
        },
    }
}

/// Build a where clause that carries all type parameter bounds from the generics.
/// This is needed for struct definitions inside impl blocks where `ty_generics`
/// doesn't carry bounds (e.g., `StructVisitor`).
fn build_visitor_where_clause(
    generics: &syn::Generics,
    existing_where: Option<&syn::WhereClause>,
) -> TokenStream {
    let new_predicates: Vec<syn::WherePredicate> = generics
        .type_params()
        .map(|tp| {
            let ident = &tp.ident;
            let bounds = &tp.bounds;
            syn::parse_quote!(#ident: #bounds)
        })
        .collect();
    extend_where_clause(existing_where, new_predicates)
}

/// Build a where clause with `GtsSchema` + extra bound on the generic param.
fn build_where_clause_for_gts(
    _generics: &syn::Generics,
    where_clause: Option<&syn::WhereClause>,
    generic_param_name: Option<&String>,
    extra_bound: &str,
) -> TokenStream {
    let Some(gp) = generic_param_name else {
        return quote! { #where_clause };
    };
    let gp_ident: syn::Ident = syn::parse_str(gp).expect("valid ident");
    let predicate: syn::WherePredicate = if extra_bound.is_empty() {
        syn::parse_quote!(#gp_ident: ::gts::GtsSchema)
    } else {
        let extra: proc_macro2::TokenStream = extra_bound.parse().expect("valid bound");
        syn::parse_quote!(#gp_ident: ::gts::GtsSchema + #extra)
    };
    extend_where_clause(where_clause, std::iter::once(predicate))
}
