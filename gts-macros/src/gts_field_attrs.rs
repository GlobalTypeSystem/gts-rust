//! Parsing for field-level `#[gts(...)]` attributes.
//!
//! Parses `#[gts(type_field)]`, `#[gts(instance_id)]`, and `#[gts(skip)]`
//! on individual struct fields.

use syn::spanned::Spanned;

/// A single parsed field-level `#[gts(...)]` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GtsFieldAttr {
    /// `#[gts(type_field)]` — marks a `GtsSchemaId` field as the GTS type discriminator.
    TypeField,
    /// `#[gts(instance_id)]` — marks a `GtsInstanceId` field as the GTS instance ID.
    InstanceId,
    /// `#[gts(skip)]` — excludes the field from the generated JSON Schema properties.
    Skip,
}

/// Parsed field-level GTS attributes for a single field.
pub struct FieldGtsAttrs {
    pub attr: Option<GtsFieldAttr>,
}

impl FieldGtsAttrs {
    /// Parse all `#[gts(...)]` attributes on a single field.
    ///
    /// Returns an error for unknown attributes or duplicate `#[gts(...)]` blocks.
    pub fn from_field(field: &syn::Field) -> syn::Result<Self> {
        let mut result = None;

        for attr in &field.attrs {
            if !attr.path().is_ident("gts") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                let ident = meta.path.get_ident().ok_or_else(|| {
                    syn::Error::new(meta.path.span(), "GtsSchema: expected an identifier")
                })?;

                let parsed = match ident.to_string().as_str() {
                    "type_field" => GtsFieldAttr::TypeField,
                    "instance_id" => GtsFieldAttr::InstanceId,
                    "skip" => GtsFieldAttr::Skip,
                    other => {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "GtsSchema: unknown field attribute '{other}'. \
                                 Expected: type_field, instance_id, or skip"
                            ),
                        ));
                    }
                };

                if result.is_some() {
                    return Err(syn::Error::new(
                        ident.span(),
                        "GtsSchema: only one #[gts(...)] attribute per field is allowed",
                    ));
                }

                result = Some(parsed);
                Ok(())
            })?;
        }

        Ok(FieldGtsAttrs { attr: result })
    }
}
