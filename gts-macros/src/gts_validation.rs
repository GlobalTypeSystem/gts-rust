//! Validation logic for `#[derive(GtsSchema)]`.
//!
//! All compile-time checks that apply to the new macro design.

use crate::gts_attrs::GtsAttrs;
use crate::gts_field_attrs::{FieldGtsAttrs, GtsFieldAttr};
use crate::{
    count_schema_segments, extract_schema_version, extract_struct_version, is_type_gts_instance_id,
    is_type_gts_schema_id,
};
use syn::spanned::Spanned;

/// Run all validations for `#[derive(GtsSchema)]`.
pub fn validate_all(
    input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> syn::Result<()> {
    validate_struct_shape(input)?;
    validate_schema_id_format(&attrs.schema_id, input)?;
    validate_version_match(&input.ident, &attrs.schema_id)?;
    validate_segment_count(input, attrs)?;
    validate_generics(input)?;
    validate_field_gts_attrs(input, attrs, field_attrs)?;
    Ok(())
}

/// Only named structs are supported (no tuple structs, enums, or unions).
fn validate_struct_shape(input: &syn::DeriveInput) -> syn::Result<()> {
    match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(_) | syn::Fields::Unit => Ok(()),
            syn::Fields::Unnamed(_) => Err(syn::Error::new_spanned(
                &input.ident,
                "GtsSchema: tuple structs are not supported. Use a named struct instead.",
            )),
        },
        syn::Data::Enum(_) => Err(syn::Error::new_spanned(
            &input.ident,
            "GtsSchema: enums are not supported. Use a named struct instead.",
        )),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            &input.ident,
            "GtsSchema: unions are not supported. Use a named struct instead.",
        )),
    }
}

/// Validate the `schema_id` format via `gts_id::validate_gts_id()`.
fn validate_schema_id_format(schema_id: &str, input: &syn::DeriveInput) -> syn::Result<()> {
    // Schema IDs must end with ~ (type marker)
    if !schema_id.ends_with('~') {
        return Err(syn::Error::new_spanned(
            &input.ident,
            format!("GtsSchema: invalid schema_id '{schema_id}': must end with '~' (type marker)"),
        ));
    }

    if let Err(e) = gts_id::validate_gts_id(schema_id, false) {
        let msg = match &e {
            gts_id::GtsIdError::Id { cause, .. } => {
                format!("Invalid GTS schema ID: {cause}")
            }
            gts_id::GtsIdError::Segment { num, cause, .. } => {
                format!("Segment #{num}: {cause}")
            }
        };
        return Err(syn::Error::new_spanned(
            &input.ident,
            format!("GtsSchema: {msg}"),
        ));
    }

    Ok(())
}

/// Validate version consistency between struct name and schema ID.
fn validate_version_match(struct_ident: &syn::Ident, schema_id: &str) -> syn::Result<()> {
    let struct_name = struct_ident.to_string();
    let struct_version = extract_struct_version(&struct_name);
    let schema_version = extract_schema_version(schema_id);

    match (struct_version, schema_version) {
        (Some(sv), Some(schv)) if sv != schv => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "GtsSchema: version mismatch between struct name and schema_id. \
                 Struct '{struct_name}' has version suffix '{}' but schema_id '{schema_id}' \
                 has version '{}'. The versions must match exactly \
                 (e.g., BaseEventV1 with v1~, or BaseEventV2_0 with v2.0~)",
                sv.to_struct_suffix(),
                schv.to_schema_version()
            ),
        )),
        (Some(_), Some(_)) => Ok(()),
        (None, Some(schv)) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "GtsSchema: schema_id '{schema_id}' has a version but struct '{struct_name}' \
                 does not have a version suffix. Add '{}' suffix to the struct name \
                 (e.g., '{struct_name}{}')",
                schv.to_struct_suffix(),
                schv.to_struct_suffix()
            ),
        )),
        (Some(sv), None) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "GtsSchema: struct '{struct_name}' has version suffix '{}' but \
                 cannot extract version from schema_id '{schema_id}'. \
                 Expected format with version like 'gts.x.foo.v1~' or 'gts.x.foo.v1.0~'",
                sv.to_struct_suffix()
            ),
        )),
        (None, None) => Err(syn::Error::new_spanned(
            struct_ident,
            format!(
                "GtsSchema: both struct name and schema_id must have a version. \
                 Struct '{struct_name}' has no version suffix (e.g., V1) and schema_id '{schema_id}' \
                 has no version (e.g., v1~). Add version to both (e.g., '{struct_name}V1' with 'gts.x.foo.v1~')"
            ),
        )),
    }
}

/// Validate segment count matches extends presence.
///
/// - No `extends` → exactly 1 segment (root type)
/// - `extends = Parent` → 2+ segments (derived type)
fn validate_segment_count(input: &syn::DeriveInput, attrs: &GtsAttrs) -> syn::Result<()> {
    let segment_count = count_schema_segments(&attrs.schema_id);

    match (&attrs.extends, segment_count) {
        (None, count) if count > 1 => Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "GtsSchema: schema_id '{}' has {count} segments but no 'extends' is specified. \
                 A root type must have exactly 1 segment. Either add 'extends = ParentStruct' \
                 or remove extra segments from schema_id.",
                attrs.schema_id
            ),
        )),
        (Some(_), count) if count < 2 => Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "GtsSchema: 'extends' is specified but schema_id '{}' has only {count} segment. \
                 A derived type must have at least 2 segments. Either remove 'extends' \
                 or add a parent segment to schema_id.",
                attrs.schema_id
            ),
        )),
        _ => Ok(()),
    }
}

/// At most 1 generic type parameter.
fn validate_generics(input: &syn::DeriveInput) -> syn::Result<()> {
    let type_params: Vec<_> = input.generics.type_params().collect();
    if type_params.len() > 1 {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "GtsSchema: at most one generic type parameter is allowed. \
             GTS inheritance is single-chain, not multi-branch.",
        ));
    }
    Ok(())
}

/// Validate field-level `#[gts(...)]` attributes:
/// - `#[gts(type_field)]` must be on a `GtsSchemaId` field
/// - `#[gts(instance_id)]` must be on a `GtsInstanceId` field
/// - `#[gts(type_field)]` and `#[gts(instance_id)]` are mutually exclusive
/// - At most one of each per struct
/// - Root structs (no `extends`) must declare exactly one of `type_field` / `instance_id`
/// - Derived structs (`extends = Parent`) must declare neither — the root's chained
///   identifier already carries the GTS type, and a redundant identity field on a nested
///   struct produces an ambiguous serialized shape
fn validate_field_gts_attrs(
    input: &syn::DeriveInput,
    attrs: &GtsAttrs,
    field_attrs: &[(syn::Field, FieldGtsAttrs)],
) -> syn::Result<()> {
    let mut has_type_field = false;
    let mut has_instance_id = false;

    for (field, field_attr) in field_attrs {
        let Some(attr) = &field_attr.attr else {
            continue;
        };

        match attr {
            GtsFieldAttr::TypeField => {
                if has_type_field {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: duplicate #[gts(type_field)]. Only one type_field per struct is allowed.",
                    ));
                }
                if has_instance_id {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: #[gts(type_field)] and #[gts(instance_id)] are mutually exclusive. \
                         A struct's instances are either well-known (instance_id) or anonymous (type_field), not both.",
                    ));
                }
                if attrs.extends.is_some() {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: #[gts(type_field)] is only valid on root structs (no `extends`). \
                         Derived structs inherit their GTS type from the root's chained identifier; \
                         a redundant identity field on a nested struct produces an ambiguous serialized shape.",
                    ));
                }
                if !is_type_gts_schema_id(&field.ty) {
                    return Err(syn::Error::new(
                        field.ty.span(),
                        "GtsSchema: #[gts(type_field)] must be on a field of type GtsSchemaId. \
                         The type field identifies the GTS schema type (ending with ~).",
                    ));
                }
                has_type_field = true;
            }
            GtsFieldAttr::InstanceId => {
                if has_instance_id {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: duplicate #[gts(instance_id)]. Only one instance_id per struct is allowed.",
                    ));
                }
                if has_type_field {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: #[gts(type_field)] and #[gts(instance_id)] are mutually exclusive. \
                         A struct's instances are either well-known (instance_id) or anonymous (type_field), not both.",
                    ));
                }
                if attrs.extends.is_some() {
                    return Err(syn::Error::new(
                        field.span(),
                        "GtsSchema: #[gts(instance_id)] is only valid on root structs (no `extends`). \
                         Derived structs inherit their GTS type from the root's chained identifier; \
                         a redundant identity field on a nested struct produces an ambiguous serialized shape.",
                    ));
                }
                if !is_type_gts_instance_id(&field.ty) {
                    return Err(syn::Error::new(
                        field.ty.span(),
                        "GtsSchema: #[gts(instance_id)] must be on a field of type GtsInstanceId. \
                         The instance ID identifies a specific well-known GTS instance.",
                    ));
                }
                has_instance_id = true;
            }
            GtsFieldAttr::Skip => {
                // No additional validation needed for skip
            }
        }
    }

    if attrs.extends.is_none() && !has_type_field && !has_instance_id {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "GtsSchema: root structs must declare exactly one of #[gts(type_field)] or #[gts(instance_id)]. \
             The identity field guarantees GTS type is always deducible from any instance - required for \
             Type Registry lookups, RPC/MCP payload dispatch, and Event consumers. \
             Annotate the schema-id field with #[gts(type_field)] (for anonymous instances) or \
             #[gts(instance_id)] (for well-known instances).",
        ));
    }

    Ok(())
}
