//! Parsing for `#[gts(...)]` struct-level attributes.
//!
//! Parses the top-level `#[gts(...)]` attribute on a struct into [`GtsAttrs`],
//! which contains the schema metadata needed for `#[derive(GtsSchema)]`.

use syn::{LitStr, Token, parse::ParseStream};

/// Parsed struct-level `#[gts(...)]` attributes.
pub struct GtsAttrs {
    pub dir_path: String,
    pub schema_id: String,
    pub description: String,
    pub extends: Option<syn::Ident>,
}

impl GtsAttrs {
    /// Parse `#[gts(...)]` from the attributes on a `DeriveInput`.
    ///
    /// Returns an error if:
    /// - No `#[gts(...)]` attribute is found
    /// - Required keys (`dir_path`, `schema_id`, `description`) are missing
    /// - Unknown keys are present
    pub fn from_derive_input(input: &syn::DeriveInput) -> syn::Result<Self> {
        let gts_attr = input
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("gts"))
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &input.ident,
                    "GtsSchema: missing #[gts(...)] attribute. Add #[gts(dir_path = \"...\", schema_id = \"...\", description = \"...\")]",
                )
            })?;

        gts_attr.parse_args_with(|stream: ParseStream| Self::parse_inner(stream, &input.ident))
    }

    fn parse_inner(input: ParseStream, struct_ident: &syn::Ident) -> syn::Result<Self> {
        let mut dir_path: Option<String> = None;
        let mut schema_id: Option<String> = None;
        let mut description: Option<String> = None;
        let mut extends: Option<syn::Ident> = None;

        // Tracks which keys have already been parsed so duplicates emit a clear error
        // rather than silently overwriting.
        let mut seen_extends = false;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let key_str = key.to_string();

            match key_str.as_str() {
                "dir_path" => {
                    if dir_path.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "GtsSchema: duplicate attribute 'dir_path'",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    dir_path = Some(value.value());
                }
                "schema_id" => {
                    if schema_id.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "GtsSchema: duplicate attribute 'schema_id'",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    schema_id = Some(value.value());
                }
                "description" => {
                    if description.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "GtsSchema: duplicate attribute 'description'",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    description = Some(value.value());
                }
                "extends" => {
                    if seen_extends {
                        return Err(syn::Error::new_spanned(
                            key,
                            "GtsSchema: duplicate attribute 'extends'",
                        ));
                    }
                    seen_extends = true;
                    input.parse::<Token![=]>()?;
                    let ident: syn::Ident = input.parse()?;
                    // `extends = None` is an explicit root-marker equivalent to omitting
                    // `extends` — ADR §Struct-Level Attributes. Both forms leave `extends`
                    // unset and are treated identically downstream.
                    if ident == "None" {
                        extends = None;
                    } else {
                        extends = Some(ident);
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        format!(
                            "GtsSchema: unknown attribute '{key_str}'. \
                             Expected: dir_path, schema_id, description, or extends"
                        ),
                    ));
                }
            }

            if !input.is_empty() {
                if !input.peek(Token![,]) {
                    return Err(syn::Error::new(
                        input.span(),
                        "GtsSchema: expected `,` between attributes",
                    ));
                }
                input.parse::<Token![,]>()?;
            }
        }

        Ok(GtsAttrs {
            dir_path: dir_path.ok_or_else(|| {
                syn::Error::new_spanned(
                    struct_ident,
                    "GtsSchema: missing required attribute 'dir_path' in #[gts(...)]",
                )
            })?,
            schema_id: schema_id.ok_or_else(|| {
                syn::Error::new_spanned(
                    struct_ident,
                    "GtsSchema: missing required attribute 'schema_id' in #[gts(...)]",
                )
            })?,
            description: description.ok_or_else(|| {
                syn::Error::new_spanned(
                    struct_ident,
                    "GtsSchema: missing required attribute 'description' in #[gts(...)]",
                )
            })?,
            extends,
        })
    }
}
