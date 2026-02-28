// Proc macros run at compile time, so panics become compile errors
#![allow(clippy::expect_used, clippy::unwrap_used)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Fields, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

// Field name constants to avoid duplication
const ID_FIELD_NAMES: &[&str] = &["$id", "id", "gts_id", "gtsId"];
const TYPE_FIELD_NAMES: &[&str] = &["type", "r#type", "gts_type", "gtsType", "schema"];
const SERDE_TYPE_RENAMES: &[&str] = &["type", "gts_type", "gtsType", "schema"];

/// Represents a parsed version (major and optional minor)
#[derive(Debug, PartialEq)]
struct Version {
    major: u32,
    minor: Option<u32>,
}

impl Version {
    /// Format version for struct name suffix (e.g., "V1" or "`V1_0`")
    fn to_struct_suffix(&self) -> String {
        match self.minor {
            Some(minor) => format!("V{}_{}", self.major, minor),
            None => format!("V{}", self.major),
        }
    }

    /// Format version for schema ID (e.g., "v1" or "v1.0")
    fn to_schema_version(&self) -> String {
        match self.minor {
            Some(minor) => format!("v{}.{}", self.major, minor),
            None => format!("v{}", self.major),
        }
    }
}

/// Extract version from struct name suffix (e.g., `BaseEventV1` -> V1, `BaseEventV2_0` -> V2.0)
fn extract_struct_version(struct_name: &str) -> Option<Version> {
    // Look for pattern: V<major> or V<major>_<minor> at the end of the name
    // We need to find the last 'V' followed by digits
    let bytes = struct_name.as_bytes();
    let mut v_pos = None;

    // Find the last 'V' that starts a version suffix
    for i in (0..bytes.len()).rev() {
        // Check if 'V' is followed by at least one digit
        if bytes[i] == b'V' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            v_pos = Some(i);
            break;
        }
    }

    let v_pos = v_pos?;
    let version_part = &struct_name[v_pos + 1..]; // Skip the 'V'

    // Parse major_minor pattern
    if let Some(underscore_pos) = version_part.find('_') {
        // Has minor version: V<major>_<minor>
        let major_str = &version_part[..underscore_pos];
        let minor_str = &version_part[underscore_pos + 1..];

        let major = major_str.parse::<u32>().ok()?;
        let minor = minor_str.parse::<u32>().ok()?;
        Some(Version {
            major,
            minor: Some(minor),
        })
    } else {
        // Only major version: V<major>
        let major = version_part.parse::<u32>().ok()?;
        Some(Version { major, minor: None })
    }
}

/// Extract version from `schema_id`'s last segment (e.g., `gts.x.core.events.type.v1~` -> v1)
fn extract_schema_version(schema_id: &str) -> Option<Version> {
    // Get the last segment (after last '~' that's followed by content, or the whole string if no '~')
    // schema_id format: "gts.vendor.package.namespace.type.vMAJOR~" or with inheritance
    // "gts.x.core.events.type.v1~x.core.audit.event.v1~"

    // The version for this struct is in the LAST segment
    let last_segment = if schema_id.ends_with('~') {
        // Trim the trailing ~ and find the last segment
        let trimmed = schema_id.trim_end_matches('~');
        if let Some(pos) = trimmed.rfind('~') {
            &trimmed[pos + 1..]
        } else {
            trimmed
        }
    } else {
        schema_id
    };

    // Now find the version in this segment
    // Format is: something.vMAJOR or something.vMAJOR.MINOR
    // Find the last ".v" followed by a digit
    let mut version_start = None;
    let bytes = last_segment.as_bytes();

    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'.' && bytes[i + 1] == b'v' && bytes[i + 2].is_ascii_digit() {
            version_start = Some(i + 2); // Position after ".v"
        }
    }

    let version_start = version_start?;
    let version_part = &last_segment[version_start..];

    // Parse version: MAJOR or MAJOR.MINOR
    if let Some(dot_pos) = version_part.find('.') {
        // Has minor version: MAJOR.MINOR
        let major_str = &version_part[..dot_pos];
        let minor_str = &version_part[dot_pos + 1..];

        let major = major_str.parse::<u32>().ok()?;
        let minor = minor_str.parse::<u32>().ok()?;
        Some(Version {
            major,
            minor: Some(minor),
        })
    } else {
        // Only major version
        let major = version_part.parse::<u32>().ok()?;
        Some(Version { major, minor: None })
    }
}

/// Extract the parent schema ID from a `schema_id` (removes the last segment)
/// e.g., `gts.x.core.events.type.v1~x.core.audit.event.v1~` -> `gts.x.core.events.type.v1~`
fn extract_parent_schema_id(schema_id: &str) -> Option<String> {
    let trimmed = schema_id.trim_end_matches('~');
    trimmed
        .rfind('~')
        .map(|pos| format!("{}~", &trimmed[..pos]))
}

/// Count the number of segments in a `schema_id`
/// e.g., `gts.x.core.events.type.v1~` -> 1
/// e.g., `gts.x.core.events.type.v1~x.core.audit.event.v1~` -> 2
fn count_schema_segments(schema_id: &str) -> usize {
    schema_id.matches('~').count()
}

/// Check if a type is `GtsInstanceId` (either directly or as a path)
fn is_type_gts_instance_id(ty: &syn::Type) -> bool {
    is_type_named(ty, "GtsInstanceId")
}

/// Check if a type is `GtsSchemaId` (either directly or as a path)
fn is_type_gts_schema_id(ty: &syn::Type) -> bool {
    is_type_named(ty, "GtsSchemaId")
}

/// Helper function to check if a type matches a given name (either directly or as `gts::Name`)
fn is_type_named(ty: &syn::Type, name: &str) -> bool {
    match ty {
        syn::Type::Path(type_path) => {
            // Check for simple name or gts::name
            if let Some(last_segment) = type_path.path.segments.last()
                && last_segment.ident == name
            {
                return true;
            }

            // Check for full path like gts::Name
            if type_path.path.segments.len() == 2 {
                let segments: Vec<String> = type_path
                    .path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect();
                if segments == ["gts", name] {
                    return true;
                }
            }

            false
        }
        _ => false,
    }
}

/// Extract serde rename value from field attributes
fn get_serde_rename(field: &syn::Field) -> Option<String> {
    for attr in &field.attrs {
        // Parse the serde attribute using a simpler approach
        if attr.path().is_ident("serde")
            && let Ok(meta) = attr.meta.require_list()
        {
            let tokens = meta.tokens.to_string();

            // Look for rename = "value" pattern in the token string
            if let Some(rename_start) = tokens.find("rename") {
                let rename_part = &tokens[rename_start..];
                if let Some(eq_pos) = rename_part.find('=') {
                    let value_part = &rename_part[eq_pos + 1..].trim();
                    // Extract the string value between quotes
                    if value_part.starts_with('"') && value_part.ends_with('"') {
                        let rename_value = &value_part[1..value_part.len() - 1];
                        return Some(rename_value.to_owned());
                    }
                }
            }
        }
    }
    None
}

/// Check if a field has a serde rename matching any of the given names
fn has_matching_serde_rename(field: &syn::Field, names: &[&str]) -> bool {
    get_serde_rename(field).is_some_and(|rename| names.contains(&rename.as_str()))
}

/// Check if a field name matches any of the given names
fn field_name_matches(field: &syn::Field, names: &[&str]) -> bool {
    field
        .ident
        .as_ref()
        .is_some_and(|name| names.contains(&name.to_string().as_str()))
}

/// Validate base struct field requirements
fn validate_base_struct_fields(
    input: &syn::DeriveInput,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    args: &GtsSchemaArgs,
) -> Result<(), syn::Error> {
    if !matches!(args.base, BaseAttr::IsBase) {
        return Ok(());
    }

    // Check for presence of ID and GTS Type fields (including serde renames)
    let has_id_field = fields.iter().any(|f| field_name_matches(f, ID_FIELD_NAMES));

    let has_type_field = fields.iter().any(|f| {
        field_name_matches(f, TYPE_FIELD_NAMES) || has_matching_serde_rename(f, SERDE_TYPE_RENAMES)
    });

    if !has_id_field && !has_type_field {
        return Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "struct_to_gts_schema: Base structs must have either an ID field (one of: {}) OR a GTS Type field (one of: {}), but not both.",
                ID_FIELD_NAMES.join(", "),
                TYPE_FIELD_NAMES.join(", ")
            ),
        ));
    }

    // Validate field types
    validate_field_types(input, fields)
}

/// Validate that field types are correct for ID and GTS Type fields
fn validate_field_types(
    input: &syn::DeriveInput,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> Result<(), syn::Error> {
    let has_valid_id_field = fields.iter().any(|field| {
        field_name_matches(field, ID_FIELD_NAMES) && is_type_gts_instance_id(&field.ty)
    });

    let has_valid_type_field = fields.iter().any(|field| {
        let is_type_field = field_name_matches(field, TYPE_FIELD_NAMES)
            || has_matching_serde_rename(field, SERDE_TYPE_RENAMES);
        is_type_field && is_type_gts_schema_id(&field.ty)
    });

    // Enforce "either/or but not both" logic
    if has_valid_id_field && has_valid_type_field {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "struct_to_gts_schema: Base structs must have either an ID field (one of: $id, id, gts_id, or gtsId) of type GtsInstanceId OR a GTS Type field (one of: type, gts_type, gtsType, or schema) of type GtsSchemaId, but not both. Found both valid ID and GTS Type fields.",
        ));
    }

    if !has_valid_id_field && !has_valid_type_field {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "struct_to_gts_schema: Base structs must have either an ID field (one of: $id, id, gts_id, or gtsId) of type GtsInstanceId OR a GTS Type field (one of: type, gts_type, gtsType, or schema) of type GtsSchemaId",
        ));
    }

    Ok(())
}

/// Validate that the struct name version suffix matches the `schema_id` version
fn validate_version_match(struct_ident: &syn::Ident, schema_id: &str) -> syn::Result<()> {
    let struct_name = struct_ident.to_string();
    let struct_version = extract_struct_version(&struct_name);
    let schema_version = extract_schema_version(schema_id);

    match (struct_version, schema_version) {
        (Some(sv), Some(schv)) if sv != schv => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "struct_to_gts_schema: Version mismatch between struct name and schema_id. \
                 Struct '{struct_name}' has version suffix '{}' but schema_id '{schema_id}' \
                 has version '{}'. The versions must match exactly \
                 (e.g., BaseEventV1 with v1~, or BaseEventV2_0 with v2.0~)",
                sv.to_struct_suffix(),
                schv.to_schema_version()
            ),
        )),
        (Some(_), Some(_)) => Ok(()), // Versions match
        (None, Some(schv)) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "struct_to_gts_schema: schema_id '{schema_id}' has a version but struct '{struct_name}' \
                 does not have a version suffix. Add '{}' suffix to the struct name \
                 (e.g., '{struct_name}{}')",
                schv.to_struct_suffix(),
                schv.to_struct_suffix()
            ),
        )),
        (Some(sv), None) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "struct_to_gts_schema: Struct '{struct_name}' has version suffix '{}' but \
                 cannot extract version from schema_id '{schema_id}'. \
                 Expected format with version like 'gts.x.foo.v1~' or 'gts.x.foo.v1.0~'",
                sv.to_struct_suffix()
            ),
        )),
        (None, None) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "struct_to_gts_schema: Both struct name and schema_id must have a version. \
                 Struct '{struct_name}' has no version suffix (e.g., V1) and schema_id '{schema_id}' \
                 has no version (e.g., v1~). Add version to both (e.g., '{struct_name}V1' with 'gts.x.foo.v1~')"
            ),
        )),
    }
}

/// Check if a derive attribute contains a specific trait name
fn has_derive(input: &syn::DeriveInput, trait_name: &str) -> bool {
    input.attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && attr
                .meta
                .require_list()
                .map(|meta| meta.tokens.to_string().contains(trait_name))
                .unwrap_or(false)
    })
}

/// Helper struct for parsing derive lists.
struct DeriveList(syn::punctuated::Punctuated<syn::Path, syn::Token![,]>);
impl syn::parse::Parse for DeriveList {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(DeriveList(syn::punctuated::Punctuated::parse_terminated(
            input,
        )?))
    }
}

/// Check if a derive list contains a specific trait by name.
fn derive_list_contains(
    derives: &syn::punctuated::Punctuated<syn::Path, syn::Token![,]>,
    trait_name: &str,
) -> bool {
    derives.iter().any(|path| {
        path.segments
            .last()
            .is_some_and(|seg| seg.ident == trait_name)
    })
}

/// Check if an attribute is a `derive`/`cfg_attr` that includes a specific trait.
fn attr_derives_trait(attr: &syn::Attribute, trait_name: &str) -> bool {
    if attr.path().is_ident("derive") {
        if let Ok(meta) = attr.meta.require_list()
            && let Ok(DeriveList(derives)) = syn::parse2::<DeriveList>(meta.tokens.clone())
        {
            return derive_list_contains(&derives, trait_name);
        }
        return false;
    }

    if attr.path().is_ident("cfg_attr") {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        let nested: syn::punctuated::Punctuated<syn::Meta, syn::Token![,]> =
            match list.parse_args_with(syn::punctuated::Punctuated::parse_terminated) {
                Ok(meta) => meta,
                Err(_) => return false,
            };
        for meta in nested {
            if let syn::Meta::List(inner) = meta
                && inner.path.is_ident("derive")
                && let Ok(DeriveList(derives)) = syn::parse2::<DeriveList>(inner.tokens)
                && derive_list_contains(&derives, trait_name)
            {
                return true;
            }
        }

        // Fallback: if parsing fails or cfg_attr is complex, do a token scan
        let tokens = list.tokens.to_string();
        return tokens.contains(trait_name);
    }

    false
}

/// Check if attributes include Serialize or Deserialize derives (direct or `cfg_attr`).
fn has_serde_derives(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr_derives_trait(attr, "Serialize") || attr_derives_trait(attr, "Deserialize")
    })
}

/// Remove specific derive traits from the input.
///
/// For nested structs, we need to remove Serialize and Deserialize derives
/// to prevent direct serialization. This enforces that nested GTS structs
/// can only be serialized through their base struct wrapper.
fn remove_derives(input: &mut syn::DeriveInput, traits_to_remove: &[&str]) {
    let mut new_attrs = Vec::new();

    for attr in input.attrs.drain(..) {
        if !attr.path().is_ident("derive") {
            new_attrs.push(attr);
            continue;
        }

        // Parse the derive list and filter out unwanted traits
        if let Ok(meta) = attr.meta.require_list() {
            let tokens = &meta.tokens;

            // Parse as punctuated paths
            if let Ok(DeriveList(derives)) = syn::parse2::<DeriveList>(tokens.clone()) {
                // Filter out the traits to remove
                let kept_derives: Vec<_> = derives
                    .iter()
                    .filter(|path| {
                        let path_str = quote::quote!(#path).to_string();
                        !traits_to_remove.iter().any(|t| path_str.contains(t))
                    })
                    .collect();

                if !kept_derives.is_empty() {
                    // Rebuild the derive attribute with only kept traits
                    new_attrs.push(syn::parse_quote!(#[derive(#(#kept_derives),*)]));
                }
                // If all traits were removed, don't add any derive
            } else {
                // Couldn't parse, keep the original
                new_attrs.push(attr);
            }
        } else {
            new_attrs.push(attr);
        }
    }

    input.attrs = new_attrs;
}

/// Add missing required derives (Serialize, Deserialize, `JsonSchema`)
///
/// For nested structs (`base = ParentStruct`), Serialize and Deserialize are NOT added.
/// This prevents direct serialization of nested structs - they can only be serialized
/// through their base struct wrapper.
fn add_missing_derives(input: &mut syn::DeriveInput, base: &BaseAttr) {
    // For nested structs (base = ParentStruct), only add JsonSchema
    // Serialize/Deserialize will be provided via GtsSerialize/GtsDeserialize traits
    let is_nested = matches!(base, BaseAttr::Parent(_));

    let derives_to_add: Vec<&str> = if is_nested {
        // Nested struct: only JsonSchema
        [("JsonSchema", "schemars::JsonSchema")]
            .into_iter()
            .filter(|(check, _)| !has_derive(input, check))
            .map(|(_, full)| full)
            .collect()
    } else {
        // Base struct: all three derives
        [
            ("Serialize", "serde::Serialize"),
            ("Deserialize", "serde::Deserialize"),
            ("JsonSchema", "schemars::JsonSchema"),
        ]
        .into_iter()
        .filter(|(check, _)| !has_derive(input, check))
        .map(|(_, full)| full)
        .collect()
    };

    if !derives_to_add.is_empty() {
        let derives_str = derives_to_add.join(", ");
        let derives_tokens: proc_macro2::TokenStream =
            derives_str.parse().expect("Failed to parse derive tokens");
        input
            .attrs
            .push(syn::parse_quote!(#[derive(#derives_tokens)]));
    }
}

/// Validate that base attribute is consistent with `schema_id` segment count
fn validate_base_segments(
    input: &syn::DeriveInput,
    base: &BaseAttr,
    schema_id: &str,
) -> Result<(), syn::Error> {
    let segment_count = count_schema_segments(schema_id);

    match base {
        BaseAttr::IsBase if segment_count > 1 => Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "struct_to_gts_schema: 'base = true' but schema_id '{schema_id}' has {segment_count} segments. \
                 A base type must have exactly 1 segment (no parent). \
                 Either use 'base = ParentStruct' or fix the schema_id."
            ),
        )),
        BaseAttr::Parent(_) if segment_count < 2 => Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "struct_to_gts_schema: 'base' specifies a parent struct but schema_id '{schema_id}' \
                 has only {segment_count} segment. A child type must have at least 2 segments. \
                 Either use 'base = true' or add parent segment to schema_id."
            ),
        )),
        _ => Ok(()),
    }
}

/// Add serde attributes for GtsSerialize/GtsDeserialize on base structs with generic fields.
///
/// For base structs (`base = true`) with a generic parameter P, this adds:
/// - `#[serde(bound(serialize = "P: ::gts::GtsSerialize", deserialize = "P: ::gts::GtsDeserialize<'de>"))]` on the struct
/// - `#[serde(serialize_with = "::gts::serialize_gts", deserialize_with = "::gts::deserialize_gts")]` on the generic field
fn add_gts_serde_attrs(input: &mut syn::DeriveInput, base: &BaseAttr) {
    // Only for base structs
    if !matches!(base, BaseAttr::IsBase) {
        return;
    }

    // Get the generic type parameter name if present
    let generic_param_name: Option<String> = input
        .generics
        .type_params()
        .next()
        .map(|tp| tp.ident.to_string());

    let Some(generic_param) = generic_param_name else {
        return; // No generic parameter, nothing to do
    };

    // Build the bound strings
    let serialize_bound = format!("{generic_param}: ::gts::GtsSerialize");
    let deserialize_bound = format!("{generic_param}: ::gts::GtsDeserialize<'de>");

    // Add serde bound attribute on the struct
    let bound_attr: syn::Attribute = syn::parse_quote!(
        #[serde(bound(
            serialize = #serialize_bound,
            deserialize = #deserialize_bound
        ))]
    );
    input.attrs.push(bound_attr);

    // Add serialize_with/deserialize_with attributes on the generic field
    if let syn::Data::Struct(ref mut data_struct) = input.data
        && let syn::Fields::Named(ref mut fields) = data_struct.fields
    {
        for field in &mut fields.named {
            // Check if this field's type is the generic parameter
            let field_type = &field.ty;
            let field_type_str = quote::quote!(#field_type).to_string().replace(' ', "");
            if field_type_str == generic_param {
                // Add serde attributes to this field
                let field_attr: syn::Attribute = syn::parse_quote!(
                    #[serde(
                        serialize_with = "::gts::serialize_gts",
                        deserialize_with = "::gts::deserialize_gts"
                    )]
                );
                field.attrs.push(field_attr);
            }
        }
    }
}

/// Build a custom where clause with additional trait bounds on generic params
fn build_where_clause(
    generics: &syn::Generics,
    where_clause: Option<&syn::WhereClause>,
    bounds: &str,
) -> proc_macro2::TokenStream {
    if let Some(generic_param) = generics.type_params().next() {
        let generic_ident = &generic_param.ident;
        let bounds_tokens: proc_macro2::TokenStream =
            bounds.parse().expect("Failed to parse bounds");
        if let Some(existing) = where_clause {
            quote! { #existing #generic_ident: #bounds_tokens, }
        } else {
            quote! { where #generic_ident: #bounds_tokens }
        }
    } else {
        quote! { #where_clause }
    }
}

/// Represents the `base` attribute value for struct inheritance
enum BaseAttr {
    /// This struct is a base type (no parent)
    IsBase,
    /// This struct inherits from the specified parent struct (e.g., `ParentStruct`)
    /// The macro automatically uses `ParentStruct<()>` in generated code
    Parent(syn::Ident),
}

/// Arguments for the `struct_to_gts_schema` macro
struct GtsSchemaArgs {
    dir_path: String,
    schema_id: String,
    description: String,
    properties: String,
    base: BaseAttr,
}

impl Parse for GtsSchemaArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut dir_path: Option<String> = None;
        let mut schema_id: Option<String> = None;
        let mut description: Option<String> = None;
        let mut properties: Option<String> = None;
        let mut base: Option<BaseAttr> = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "dir_path" => {
                    let value: LitStr = input.parse()?;
                    dir_path = Some(value.value());
                }
                "schema_id" => {
                    let value: LitStr = input.parse()?;
                    let id = value.value();
                    // Schema-specific check: must end with ~
                    if !id.ends_with('~') {
                        return Err(syn::Error::new_spanned(
                            value,
                            format!(
                                "struct_to_gts_schema: Invalid GTS schema ID: must end with '~' (type marker), got '{id}'"
                            ),
                        ));
                    }
                    // General GTS ID validation via shared crate
                    if let Err(e) = gts_id::validate_gts_id(&id, false) {
                        let msg = match &e {
                            gts_id::GtsIdError::Id { cause, .. } => {
                                format!("Invalid GTS schema ID: {cause}")
                            }
                            gts_id::GtsIdError::Segment { num, cause, .. } => {
                                format!("Segment #{num}: {cause}")
                            }
                        };
                        return Err(syn::Error::new_spanned(
                            value,
                            format!("struct_to_gts_schema: {msg}"),
                        ));
                    }
                    schema_id = Some(id);
                }
                "description" => {
                    let value: LitStr = input.parse()?;
                    description = Some(value.value());
                }
                "properties" => {
                    let value: LitStr = input.parse()?;
                    properties = Some(value.value());
                }
                "base" => {
                    // base can be: true (is a base type) or a struct name (parent struct)
                    // Handle 'true' as a boolean literal (keyword)
                    if input.peek(syn::LitBool) {
                        let lit: syn::LitBool = input.parse()?;
                        if lit.value {
                            base = Some(BaseAttr::IsBase);
                        } else {
                            return Err(syn::Error::new_spanned(
                                lit,
                                "base = false is not valid. Use 'base = true' for base types or 'base = ParentStruct' for child types",
                            ));
                        }
                    } else if input.peek(syn::Ident) {
                        // Parse parent struct name - the macro automatically adds <()>
                        let ident: syn::Ident = input.parse()?;
                        base = Some(BaseAttr::Parent(ident));
                    } else {
                        return Err(syn::Error::new_spanned(
                            key,
                            "base must be 'true' or a parent struct name (e.g., 'base = ParentStruct')",
                        ));
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "Unknown attribute. Expected: dir_path, schema_id, description, properties, or base",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(GtsSchemaArgs {
            dir_path: dir_path
                .ok_or_else(|| input.error("Missing required attribute: dir_path"))?,
            schema_id: schema_id
                .ok_or_else(|| input.error("Missing required attribute: schema_id"))?,
            description: description
                .ok_or_else(|| input.error("Missing required attribute: description"))?,
            properties: properties
                .ok_or_else(|| input.error("Missing required attribute: properties"))?,
            base: base
                .ok_or_else(|| input.error("Missing required attribute: base (use 'base = true' for base types or 'base = ParentStruct' for child types)"))?,
        })
    }
}

/// Annotate a Rust struct for GTS schema generation.
///
/// This macro serves three purposes:
///
/// ## 1. Compile-Time Validation & Guarantees
///
/// The macro validates your annotations at compile time, catching errors early:
/// - ✅ All required attributes exist (`dir_path`, `schema_id`, `description`, `properties`)
/// - ✅ Every property in `properties` exists as a field in the struct
/// - ✅ Only structs with named fields are supported (no tuple/unit structs or enums)
/// - ✅ Single generic parameter maximum (prevents inheritance ambiguity)
/// - ✅ Valid GTS ID format enforcement
/// - ✅ Zero runtime allocation for generated constants
///
/// ## 2. Schema Generation
///
/// After annotating your structs, run:
/// ```bash
/// cargo gts generate --source src/
/// ```
///
/// Or use the GTS CLI directly:
/// ```bash
/// gts generate-from-rust --source src/ --output schemas/
/// ```
///
/// This will generate JSON Schema files at the specified `dir_path` with names derived from `schema_id` for each annotated struct (e.g., `{dir_path}/{schema_id}.schema.json`).
///
/// ## 3. Runtime API
///
/// The macro generates these associated methods and implements the `GtsSchema` trait:
///
/// - `gts_schema_id() -> &'static GtsSchemaId` - Get the struct's GTS schema ID
/// - `gts_base_schema_id() -> Option<&'static GtsSchemaId>` - Get parent schema ID (None for base structs)
/// - `gts_schema_with_refs() -> serde_json::Value` - JSON Schema with `allOf` + `$ref` for inheritance
/// - `gts_schema_with_refs_as_string() -> String` - Schema as compact JSON string
/// - `gts_schema_with_refs_as_string_pretty() -> String` - Schema as pretty-printed JSON string
/// - `gts_make_instance_id(segment: &str) -> gts::GtsInstanceId` - Generate an instance ID by appending
///   a segment to the schema ID. The segment must be a valid GTS segment (e.g., "a.b.c.v1")
/// - `GtsSchema` trait implementation - Enables runtime schema composition for nested generic types
///   (e.g., `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`), with proper nesting and inheritance support.
///   Generic fields automatically have `additionalProperties: false` set to ensure type safety.
///
/// # Arguments
///
/// * `dir_path` - Directory where the schema file will be generated (relative to crate root)
/// * `schema_id` - GTS identifier in format: `gts.vendor.package.namespace.type.vMAJOR~`
///   - **Automatic inheritance**: If the `schema_id` contains multiple segments separated by `~`, inheritance is automatically detected
///   - Example: `gts.x.core.events.type.v1~x.core.audit.event.v1~` inherits from `gts.x.core.events.type.v1~`
/// * `description` - Human-readable description of the schema
/// * `properties` - Comma-separated list of struct fields to include in the schema
/// * `base` - Explicit base/parent struct declaration (required):
///   - `base = true`: Marks this struct as a base type (must have single-segment `schema_id`)
///   - `base = ParentStruct`: Parent struct name (macro automatically uses `ParentStruct<()>`)
///
/// # Memory Efficiency
///
/// Schema IDs use `LazyLock` for efficient one-time initialization with **zero allocation after first access**:
/// - `gts_schema_id()` and `gts_base_schema_id()` return static references to `GtsSchemaId` instances
/// - Schema generation methods create JSON on-demand using schemars and the `GtsSchema` trait
///
/// # Example
///
/// ```ignore
/// use gts_macros::struct_to_gts_schema;
///
/// #[struct_to_gts_schema(
///     dir_path = "schemas",
///     schema_id = "gts.x.core.events.topic.v1~",
///     description = "Event broker topics",
///     properties = "id,persisted,retention_days,name"
/// )]
/// struct User {
///     id: String,
///     persisted: bool,
///     retention_days: i32,
///     internal_field: i32, // Not included in schema (not in properties list)
/// }
///
/// // Runtime usage:
/// let schema_id = User::gts_schema_id();
/// let schema_json = User::gts_schema_with_refs_as_string_pretty();
/// let instance_id = User::gts_make_instance_id("vendor.marketplace.orders.order_created.v1");
/// assert_eq!(instance_id.as_ref(), "gts.x.core.events.topic.v1~vendor.marketplace.orders.order_created.v1");
/// ```
#[proc_macro_attribute]
#[allow(
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    clippy::cognitive_complexity
)]
pub fn struct_to_gts_schema(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as GtsSchemaArgs);
    let input = parse_macro_input!(item as DeriveInput);

    // Prohibit multiple type generic parameters (GTS notation assumes nested segments)
    let generic_count = input.generics.type_params().count();
    if generic_count > 1 {
        return syn::Error::new_spanned(
            &input.ident,
            "struct_to_gts_schema: Multiple type generic parameters are not supported (GTS schemas assume nested segments)",
        )
        .to_compile_error()
        .into();
    }

    // base = true can have 0 or 1 generic field:
    // - 0 generics: This is a leaf/terminal type, no derived structs can extend it
    // - 1 generic: Derived structs can extend via the generic field
    // (validation that base = ParentStruct requires parent to have 1 generic is done later via compile-time assertion)

    // Parse properties list
    let property_names: Vec<String> = args
        .properties
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    // Extract struct fields for validation
    // Allow unit structs (no fields) for nested types that don't add new properties
    let struct_fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => Some(&fields.named),
            Fields::Unit => {
                // Unit structs are allowed for nested types with empty properties
                if !property_names.is_empty() {
                    return syn::Error::new_spanned(
                        &input.ident,
                        "struct_to_gts_schema: Unit struct cannot have properties. \
                         Either add named fields or use properties = \"\"",
                    )
                    .to_compile_error()
                    .into();
                }
                None // No fields to validate
            }
            Fields::Unnamed(_) => {
                return syn::Error::new_spanned(
                    &input.ident,
                    "struct_to_gts_schema: Tuple structs are not supported. \
                     Use a struct with named fields or a unit struct (for empty nested types)",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                &input.ident,
                "struct_to_gts_schema: Only structs are supported",
            )
            .to_compile_error()
            .into();
        }
    };

    // Validate that all requested properties exist (only for structs with fields)
    if let Some(fields) = struct_fields {
        let available_fields: Vec<String> = fields
            .iter()
            .filter_map(|f| f.ident.as_ref().map(ToString::to_string))
            .collect();

        for prop in &property_names {
            if !available_fields.contains(prop) {
                return syn::Error::new_spanned(
                    &input.ident,
                    format!(
                        "struct_to_gts_schema: Property '{prop}' not found in struct. Available fields: {available_fields:?}"
                    ),
                )
                .to_compile_error()
                .into();
            }
        }

        // Validate base struct field requirements
        if let Err(err) = validate_base_struct_fields(&input, fields, &args) {
            return err.to_compile_error().into();
        }
    }

    // Validate version match between struct name suffix and schema_id
    if let Err(err) = validate_version_match(&input.ident, &args.schema_id) {
        return err.to_compile_error().into();
    }

    // Add GtsSchema bound to generic type parameters so that only valid GTS types
    // (those with struct_to_gts_schema applied, or ()) can be used as generic args.
    // This prevents usage like BaseEventV1<SomeRandomStruct> where SomeRandomStruct
    // is not a proper GTS schema type.
    let mut modified_input = input.clone();
    for param in modified_input.generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!(::gts::GtsSchema));
    }

    // For nested structs, remove Serialize/Deserialize to prevent direct serialization
    // This is critical for Issue #24: nested structs can only be serialized through base struct
    if matches!(&args.base, BaseAttr::Parent(_)) {
        if has_serde_derives(&input.attrs) {
            return syn::Error::new_spanned(
                &input.ident,
                "struct_to_gts_schema: Nested structs must not derive Serialize/Deserialize. \
                 Serialize them through the base struct instead.",
            )
            .to_compile_error()
            .into();
        }
        remove_derives(&mut modified_input, &["Serialize", "Deserialize"]);
    }

    // Automatically add required derives: Serialize, Deserialize, JsonSchema
    // For nested structs, only JsonSchema is added (no direct serialization)
    add_missing_derives(&mut modified_input, &args.base);

    // For base structs with generic fields, add serde attributes for GtsSerialize/GtsDeserialize
    add_gts_serde_attrs(&mut modified_input, &args.base);

    // Validate base attribute consistency with schema_id segments
    if let Err(err) = validate_base_segments(&input, &args.base, &args.schema_id) {
        return err.to_compile_error().into();
    }
    let expected_parent_schema_id = extract_parent_schema_id(&args.schema_id);

    // Build the schema output file path from dir_path + schema_id
    let struct_name = &input.ident;
    let dir_path = &args.dir_path;
    let schema_id = &args.schema_id;
    let description = &args.description;
    let properties_str = &args.properties;

    let schema_file_path = format!("{dir_path}/{schema_id}.schema.json");

    // Extract generics to properly handle generic structs
    // Use modified_input.generics which has the GtsSchema bounds added
    let generics = &modified_input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Get the generic type parameter name if present
    let generic_param_name: Option<String> = input
        .generics
        .type_params()
        .next()
        .map(|tp| tp.ident.to_string());

    let mut generic_field_name: Option<String> = None;

    // Find the field that uses the generic type (only for structs with fields)
    // Use the SERIALIZED name (serde rename if present, otherwise field ident)
    if let (Some(gp), Some(fields)) = (&generic_param_name, struct_fields) {
        for field in fields {
            let field_type = &field.ty;
            let field_type_str = quote::quote!(#field_type).to_string().replace(' ', "");
            if field_type_str == *gp
                && let Some(ident) = &field.ident
            {
                // Use serde rename if present, otherwise use the field identifier
                generic_field_name =
                    Some(get_serde_rename(field).unwrap_or_else(|| ident.to_string()));
                break;
            }
        }
    }

    // Generate the GENERIC_FIELD constant value
    let generic_field_option = if let Some(ref field_name) = generic_field_name {
        quote! { Some(#field_name) }
    } else {
        quote! { None }
    };

    // Generate BASE_SCHEMA_ID constant (private) and compile-time assertion for base struct matching
    let base_schema_id_const = if let Some(parent_id) = &expected_parent_schema_id {
        quote! {
            /// Parent schema ID (extracted from schema_id segments). Use `gts_base_schema_id()` instead.
            #[doc(hidden)]
            #[allow(dead_code)]
            const BASE_SCHEMA_ID: Option<&'static str> = Some(#parent_id);
        }
    } else {
        quote! {
            /// Parent schema ID (None for base types). Use `gts_base_schema_id()` instead.
            #[doc(hidden)]
            #[allow(dead_code)]
            const BASE_SCHEMA_ID: Option<&'static str> = None;
        }
    };

    // Generate the literal option value for use in static initializers (avoids Self::BASE_SCHEMA_ID)
    let base_schema_id_option = if let Some(parent_id) = &expected_parent_schema_id {
        quote! { Some(#parent_id) }
    } else {
        quote! { None::<&'static str> }
    };

    // Generate compile-time assertion when base = ParentStruct
    let base_assertion = match &args.base {
        BaseAttr::Parent(parent_ident) => {
            let parent_id = expected_parent_schema_id
                .as_ref()
                .expect("parent_id must exist when base is specified");
            let schema_id_assertion_msg = format!(
                "struct_to_gts_schema: Base struct '{parent_ident}' schema ID must match parent segment '{parent_id}' from schema_id"
            );
            let generic_field_assertion_msg = format!(
                "struct_to_gts_schema: Base struct '{parent_ident}' must have exactly 1 generic field. \
                 Parent types must define a generic field (e.g., `pub payload: P`) that child types extend."
            );
            quote! {
                // Compile-time assertion: verify parent struct's GTS_SCHEMA_ID matches expected parent segment
                // We use <ParentStruct<()> as GtsSchema> since all GTS structs must be generic
                const _: () = {
                    // Use a const assertion to verify at compile time
                    const PARENT_ID: &'static str = <#parent_ident<()> as ::gts::GtsSchema>::SCHEMA_ID;
                    const EXPECTED_ID: &'static str = #parent_id;
                    // Use a manual string comparison for const context
                    const _: () = {
                        // Manual string equality check for const context
                        if PARENT_ID.as_bytes().len() != EXPECTED_ID.as_bytes().len() {
                            panic!(#schema_id_assertion_msg);
                        }
                        let mut i = 0;
                        while i < PARENT_ID.as_bytes().len() {
                            if PARENT_ID.as_bytes()[i] != EXPECTED_ID.as_bytes()[i] {
                                panic!(#schema_id_assertion_msg);
                            }
                            i += 1;
                        }
                    };
                };

                // Compile-time assertion: verify parent struct has exactly 1 generic field
                const _: () = {
                    const PARENT_GENERIC_FIELD: Option<&'static str> = <#parent_ident<()> as ::gts::GtsSchema>::GENERIC_FIELD;
                    if PARENT_GENERIC_FIELD.is_none() {
                        panic!(#generic_field_assertion_msg);
                    }
                };
            }
        }
        BaseAttr::IsBase => quote! {},
    };

    // Generate gts_schema() implementation based on whether we have a generic parameter
    let has_generic = input.generics.type_params().count() > 0;

    // Build custom where clauses for different impl blocks
    let gts_schema_where_clause = build_where_clause(
        generics,
        where_clause,
        "::gts::GtsSchema + ::schemars::JsonSchema",
    );
    let serialize_where_clause = build_where_clause(
        generics,
        where_clause,
        "::gts::GtsSerialize + ::gts::GtsSchema",
    );

    let gts_schema_impl = if has_generic {
        let generic_param = input.generics.type_params().next().unwrap();
        let generic_ident = &generic_param.ident;
        let generic_field_for_path = generic_field_name.as_deref().unwrap_or_default();

        quote! {
            fn gts_schema() -> serde_json::Value {
                Self::gts_schema_with_refs()
            }

            fn innermost_schema_id() -> &'static str {
                // Recursively get the innermost type's schema ID
                let inner_id = <#generic_ident as ::gts::GtsSchema>::innermost_schema_id();
                if inner_id.is_empty() {
                    Self::SCHEMA_ID
                } else {
                    inner_id
                }
            }

            fn innermost_schema() -> serde_json::Value {
                // Get the innermost type's raw schemars schema
                let inner = <#generic_ident as ::gts::GtsSchema>::innermost_schema();
                // If inner is just {"type": "object"} (from ()), return our own schema
                // schemars RootSchema serializes at root level (not under "schema" field)
                if inner.get("properties").is_none() {
                    let root_schema = schemars::schema_for!(Self);
                    return serde_json::to_value(&root_schema).expect("schemars");
                }
                inner
            }

            fn collect_nesting_path() -> Vec<&'static str> {
                // Collect the path from outermost to the PARENT of the innermost type.
                // For Outer<Middle<()>> where Outer has generic field "a" and Middle has "b":
                //   - () has no properties, so Middle IS the innermost
                //   - Path is just ["a"]
                // For Outer<Middle<Inner>> where Inner has properties:
                //   - Inner is the innermost type with properties
                //   - Path is ["a", "b"]

                let inner_path = <#generic_ident as ::gts::GtsSchema>::collect_nesting_path();
                let inner_id = <#generic_ident as ::gts::GtsSchema>::SCHEMA_ID;

                // If inner type is () (empty ID), don't include this type's field
                // because this type IS the innermost type with properties
                if inner_id.is_empty() {
                    return Vec::new();
                }

                // Otherwise, prepend this type's generic field to inner path
                let mut path = Vec::new();
                let field = #generic_field_for_path;
                if !field.is_empty() {
                    path.push(field);
                }
                path.extend(inner_path);
                path
            }

            fn gts_schema_with_refs_allof() -> serde_json::Value {
                // Use THIS struct's schema ID for both $id and parent determination
                // When a generic base struct is instantiated with a concrete type,
                // it should still generate its own base schema, not the innermost type's schema
                let schema_id = Self::SCHEMA_ID;

                // Get parent's ID by removing last segment from THIS struct's schema_id
                // e.g., "a~b~c~" -> "a~b~"
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

                // Get THIS struct's schema (schemars will expand generic fields automatically)
                let root_schema = schemars::schema_for!(Self);
                let schema_val = serde_json::to_value(&root_schema).expect("schemars");
                let mut properties = schema_val.get("properties").cloned().unwrap_or(serde_json::json!({}));
                let required = schema_val.get("required").cloned().unwrap_or(serde_json::json!([]));

                // Replace the generic field with a simple {"type": "object"} placeholder
                // The generic field should not be expanded, regardless of the concrete type parameter
                if let Some(generic_field) = Self::GENERIC_FIELD {
                    if let Some(props) = properties.as_object_mut() {
                        if props.contains_key(generic_field) {
                            props.insert(generic_field.to_owned(), serde_json::json!({
                                "type": "object"
                            }));
                        }
                    }
                }

                // If no parent (base type), return simple schema without allOf
                // Base types have additionalProperties: false at root level
                // Generic fields are just {"type": "object"} (will be extended by children)
                if parent_schema_id.is_empty() {
                    let mut schema = serde_json::json!({
                        "$id": format!("gts://{}", schema_id),
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": false,
                        "properties": properties
                    });
                    if !required.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                        schema["required"] = required;
                    }
                    return schema;
                }

                // Build the nesting path from outer to inner generic fields
                // For Outer<Middle<Inner>> where Outer has field "a" and Middle has field "b":
                //   - innermost is Inner
                //   - parent is derived from innermost's schema ID
                //   - path ["a", "b"] wraps Inner's properties
                let nesting_path = Self::collect_nesting_path();

                // Get the generic field name for the innermost type (if it has one)
                // This field should NOT have additionalProperties: false since it will be extended
                let innermost_generic_field = <#generic_ident as ::gts::GtsSchema>::GENERIC_FIELD;

                // Wrap properties in the nesting path
                let nested_properties = Self::wrap_in_nesting_path(&nesting_path, properties, required.clone(), innermost_generic_field);

                // Child type - use allOf with $ref to parent
                serde_json::json!({
                    "$id": format!("gts://{}", schema_id),
                    "$schema": "http://json-schema.org/draft-07/schema#",
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
    } else {
        // For non-generic child types extending a generic base, we need to get the parent's
        // generic field name at compile time to properly nest the child properties
        let parent_generic_field_code = match &args.base {
            BaseAttr::Parent(parent_ident) => {
                quote! {
                    // Get the parent's generic field name for nesting
                    let parent_generic_field: Option<&'static str> = <#parent_ident<()> as ::gts::GtsSchema>::GENERIC_FIELD;
                }
            }
            BaseAttr::IsBase => {
                quote! {
                    let parent_generic_field: Option<&'static str> = None;
                }
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
                // Return this type's schemars schema (RootSchema serializes at root level)
                let root_schema = schemars::schema_for!(Self);
                serde_json::to_value(&root_schema).expect("schemars")
            }
            fn gts_schema_with_refs_allof() -> serde_json::Value {
                let schema_id = Self::SCHEMA_ID;

                // Get parent's ID by removing last segment
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

                // Get this type's schemars schema (RootSchema serializes at root level)
                let root_schema = schemars::schema_for!(Self);
                let schema_val = serde_json::to_value(&root_schema).expect("schemars");
                let mut properties = schema_val.get("properties").cloned().unwrap_or_else(|| serde_json::json!({}));
                let required = schema_val.get("required").cloned().unwrap_or_else(|| serde_json::json!([]));

                // Resolve internal $ref references to GtsInstanceId and GtsSchemaId at compile time
                // This is needed for schemas validated directly (not through GtsStore)
                // Runtime resolution in GtsStore::resolve_schema_refs provides additional coverage
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

                // If no parent (base type), return simple schema without allOf
                // Non-generic base types have additionalProperties: false at root level
                if parent_schema_id.is_empty() {
                    let mut schema = serde_json::json!({
                        "$id": format!("gts://{}", schema_id),
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": false,
                        "properties": properties
                    });
                    if !required.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                        schema["required"] = required;
                    }
                    return schema;
                }

                // Get the parent's generic field name for nesting child properties
                #parent_generic_field_code

                // Child type - use allOf with $ref to parent
                // Parent MUST have a generic field - this is enforced by compile-time assertion
                let field_name = parent_generic_field
                    .expect("Parent struct must have a generic field for derived types to extend");

                // Wrap properties in the parent's generic field path
                let nested_properties = Self::wrap_in_nesting_path(&[field_name], properties, required, None);
                serde_json::json!({
                    "$id": format!("gts://{}", schema_id),
                    "$schema": "http://json-schema.org/draft-07/schema#",
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
    };

    // Check if this is a unit struct - we need to add an allow attribute for clippy
    // because quote! may emit {} instead of ; for unit structs
    let is_unit_struct = matches!(&input.data, Data::Struct(data_struct) if matches!(&data_struct.fields, Fields::Unit));
    let is_base_unit_struct = is_unit_struct && matches!(args.base, BaseAttr::IsBase);

    if is_unit_struct {
        modified_input
            .attrs
            .push(syn::parse_quote!(#[allow(clippy::empty_structs_with_brackets)]));
    }

    // For BASE unit structs only, we provide custom Serialize/Deserialize implementations
    // (nested unit structs get GtsSerialize/GtsDeserialize instead)
    if is_base_unit_struct {
        // Remove our auto-added Serialize/Deserialize derives since we provide custom impls
        // Keep JsonSchema from our auto-added derives
        modified_input.attrs.retain(|attr| {
            if attr.path().is_ident("derive") {
                if let Ok(meta) = attr.meta.require_list() {
                    let tokens = meta.tokens.to_string();
                    // Remove derives that contain Serialize or Deserialize
                    // (our auto-added derive will have both)
                    !tokens.contains("Serialize") && !tokens.contains("Deserialize")
                } else {
                    true
                }
            } else {
                true
            }
        });

        // Add just JsonSchema for unit structs (Serialize/Deserialize are custom impl'd below)
        modified_input
            .attrs
            .push(syn::parse_quote!(#[derive(schemars::JsonSchema)]));
    }

    // Generate custom serialization implementation for BASE unit structs to serialize as {} instead of null
    // (nested unit structs get GtsSerialize/GtsDeserialize impls instead)
    let custom_serialize_impl = if is_base_unit_struct {
        quote! {
            // Custom Serialize implementation for unit structs to serialize as {} instead of null
            impl #impl_generics serde::Serialize for #struct_name #ty_generics #where_clause {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    // Serialize unit struct as empty object {}
                    use serde::ser::SerializeMap;
                    let mut map = serializer.serialize_map(Some(0))?;
                    map.end()
                }
            }

            // Custom Deserialize implementation for unit structs to deserialize from {} instead of null
            impl<'de, #impl_generics> serde::Deserialize<'de> for #struct_name #ty_generics #where_clause {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    // Deserialize unit struct from empty object {} or null
                    use serde::de::{Visitor, MapAccess};
                    use std::fmt;

                    struct UnitStructVisitor #ty_generics;

                    impl<'de, #impl_generics> Visitor<'de> for UnitStructVisitor #ty_generics #where_clause {
                        type Value = #struct_name #ty_generics;

                        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                            formatter.write_str("unit struct")
                        }

                        // Handle empty object {}
                        fn visit_map<M>(self, _map: M) -> Result<Self::Value, M::Error>
                        where
                            M: MapAccess<'de>,
                        {
                            Ok(#struct_name)
                        }

                        // Handle null (for backward compatibility)
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
    } else {
        quote! {}
    };

    // Generate GtsSerialize/GtsDeserialize impls for nested structs (base = ParentStruct)
    // These structs don't have Serialize/Deserialize derives, so they need explicit impls
    let gts_serialize_impl = if matches!(&args.base, BaseAttr::Parent(_)) && !is_unit_struct {
        // Collect field information for serialization, including whether each field is the generic type
        let fields_for_serialize: Vec<_> = struct_fields
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        let ident = field.ident.as_ref()?;
                        // Use serde rename if present, otherwise use field name
                        let serialize_name =
                            get_serde_rename(field).unwrap_or_else(|| ident.to_string());
                        // Check if this field's type is the generic parameter
                        let is_generic = generic_param_name.as_ref().is_some_and(|gp| {
                            let field_type = &field.ty;
                            let field_type_str =
                                quote::quote!(#field_type).to_string().replace(' ', "");
                            field_type_str == *gp
                        });
                        Some((ident.clone(), serialize_name, is_generic))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let num_fields = fields_for_serialize.len();
        let field_serialize_calls: Vec<_> = fields_for_serialize
            .iter()
            .map(|(ident, serialize_name, is_generic)| {
                if *is_generic {
                    // For generic fields, use GtsSerializeWrapper
                    quote! {
                        state.serialize_field(#serialize_name, &::gts::GtsSerializeWrapper(&self.#ident))?;
                    }
                } else {
                    quote! {
                        state.serialize_field(#serialize_name, &self.#ident)?;
                    }
                }
            })
            .collect();

        let field_idents: Vec<_> = fields_for_serialize
            .iter()
            .map(|(ident, _, _)| ident)
            .collect();
        let field_names: Vec<_> = fields_for_serialize
            .iter()
            .map(|(_, name, _)| name.as_str())
            .collect();
        let _fields_is_generic: Vec<_> = fields_for_serialize
            .iter()
            .map(|(_, _, is_generic)| *is_generic)
            .collect();

        let struct_name_str = struct_name.to_string();

        // Get the type param identifiers for use in impl<'de, ...>
        // Include GtsSchema bound since the struct definition requires it
        let type_param_idents: Vec<_> = generics.type_params().map(|p| &p.ident).collect();

        // Build the impl generics for deserialize: impl<'de, T1: GtsSchema, T2: GtsSchema, ...>
        // The GtsSchema bound is required because the struct definition has it
        let de_impl_generics = if type_param_idents.is_empty() {
            quote! { 'de }
        } else {
            quote! { 'de, #(#type_param_idents: ::gts::GtsSchema),* }
        };

        // Build base where clause that includes GtsSchema bound (required by struct definition)
        let gts_schema_where = if let Some(ref gp) = generic_param_name {
            let gp_ident: syn::Ident = syn::parse_str(gp).expect("valid ident");
            if let Some(existing) = where_clause {
                quote! { #existing #gp_ident: ::gts::GtsSchema, }
            } else {
                quote! { where #gp_ident: ::gts::GtsSchema }
            }
        } else {
            quote! { #where_clause }
        };

        // Build where clause for GtsSerialize that includes GtsSchema + GtsSerialize bounds
        let gts_serialize_where = if let Some(ref gp) = generic_param_name {
            let gp_ident: syn::Ident = syn::parse_str(gp).expect("valid ident");
            if let Some(existing) = where_clause {
                quote! { #existing #gp_ident: ::gts::GtsSchema + ::gts::GtsSerialize, }
            } else {
                quote! { where #gp_ident: ::gts::GtsSchema + ::gts::GtsSerialize }
            }
        } else {
            quote! { #where_clause }
        };

        // Build where clause for GtsDeserialize that includes GtsSchema + GtsDeserialize bounds
        let gts_deserialize_where = if let Some(ref gp) = generic_param_name {
            let gp_ident: syn::Ident = syn::parse_str(gp).expect("valid ident");
            if let Some(existing) = where_clause {
                quote! { #existing #gp_ident: ::gts::GtsSchema + ::gts::GtsDeserialize<'de>, }
            } else {
                quote! { where #gp_ident: ::gts::GtsSchema + ::gts::GtsDeserialize<'de> }
            }
        } else {
            quote! { #where_clause }
        };

        // Generate field visit code - for generic fields, use GtsDeserializeWrapper
        let field_visit_code: Vec<_> = fields_for_serialize
            .iter()
            .map(|(ident, name, is_generic)| {
                if *is_generic {
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

        quote! {
            // GtsSerialize implementation for nested struct (no direct serialization allowed)
            impl #impl_generics ::gts::GtsSerialize for #struct_name #ty_generics #gts_serialize_where {
                fn gts_serialize<__S>(&self, serializer: __S) -> Result<__S::Ok, __S::Error>
                where
                    __S: serde::Serializer,
                {
                    use serde::ser::SerializeStruct;
                    let mut state = serializer.serialize_struct(#struct_name_str, #num_fields)?;
                    #(#field_serialize_calls)*
                    state.end()
                }
            }

            // GtsDeserialize implementation for nested struct
            impl<#de_impl_generics> ::gts::GtsDeserialize<'de> for #struct_name #ty_generics #gts_deserialize_where {
                fn gts_deserialize<__D>(deserializer: __D) -> Result<Self, __D::Error>
                where
                    __D: serde::Deserializer<'de>,
                {
                    use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
                    use std::fmt;

                    #[allow(non_camel_case_types)]
                    #[derive(serde::Deserialize)]
                    #[serde(field_identifier, rename_all = "snake_case")]
                    enum Field {
                        #(#field_idents,)*
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

                            #(let #field_idents = #field_idents
                                .ok_or_else(|| serde::de::Error::missing_field(#field_names))?;)*

                            Ok(#struct_name {
                                #(#field_idents,)*
                            })
                        }
                    }

                    const FIELDS: &[&str] = &[#(#field_names,)*];
                    deserializer.deserialize_struct(#struct_name_str, FIELDS, StructVisitor(std::marker::PhantomData))
                }
            }
        }
    } else if matches!(&args.base, BaseAttr::Parent(_)) && is_unit_struct {
        // Unit struct nested type - simple impls
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
    } else {
        quote! {}
    };

    // Block direct serde Serialize/Deserialize impls on nested structs.
    // If a user tries to add them manually, this will conflict with the blanket impls.
    let no_direct_serde_impl = if matches!(&args.base, BaseAttr::Parent(_)) {
        quote! {
            impl #impl_generics ::gts::GtsNoDirectSerialize for #struct_name #ty_generics #where_clause {}
            impl #impl_generics ::gts::GtsNoDirectDeserialize for #struct_name #ty_generics #where_clause {}
        }
    } else {
        quote! {}
    };

    // For nested structs, we don't generate instance serialization methods (gts_instance_json, etc.)
    // because they don't have Serialize. Instead, they must be serialized through their base struct.
    let instance_methods_impl = if matches!(&args.base, BaseAttr::Parent(_)) {
        quote! {}
    } else {
        quote! {
            // Instance serialization methods (require Serialize bound)
            impl #impl_generics #struct_name #ty_generics #serialize_where_clause {
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
                    serde_json::to_string_pretty(self).expect("Failed to serialize instance to JSON string")
                }
            }
        }
    };

    let expanded = quote! {
        #modified_input

        // Compile-time assertion for base struct matching (if specified)
        #base_assertion

        // Custom serialization for unit structs to serialize as {} instead of null
        #custom_serialize_impl

        // GtsSerialize/GtsDeserialize impls for nested structs
        #gts_serialize_impl
        #no_direct_serde_impl

        impl #impl_generics #struct_name #ty_generics #gts_schema_where_clause {
            /// File path where the GTS schema will be generated by the CLI.
            #[doc(hidden)]
            #[allow(dead_code)]
            const GTS_SCHEMA_FILE_PATH: &'static str = #schema_file_path;

            /// GTS schema description.
            #[doc(hidden)]
            #[allow(dead_code)]
            const GTS_SCHEMA_DESCRIPTION: &'static str = #description;

            /// Comma-separated list of properties included in the schema.
            #[doc(hidden)]
            #[allow(dead_code)]
            const GTS_SCHEMA_PROPERTIES: &'static str = #properties_str;

            #base_schema_id_const

            /// Get the GTS schema identifier as a static reference.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_schema_id() -> &'static ::gts::gts::GtsSchemaId {
                static GTS_SCHEMA_ID: std::sync::LazyLock<::gts::gts::GtsSchemaId> =
                    std::sync::LazyLock::new(|| ::gts::gts::GtsSchemaId::new(#schema_id));
                &GTS_SCHEMA_ID
            }

            /// Get the parent (base) schema identifier as a static reference.
            /// Returns `None` for base structs (those with `base = true`).
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

        // Implement GtsSchema trait for runtime schema composition
        impl #impl_generics ::gts::GtsSchema for #struct_name #ty_generics #gts_schema_where_clause {
            const SCHEMA_ID: &'static str = #schema_id;
            const GENERIC_FIELD: Option<&'static str> = #generic_field_option;

            fn gts_schema_with_refs() -> serde_json::Value {
                Self::gts_schema_with_refs_allof()
            }

            #gts_schema_impl
        }

        // Public API methods for schema serialization
        impl #impl_generics #struct_name #ty_generics #gts_schema_where_clause {
            /// Get the JSON Schema with `allOf` + `$ref` for inheritance as a JSON string.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_schema_with_refs_as_string() -> String {
                use ::gts::GtsSchema;
                serde_json::to_string(&Self::gts_schema_with_refs_allof()).expect("Failed to serialize schema")
            }

            /// Get the JSON Schema with `allOf` + `$ref` for inheritance as a pretty-printed JSON string.
            #[allow(dead_code)]
            #[must_use]
            pub fn gts_schema_with_refs_as_string_pretty() -> String {
                use ::gts::GtsSchema;
                serde_json::to_string_pretty(&Self::gts_schema_with_refs_allof()).expect("Failed to serialize schema")
            }
        }

        // Instance serialization methods (only for base structs)
        #instance_methods_impl
    };

    TokenStream::from(expanded)
}

/// Arguments for the `#[gts_well_known_instance]` macro.
/// This is a parse-only validation type; fields are validated during `Parse` and not retained.
struct GtsInstanceArgs;

impl Parse for GtsInstanceArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut dir_path: Option<String> = None;
        let mut schema_id: Option<String> = None;
        let mut instance_segment: Option<String> = None;
        let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            let key_str = key.to_string();
            if !seen_keys.insert(key_str.clone()) {
                return Err(syn::Error::new_spanned(
                    key,
                    format!(
                        "gts_well_known_instance: Duplicate attribute '{key_str}'. Each attribute must appear exactly once."
                    ),
                ));
            }

            match key_str.as_str() {
                "dir_path" => {
                    let value: LitStr = input.parse()?;
                    dir_path = Some(value.value());
                }
                "schema_id" => {
                    let value: LitStr = input.parse()?;
                    let id = value.value();
                    // schema_id must end with ~ (type marker)
                    if !id.ends_with('~') {
                        return Err(syn::Error::new_spanned(
                            value,
                            format!(
                                "gts_well_known_instance: schema_id must end with '~' (type marker), got '{id}'"
                            ),
                        ));
                    }
                    // General GTS ID validation
                    if let Err(e) = gts_id::validate_gts_id(&id, false) {
                        let msg = match &e {
                            gts_id::GtsIdError::Id { cause, .. } => {
                                format!("Invalid GTS schema ID: {cause}")
                            }
                            gts_id::GtsIdError::Segment { num, cause, .. } => {
                                format!("Segment #{num}: {cause}")
                            }
                        };
                        return Err(syn::Error::new_spanned(
                            value,
                            format!("gts_well_known_instance: {msg}"),
                        ));
                    }
                    schema_id = Some(id);
                }
                "instance_segment" => {
                    let value: LitStr = input.parse()?;
                    let seg = value.value();

                    // Reject instance_segment ending with ~ (type/schema marker)
                    if seg.ends_with('~') {
                        return Err(syn::Error::new_spanned(
                            value,
                            "gts_well_known_instance: instance_segment must not end with '~' \u{2014} that is a schema/type marker. Instance segments do not end with '~'",
                        ));
                    }

                    // Reject wildcard-only instance_segment
                    if seg == "*" {
                        return Err(syn::Error::new_spanned(
                            value,
                            "gts_well_known_instance: instance_segment must not be a bare wildcard '*'. Wildcards are not valid in generated instance IDs",
                        ));
                    }

                    // Validate the segment using gts_id::validate_segment (no wildcards)
                    if let Err(cause) = gts_id::validate_segment(2, &seg, false) {
                        return Err(syn::Error::new_spanned(
                            value,
                            format!(
                                "gts_well_known_instance: Invalid instance_segment '{seg}': {cause}"
                            ),
                        ));
                    }

                    instance_segment = Some(seg);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "gts_well_known_instance: Unknown attribute. Expected: dir_path, schema_id, instance_segment",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        // Validate composed instance ID after both parts are parsed
        let schema_id_val = schema_id.ok_or_else(|| {
            input.error("gts_well_known_instance: Missing required attribute: schema_id")
        })?;
        let instance_segment_val = instance_segment.ok_or_else(|| {
            input.error("gts_well_known_instance: Missing required attribute: instance_segment")
        })?;
        let _dir_path_val = dir_path.ok_or_else(|| {
            input.error("gts_well_known_instance: Missing required attribute: dir_path")
        })?;

        // Validate the composed instance ID (schema_id + instance_segment forms a valid GTS instance ID)
        let composed = format!("{schema_id_val}{instance_segment_val}");
        if let Err(e) = gts_id::validate_gts_id(&composed, false) {
            let msg = match &e {
                gts_id::GtsIdError::Id { cause, .. } => {
                    format!("Invalid composed instance ID '{composed}': {cause}")
                }
                gts_id::GtsIdError::Segment { num, cause, .. } => {
                    format!("Invalid composed instance ID '{composed}': Segment #{num}: {cause}")
                }
            };
            return Err(input.error(format!("gts_well_known_instance: {msg}")));
        }

        Ok(GtsInstanceArgs)
    }
}

/// Declare a well-known GTS instance as a const JSON string literal.
///
/// This macro:
/// 1. **At compile time**: validates the `schema_id` and `instance_segment` GTS ID formats
///    and verifies the annotated item is a `const` of type `&str`.
/// 2. **At generate time**: the CLI (`gts generate-from-rust --mode instances`) scans for
///    these annotations, validates the JSON payload, injects the `"id"` field, and writes
///    `{dir_path}/{schema_id}{instance_segment}.instance.json`.
///
/// The macro passes the annotated `const` item through unchanged -- it is purely metadata
/// for the CLI extraction step.
///
/// # Arguments
///
/// * `dir_path` - Output directory for the generated instance file (relative to crate root or `--output`)
/// * `schema_id` - GTS schema ID this instance conforms to (must end with `~`)
/// * `instance_segment` - Appended to `schema_id` to form the full instance ID (must not end with `~`)
///
/// # Example
///
/// ```ignore
/// use gts_macros::gts_well_known_instance;
///
/// #[gts_well_known_instance(
///     dir_path = "instances",
///     schema_id = "gts.x.core.events.topic.v1~",
///     instance_segment = "x.commerce._.orders.v1.0"
/// )]
/// const ORDERS_TOPIC: &str = r#"{
///     "name": "orders",
///     "description": "Order lifecycle events topic",
///     "retention": "P90D",
///     "partitions": 16
/// }"#;
/// ```
///
/// The CLI generates:
/// `instances/gts.x.core.events.topic.v1~x.commerce._.orders.v1.0.instance.json`
/// with an injected `"id"` field: `"gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"`.
#[proc_macro_attribute]
pub fn gts_well_known_instance(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _args = parse_macro_input!(attr as GtsInstanceArgs);

    // Parse the annotated item -- must be a const item
    let item_clone = item.clone();
    let parsed: syn::Result<syn::ItemConst> = syn::parse(item_clone);

    match parsed {
        Err(_) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "gts_well_known_instance: Only `const` items are supported. \
                 Usage: `const NAME: &str = r#\"{ ... }\"#;`",
            )
            .to_compile_error()
            .into();
        }
        Ok(item_const) => {
            // Validate the const has type &str (or &'lifetime str) using AST matching.
            // This is more robust than stringifying tokens and avoids false rejections
            // from formatting differences (spaces, lifetime names, etc.).
            let ty = &item_const.ty;
            let is_ref_str = match ty.as_ref() {
                syn::Type::Reference(syn::TypeReference { elem, .. }) => {
                    matches!(elem.as_ref(), syn::Type::Path(p) if p.qself.is_none() && p.path.is_ident("str"))
                }
                _ => false,
            };
            if !is_ref_str {
                let ty_str = quote::quote!(#ty).to_string().replace(' ', "");
                return syn::Error::new_spanned(
                    ty,
                    format!(
                        "gts_well_known_instance: The annotated const must have type `&str`, got `{ty_str}`. \
                         Usage: `const NAME: &str = r#\"{{ ... }}\"#;`"
                    ),
                )
                .to_compile_error()
                .into();
            }

            // Validate the const value is a string literal (not a macro invocation)
            match item_const.expr.as_ref() {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(_),
                    ..
                }) => {}
                _ => {
                    return syn::Error::new_spanned(
                        &item_const.expr,
                        "gts_well_known_instance: The const value must be a string literal \
                         (raw string `r#\"...\"#` or regular string `\"...\"`). \
                         Macro invocations like `concat!()` are not supported.",
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
    }

    // Pass the item through unchanged -- this macro is purely metadata for the CLI
    item
}
