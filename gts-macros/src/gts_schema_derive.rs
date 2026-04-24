//! Implementation of `#[derive(GtsSchema)]`.
//!
//! Parses and validates `#[gts(...)]` attributes, then generates the
//! `GtsSchema` trait implementation, runtime API, and associated constants.

use proc_macro2::TokenStream;

use crate::gts_attrs::GtsAttrs;
use crate::gts_codegen;
use crate::gts_field_attrs::FieldGtsAttrs;
use crate::gts_serde;
use crate::gts_validation;

/// Entry point for `#[derive(GtsSchema)]`.
pub fn derive_gts_schema(input: &syn::DeriveInput) -> TokenStream {
    match derive_gts_schema_inner(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

fn derive_gts_schema_inner(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    // 1. Parse struct-level #[gts(...)] attributes
    let attrs = GtsAttrs::from_derive_input(input)?;

    // 2. Parse field-level #[gts(...)] attributes
    let field_attrs = parse_field_attrs(input)?;

    // 3. Run all validations
    gts_validation::validate_all(input, &attrs, &field_attrs)?;

    // 4. Generate code (trait impl + runtime API)
    let mut tokens = gts_codegen::generate(input, &attrs, &field_attrs);

    // 5. Generate serde-related code (Serialize/Deserialize, GtsSerialize/GtsDeserialize)
    tokens.extend(gts_serde::generate(input, &attrs, &field_attrs));

    Ok(tokens)
}

/// Parse field-level `#[gts(...)]` attributes from all named fields.
fn parse_field_attrs(input: &syn::DeriveInput) -> syn::Result<Vec<(syn::Field, FieldGtsAttrs)>> {
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => &named.named,
            syn::Fields::Unit | syn::Fields::Unnamed(_) => return Ok(Vec::new()),
        },
        _ => return Ok(Vec::new()), // caught by validation
    };

    fields
        .iter()
        .map(|field| {
            let attrs = FieldGtsAttrs::from_field(field)?;
            Ok((field.clone(), attrs))
        })
        .collect()
}
