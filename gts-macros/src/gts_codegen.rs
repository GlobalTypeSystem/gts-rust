//! Code generation for `#[derive(GtsSchema)]`.
//!
//! Phase 2: Generates the `GtsSchema` trait implementation, runtime API methods,
//! associated constants, and compile-time assertions.

use proc_macro2::TokenStream;
use quote::quote;

use crate::gts_attrs::GtsAttrs;
use crate::gts_field_attrs::{FieldGtsAttrs, GtsFieldAttr};
use crate::{build_where_clause, extract_parent_schema_id, get_serde_rename};

/// How the generated `visit_map` should treat a missing field — matches serde's defaults.
pub enum SerdeDefault {
    /// Field is required; emit `missing_field(...)` on absence.
    None,
    /// `Option<T>` field — absence becomes `None`.
    OptionType,
    /// `#[serde(default)]` — absence becomes `T::default()`.
    Plain,
    /// `#[serde(default = "path")]` — absence becomes `path()`.
    Named(syn::Path),
}

/// Field info for serde code generation.
pub struct SerdeFieldInfo {
    pub ident: syn::Ident,
    pub serialize_name: String,
    pub is_generic: bool,
    pub default: SerdeDefault,
}

/// Generate all code for a `#[derive(GtsSchema)]` annotated struct.
pub fn generate(
    input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> TokenStream {
    let info = GenericsInfo::from_derive_input(input, field_attrs);
    let expected_parent_id = extract_parent_schema_id(&attrs.schema_id);

    let constants = gen_constants(attrs, field_attrs, &info, expected_parent_id.as_ref());
    let base_assertion = gen_base_assertion(attrs, expected_parent_id.as_ref());
    let trait_impl = gen_gts_schema_trait_impl(input, attrs, field_attrs, &info);
    let runtime_api = gen_runtime_api(attrs, &info, expected_parent_id.as_ref());
    let schema_string_methods = gen_schema_string_methods(&info);
    let instance_methods = gen_instance_methods(attrs, &info);
    let constructor = gen_constructor(input, field_attrs, &info);

    let struct_name = &info.struct_name;
    let (impl_generics, ty_generics, _) = info.generics.split_for_impl();
    let gts_schema_where = &info.gts_schema_where;

    quote! {
        #base_assertion

        impl #impl_generics #struct_name #ty_generics #gts_schema_where {
            #constants
            #runtime_api
            #schema_string_methods
        }

        #trait_impl

        #instance_methods

        #constructor
    }
}

// ---------------------------------------------------------------------------
// Generated constructor: `pub fn new(...) -> Self`
// ---------------------------------------------------------------------------

/// Generate `pub fn new(...) -> Self` on the struct.
///
/// Behavior (per ADR §Generated Constructor):
///
/// - Named structs with fields get a constructor; unit structs and tuple structs do not.
/// - The constructor's parameters are all named fields in struct-definition order,
///   **except** any field annotated `#[gts(type_field)]` — that field is auto-populated.
/// - For a generic root struct, the `type_field` is populated from
///   `<P as ::gts::GtsSchema>::gts_schema_id().clone()` so the generic specializes
///   to the child's chained identifier.
/// - For a non-generic root struct, the `type_field` is populated from
///   `Self::gts_schema_id().clone()`.
/// - `#[gts(instance_id)]` fields are passed by the caller (the macro cannot synthesize
///   an instance segment); they appear in the constructor signature like any other field.
/// - Derived structs have no identity field and get a straightforward all-fields constructor.
/// - `#[gts(skip)]` is a schema-only annotation; `#[serde(skip)]` is a serde concern.
///   Neither affects the constructor: every field is still part of the struct's data model.
fn gen_constructor(
    input: &syn::DeriveInput,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
    info: &GenericsInfo,
) -> TokenStream {
    // Only generate for named structs with at least one field.
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) if !named.named.is_empty() => &named.named,
            _ => return TokenStream::new(),
        },
        _ => return TokenStream::new(),
    };

    let type_field_ident: Option<syn::Ident> = field_attrs
        .iter()
        .find(|(_, fa)| matches!(fa.attr, Some(GtsFieldAttr::TypeField)))
        .and_then(|(f, _)| f.ident.clone());

    let struct_name = &info.struct_name;
    let (impl_generics, ty_generics, _) = info.generics.split_for_impl();
    let gts_schema_where = &info.gts_schema_where;

    // Build parameter list (all fields except the auto-populated type_field)
    // and the field-init list that constructs Self.
    let mut params = Vec::new();
    let mut inits = Vec::new();

    for field in fields {
        let Some(ident) = field.ident.as_ref() else {
            continue;
        };
        let ty = &field.ty;

        if type_field_ident.as_ref() == Some(ident) {
            // Auto-populate the type_field. For a generic root, read the trait's
            // SCHEMA_ID from the generic parameter so the outermost event specializes
            // to the child's chained identifier. For a non-generic root, reuse the
            // inherent `gts_schema_id()` accessor (cached via LazyLock).
            let populator = if let Some(generic_param) = &info.generic_param_name {
                let generic_ident = syn::Ident::new(generic_param, proc_macro2::Span::call_site());
                quote! {
                    ::gts::gts::GtsSchemaId::new(
                        <#generic_ident as ::gts::GtsSchema>::SCHEMA_ID
                    )
                }
            } else {
                quote! { Self::gts_schema_id().clone() }
            };
            inits.push(quote! { #ident: #populator });
        } else {
            params.push(quote! { #ident: #ty });
            inits.push(quote! { #ident });
        }
    }

    quote! {
        impl #impl_generics #struct_name #ty_generics #gts_schema_where {
            /// Construct an instance of this struct.
            ///
            /// If this struct declares `#[gts(type_field)]`, that field is auto-populated
            /// from the GTS schema id (of the generic parameter for a generic struct,
            /// or of `Self` otherwise) — callers never hand-assign it. All other fields
            /// are passed in positionally in struct-definition order.
            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#params),*) -> Self {
                Self {
                    #(#inits,)*
                }
            }
        }
    }
}

/// Pre-computed generics information for code generation.
struct GenericsInfo {
    struct_name: syn::Ident,
    /// Generics with `GtsSchema` bounds added to type params.
    generics: syn::Generics,
    has_generic: bool,
    generic_param_name: Option<String>,
    /// The serialized name of the field that uses the generic type parameter.
    generic_field_name: Option<String>,
    /// Where clause for impls requiring `GtsSchema + JsonSchema` bounds.
    gts_schema_where: TokenStream,
    /// Where clause for impls requiring `GtsSerialize + GtsSchema` bounds (Phase 3).
    #[allow(dead_code)]
    serialize_where: TokenStream,
}

impl GenericsInfo {
    fn from_derive_input(
        input: &syn::DeriveInput,
        field_attrs: &[(syn::Field, FieldGtsAttrs)],
    ) -> Self {
        let struct_name = input.ident.clone();

        // Clone generics and add GtsSchema bound to type params
        let mut generics = input.generics.clone();
        for param in generics.type_params_mut() {
            param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
        }

        let has_generic = input.generics.type_params().count() > 0;

        let generic_param_name: Option<String> = input
            .generics
            .type_params()
            .next()
            .map(|tp| tp.ident.to_string());

        // Find the field whose type matches the generic parameter, and get its serialized name
        let generic_field_name = generic_param_name.as_ref().and_then(|gp| {
            field_attrs.iter().find_map(|(field, _)| {
                let field_type = &field.ty;
                let field_type_str = quote::quote!(#field_type).to_string().replace(' ', "");
                if field_type_str == *gp {
                    let ident = field.ident.as_ref()?;
                    Some(get_serde_rename(field).unwrap_or_else(|| ident.to_string()))
                } else {
                    None
                }
            })
        });

        let (_, _, where_clause) = generics.split_for_impl();
        let gts_schema_where = build_where_clause(
            &generics,
            where_clause,
            "::gts::GtsSchema + ::schemars::JsonSchema",
        );
        let serialize_where = build_where_clause(
            &generics,
            where_clause,
            "::gts::GtsSerialize + ::gts::GtsSchema",
        );

        GenericsInfo {
            struct_name,
            generics,
            has_generic,
            generic_param_name,
            generic_field_name,
            gts_schema_where,
            serialize_where,
        }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

fn gen_constants(
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
    _info: &GenericsInfo,
    expected_parent_id: Option<&String>,
) -> TokenStream {
    let schema_file_path = format!("{}/{}.schema.json", attrs.dir_path, attrs.schema_id);
    let description = &attrs.description;
    let properties_str = compute_properties(field_attrs);

    let base_schema_id_const = if let Some(parent_id) = expected_parent_id {
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            const BASE_SCHEMA_ID: Option<&'static str> = Some(#parent_id);
        }
    } else {
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            const BASE_SCHEMA_ID: Option<&'static str> = None;
        }
    };

    quote! {
        #[doc(hidden)]
        #[allow(dead_code)]
        const GTS_SCHEMA_FILE_PATH: &'static str = #schema_file_path;

        #[doc(hidden)]
        #[allow(dead_code)]
        const GTS_SCHEMA_DESCRIPTION: &'static str = #description;

        #[doc(hidden)]
        #[allow(dead_code)]
        const GTS_SCHEMA_PROPERTIES: &'static str = #properties_str;

        #base_schema_id_const
    }
}

/// Compute the properties string from struct fields, excluding `#[gts(skip)]` and `#[serde(skip)]`.
fn compute_properties(field_attrs: &[(syn::Field, FieldGtsAttrs)]) -> String {
    field_attrs
        .iter()
        .filter(|(field, attrs)| {
            !matches!(attrs.attr, Some(GtsFieldAttr::Skip)) && !has_serde_skip(field)
        })
        .filter_map(|(field, _)| {
            let ident = field.ident.as_ref()?;
            Some(get_serde_rename(field).unwrap_or_else(|| ident.to_string()))
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Check if a field has `#[serde(skip)]`, `#[serde(skip_serializing)]`, or `#[serde(skip_deserializing)]`.
///
/// Matches the exact skip directives as `Meta::Path` entries so that attributes like
/// `#[serde(skip_serializing_if = "...")]` (which is a `Meta::NameValue`) are not
/// incorrectly treated as skips.
fn has_serde_skip(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let Ok(metas) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        ) else {
            return false;
        };
        metas.iter().any(|m| {
            matches!(m, syn::Meta::Path(p)
                if p.is_ident("skip")
                    || p.is_ident("skip_serializing")
                    || p.is_ident("skip_deserializing"))
        })
    })
}

// ---------------------------------------------------------------------------
// Compile-time assertions
// ---------------------------------------------------------------------------

fn gen_base_assertion(attrs: &GtsAttrs, expected_parent_id: Option<&String>) -> TokenStream {
    let Some(parent_ident) = &attrs.extends else {
        return quote! {};
    };

    let parent_id = expected_parent_id
        .as_ref()
        .expect("parent_id must exist when extends is specified");

    let schema_id_msg = format!(
        "GtsSchema: parent struct '{parent_ident}' schema ID must match parent segment \
         '{parent_id}' from schema_id"
    );
    let generic_field_msg = format!(
        "GtsSchema: parent struct '{parent_ident}' must have exactly 1 generic field. \
         Parent types must define a generic field (e.g., `pub payload: P`) that child types extend."
    );

    quote! {
        const _: () = {
            const PARENT_ID: &'static str = <#parent_ident<()> as ::gts::GtsSchema>::SCHEMA_ID;
            const EXPECTED_ID: &'static str = #parent_id;
            const _: () = {
                if PARENT_ID.as_bytes().len() != EXPECTED_ID.as_bytes().len() {
                    panic!(#schema_id_msg);
                }
                let mut i = 0;
                while i < PARENT_ID.as_bytes().len() {
                    if PARENT_ID.as_bytes()[i] != EXPECTED_ID.as_bytes()[i] {
                        panic!(#schema_id_msg);
                    }
                    i += 1;
                }
            };
        };

        const _: () = {
            const PARENT_GENERIC_FIELD: Option<&'static str> =
                <#parent_ident<()> as ::gts::GtsSchema>::GENERIC_FIELD;
            if PARENT_GENERIC_FIELD.is_none() {
                panic!(#generic_field_msg);
            }
        };
    }
}

// ---------------------------------------------------------------------------
// GtsSchema trait implementation
// ---------------------------------------------------------------------------

fn gen_gts_schema_trait_impl(
    input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
    info: &GenericsInfo,
) -> TokenStream {
    let struct_name = &info.struct_name;
    let schema_id = &attrs.schema_id;
    let (impl_generics, ty_generics, _) = info.generics.split_for_impl();
    let gts_schema_where = &info.gts_schema_where;

    let generic_field_option = if let Some(ref field_name) = info.generic_field_name {
        quote! { Some(#field_name) }
    } else {
        quote! { None }
    };

    // Collect identity field names for x-gts-ref override
    let identity_fields = collect_identity_field_names(field_attrs);

    let schema_methods = if info.has_generic {
        gen_generic_schema_methods(input, attrs, &identity_fields, info)
    } else {
        gen_non_generic_schema_methods(attrs, &identity_fields, info)
    };

    quote! {
        impl #impl_generics ::gts::GtsSchema for #struct_name #ty_generics #gts_schema_where {
            const SCHEMA_ID: &'static str = #schema_id;
            const GENERIC_FIELD: Option<&'static str> = #generic_field_option;

            fn gts_schema_with_refs() -> serde_json::Value {
                Self::gts_schema_with_refs_allof()
            }

            #schema_methods
        }
    }
}

/// Collect the serialized names of fields annotated with `#[gts(type_field)]` or `#[gts(instance_id)]`.
fn collect_identity_field_names(field_attrs: &[(syn::Field, FieldGtsAttrs)]) -> Vec<String> {
    field_attrs
        .iter()
        .filter_map(|(field, attrs)| match attrs.attr {
            Some(GtsFieldAttr::TypeField | GtsFieldAttr::InstanceId) => {
                let ident = field.ident.as_ref()?;
                Some(get_serde_rename(field).unwrap_or_else(|| ident.to_string()))
            }
            _ => None,
        })
        .collect()
}

/// Generate `x-gts-ref: "/$id"` override code for identity fields.
fn gen_identity_field_override(identity_fields: &[String]) -> TokenStream {
    if identity_fields.is_empty() {
        return quote! {};
    }

    let field_names = identity_fields.iter().map(String::as_str);

    quote! {
        // Override x-gts-ref on identity fields from "gts.*" to "/$id"
        if let Some(props_obj) = properties.as_object_mut() {
            const IDENTITY_FIELDS: &[&str] = &[#(#field_names),*];
            for field_name in IDENTITY_FIELDS {
                if let Some(prop) = props_obj.get_mut(*field_name) {
                    if let Some(obj) = prop.as_object_mut() {
                        obj.insert("x-gts-ref".to_owned(), serde_json::json!("/$id"));
                    }
                }
            }
        }
    }
}

fn gen_generic_schema_methods(
    _input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    identity_fields: &[String],
    info: &GenericsInfo,
) -> TokenStream {
    let generic_ident: syn::Ident = syn::parse_str(
        info.generic_param_name
            .as_ref()
            .expect("has_generic is true"),
    )
    .expect("valid ident");
    let generic_field_for_path = info.generic_field_name.as_deref().unwrap_or_default();
    let description = &attrs.description;
    let identity_override = gen_identity_field_override(identity_fields);

    quote! {
        fn gts_schema() -> serde_json::Value {
            Self::gts_schema_with_refs()
        }

        fn innermost_schema_id() -> &'static str {
            let inner_id = <#generic_ident as ::gts::GtsSchema>::innermost_schema_id();
            if inner_id.is_empty() {
                Self::SCHEMA_ID
            } else {
                inner_id
            }
        }

        fn innermost_schema() -> serde_json::Value {
            let inner = <#generic_ident as ::gts::GtsSchema>::innermost_schema();
            if inner.get("properties").is_none() {
                let root_schema = schemars::schema_for!(Self);
                return serde_json::to_value(&root_schema).expect("schemars");
            }
            inner
        }

        fn collect_nesting_path() -> Vec<&'static str> {
            let inner_path = <#generic_ident as ::gts::GtsSchema>::collect_nesting_path();
            let inner_id = <#generic_ident as ::gts::GtsSchema>::SCHEMA_ID;

            if inner_id.is_empty() {
                return Vec::new();
            }

            let mut path = Vec::new();
            let field = #generic_field_for_path;
            if !field.is_empty() {
                path.push(field);
            }
            path.extend(inner_path);
            path
        }

        fn gts_schema_with_refs_allof() -> serde_json::Value {
            let schema_id = Self::SCHEMA_ID;

            let parent_schema_id = if schema_id.contains('~') {
                let s = schema_id.trim_end_matches('~');
                if let Some(pos) = s.rfind('~') {
                    format!("{}~", &s[..pos])
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let root_schema = schemars::schema_for!(Self);
            let schema_val = serde_json::to_value(&root_schema).expect("schemars");
            let mut properties = schema_val.get("properties").cloned().unwrap_or(serde_json::json!({}));
            let required = schema_val.get("required").cloned().unwrap_or(serde_json::json!([]));

            // Resolve $ref for GtsInstanceId/GtsSchemaId
            if let Some(props_obj) = properties.as_object_mut() {
                for (_key, value) in props_obj.iter_mut() {
                    if let Some(ref_str) = value.get("$ref").and_then(|v| v.as_str()) {
                        if ref_str == "#/$defs/GtsInstanceId" {
                            *value = gts::GtsInstanceId::json_schema_value();
                        } else if ref_str == "#/$defs/GtsSchemaId" {
                            *value = gts::GtsSchemaId::json_schema_value();
                        }
                    }
                }
            }

            // Replace the generic field with a placeholder
            if let Some(generic_field) = Self::GENERIC_FIELD {
                if let Some(props) = properties.as_object_mut() {
                    if props.contains_key(generic_field) {
                        props.insert(generic_field.to_owned(), serde_json::json!({"type": "object"}));
                    }
                }
            }

            #identity_override

            if parent_schema_id.is_empty() {
                let mut schema = serde_json::json!({
                    "$id": format!("gts://{}", schema_id),
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "description": #description,
                    "type": "object",
                    "additionalProperties": false,
                    "properties": properties
                });
                if !required.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                    schema["required"] = required;
                }
                return schema;
            }

            let nesting_path = Self::collect_nesting_path();
            let innermost_generic_field = <#generic_ident as ::gts::GtsSchema>::GENERIC_FIELD;
            let nested_properties = Self::wrap_in_nesting_path(&nesting_path, properties, required.clone(), innermost_generic_field);

            serde_json::json!({
                "$id": format!("gts://{}", schema_id),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "description": #description,
                "type": "object",
                "additionalProperties": false,
                "allOf": [
                    { "$ref": format!("gts://{}", parent_schema_id) },
                    {
                        "type": "object",
                        "properties": nested_properties
                    }
                ]
            })
        }
    }
}

fn gen_non_generic_schema_methods(
    attrs: &GtsAttrs,
    identity_fields: &[String],
    _info: &GenericsInfo,
) -> TokenStream {
    let description = &attrs.description;
    let identity_override = gen_identity_field_override(identity_fields);

    let parent_generic_field_code = if let Some(parent_ident) = &attrs.extends {
        quote! {
            let parent_generic_field: Option<&'static str> =
                <#parent_ident<()> as ::gts::GtsSchema>::GENERIC_FIELD;
        }
    } else {
        quote! {
            let parent_generic_field: Option<&'static str> = None;
        }
    };

    quote! {
        fn gts_schema() -> serde_json::Value {
            Self::gts_schema_with_refs()
        }

        fn innermost_schema_id() -> &'static str {
            Self::SCHEMA_ID
        }

        fn innermost_schema() -> serde_json::Value {
            let root_schema = schemars::schema_for!(Self);
            serde_json::to_value(&root_schema).expect("schemars")
        }

        fn gts_schema_with_refs_allof() -> serde_json::Value {
            let schema_id = Self::SCHEMA_ID;

            let parent_schema_id = if schema_id.contains('~') {
                let s = schema_id.trim_end_matches('~');
                if let Some(pos) = s.rfind('~') {
                    format!("{}~", &s[..pos])
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let root_schema = schemars::schema_for!(Self);
            let schema_val = serde_json::to_value(&root_schema).expect("schemars");
            let mut properties = schema_val.get("properties").cloned().unwrap_or_else(|| serde_json::json!({}));
            let required = schema_val.get("required").cloned().unwrap_or_else(|| serde_json::json!([]));

            // Resolve $ref for GtsInstanceId/GtsSchemaId
            if let Some(props_obj) = properties.as_object_mut() {
                for (_key, value) in props_obj.iter_mut() {
                    if let Some(ref_str) = value.get("$ref").and_then(|v| v.as_str()) {
                        if ref_str == "#/$defs/GtsInstanceId" {
                            *value = gts::GtsInstanceId::json_schema_value();
                        } else if ref_str == "#/$defs/GtsSchemaId" {
                            *value = gts::GtsSchemaId::json_schema_value();
                        }
                    }
                }
            }

            #identity_override

            if parent_schema_id.is_empty() {
                let mut schema = serde_json::json!({
                    "$id": format!("gts://{}", schema_id),
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "description": #description,
                    "type": "object",
                    "additionalProperties": false,
                    "properties": properties
                });
                if !required.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                    schema["required"] = required;
                }
                return schema;
            }

            #parent_generic_field_code

            let field_name = parent_generic_field
                .expect("Parent struct must have a generic field for derived types to extend");

            let nested_properties = Self::wrap_in_nesting_path(&[field_name], properties, required, None);
            serde_json::json!({
                "$id": format!("gts://{}", schema_id),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "description": #description,
                "type": "object",
                "additionalProperties": false,
                "allOf": [
                    { "$ref": format!("gts://{}", parent_schema_id) },
                    {
                        "type": "object",
                        "properties": nested_properties
                    }
                ]
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime API methods
// ---------------------------------------------------------------------------

fn gen_runtime_api(
    attrs: &GtsAttrs,
    _info: &GenericsInfo,
    expected_parent_id: Option<&String>,
) -> TokenStream {
    let schema_id = &attrs.schema_id;

    let base_schema_id_option = if let Some(parent_id) = expected_parent_id {
        quote! { Some(#parent_id) }
    } else {
        quote! { None::<&'static str> }
    };

    quote! {
        /// Get the GTS schema identifier as a static reference.
        #[allow(dead_code)]
        #[must_use]
        pub fn gts_schema_id() -> &'static ::gts::gts::GtsSchemaId {
            static GTS_SCHEMA_ID: std::sync::LazyLock<::gts::gts::GtsSchemaId> =
                std::sync::LazyLock::new(|| ::gts::gts::GtsSchemaId::new(#schema_id));
            &GTS_SCHEMA_ID
        }

        /// Get the parent (base) schema identifier as a static reference.
        /// Returns `None` for root types (no `extends`).
        #[allow(dead_code)]
        #[must_use]
        pub fn gts_base_schema_id() -> Option<&'static ::gts::gts::GtsSchemaId> {
            static BASE_SCHEMA_ID: std::sync::LazyLock<Option<::gts::gts::GtsSchemaId>> =
                std::sync::LazyLock::new(|| {
                    #base_schema_id_option.map(::gts::gts::GtsSchemaId::new)
                });
            BASE_SCHEMA_ID.as_ref()
        }

        /// Generate a GTS instance ID by appending a segment to the schema ID.
        #[allow(dead_code)]
        #[must_use]
        pub fn gts_make_instance_id(segment: &str) -> ::gts::GtsInstanceId {
            ::gts::GtsInstanceId::new(#schema_id, segment)
        }
    }
}

fn gen_schema_string_methods(_info: &GenericsInfo) -> TokenStream {
    quote! {
        /// Get the JSON Schema with `allOf` + `$ref` for inheritance as a JSON string.
        #[allow(dead_code)]
        #[must_use]
        pub fn gts_schema_with_refs_as_string() -> String {
            use ::gts::GtsSchema;
            serde_json::to_string(&Self::gts_schema_with_refs_allof())
                .expect("Failed to serialize schema")
        }

        /// Get the JSON Schema with `allOf` + `$ref` for inheritance as a pretty-printed JSON string.
        #[allow(dead_code)]
        #[must_use]
        pub fn gts_schema_with_refs_as_string_pretty() -> String {
            use ::gts::GtsSchema;
            serde_json::to_string_pretty(&Self::gts_schema_with_refs_allof())
                .expect("Failed to serialize schema")
        }
    }
}

// ---------------------------------------------------------------------------
// Instance serialization methods
// ---------------------------------------------------------------------------

fn gen_instance_methods(attrs: &GtsAttrs, info: &GenericsInfo) -> TokenStream {
    // Nested structs (extends) don't get instance methods — they serialize through their base
    if attrs.extends.is_some() {
        return quote! {};
    }

    let struct_name = &info.struct_name;
    let (impl_generics, ty_generics, where_clause) = info.generics.split_for_impl();

    // Use `where Self: serde::Serialize` so methods are available only when
    // the concrete type is Serialize (regardless of generic param bounds).
    let instance_where = if let Some(existing) = where_clause {
        quote! { #existing Self: serde::Serialize, }
    } else {
        quote! { where Self: serde::Serialize }
    };

    quote! {
        impl #impl_generics #struct_name #ty_generics #instance_where {
            /// Serialize this instance to a `serde_json::Value`.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_instance_json(&self) -> serde_json::Value {
                serde_json::to_value(self).expect("Failed to serialize instance to JSON")
            }

            /// Serialize this instance to a JSON string.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_instance_json_as_string(&self) -> String {
                serde_json::to_string(self).expect("Failed to serialize instance to JSON string")
            }

            /// Serialize this instance to a pretty-printed JSON string.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_instance_json_as_string_pretty(&self) -> String {
                serde_json::to_string_pretty(self)
                    .expect("Failed to serialize instance to JSON string")
            }
        }
    }
}
