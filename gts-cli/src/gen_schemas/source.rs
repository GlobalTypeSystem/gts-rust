//! Read schema declarations without compiling or writing files.

use super::{BaseAttr, MacroAttrs, SchemaDeclaration, build_json_schema};
use anyhow::{Context, Result, bail};
use gts::{GTS_ID_PREFIX, GtsTypeId};
use quote::ToTokens;
use std::{collections::HashMap, path::Path};
use syn::{Expr, Lit, Meta, Token, punctuated::Punctuated, visit::Visit};

fn string_value(expr: &Expr) -> Result<String> {
    if let Expr::Lit(expr) = expr
        && let Lit::Str(value) = &expr.lit
    {
        return Ok(value.value());
    }
    bail!("Expected a string literal")
}

fn type_id_value(expr: &Expr) -> Result<String> {
    let id = if let Expr::Macro(expr) = expr
        && expr
            .mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "gts_id")
    {
        let suffix = syn::parse2::<syn::LitStr>(expr.mac.tokens.clone())?.value();
        format!("{GTS_ID_PREFIX}{suffix}")
    } else {
        string_value(expr)?
    };
    GtsTypeId::try_new(&id).with_context(|| format!("Invalid GTS type ID: {id}"))?;
    Ok(id)
}

type MacroArgs = Punctuated<Meta, Token![,]>;

#[cfg(test)]
pub(super) fn parse_macro_attrs(attr_body: &str) -> Result<MacroAttrs> {
    use syn::parse::Parser;
    macro_attrs(MacroArgs::parse_terminated.parse_str(attr_body)?)
}

fn macro_attrs(attributes: MacroArgs) -> Result<MacroAttrs> {
    let mut type_id = None;
    let mut description = None;
    let mut properties = None;
    let mut base = None;

    for attribute in attributes {
        let Meta::NameValue(attribute) = attribute else {
            bail!("Expected a macro argument of the form key = value");
        };
        let key = attribute.path.to_token_stream().to_string();
        match key.as_str() {
            "type_id" | "schema_id" => {
                if type_id.is_some() {
                    bail!("Specify either type_id or schema_id, not both");
                }
                type_id = Some(type_id_value(&attribute.value)?);
            }
            "description" => description = Some(string_value(&attribute.value)?),
            "properties" => properties = Some(string_value(&attribute.value)?),
            "base" => {
                base = Some(match &attribute.value {
                    Expr::Lit(expr) if matches!(&expr.lit, Lit::Bool(value) if value.value) => {
                        BaseAttr::IsBase
                    }
                    Expr::Path(expr) if expr.path.get_ident().is_some() => {
                        BaseAttr::Parent(expr.path.to_token_stream().to_string())
                    }
                    _ => bail!("base must be true or a parent struct name"),
                });
            }
            "dir_path" => bail!("dir_path has been removed; use --output DIR"),
            "traits_schema" | "traits" | "gts_abstract" | "gts_final" => {}
            _ => bail!("Unknown struct_to_gts_schema argument: {key}"),
        }
    }

    Ok(MacroAttrs {
        type_id: type_id.context("Missing required attribute: type_id")?,
        description,
        properties,
        base: base.context("Missing required attribute: base")?,
    })
}

#[derive(Default)]
struct Structs<'ast>(Vec<&'ast syn::ItemStruct>);

impl<'ast> Visit<'ast> for Structs<'ast> {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.0.push(item);
    }
}

/// Only files mentioning the macro are parsed; parse errors in those are fatal.
pub(super) fn extract_schemas(content: &str, source_file: &Path) -> Result<Vec<SchemaDeclaration>> {
    if !content.contains("struct_to_gts_schema") {
        return Ok(Vec::new());
    }
    let syntax = syn::parse_file(content)
        .with_context(|| format!("Failed to parse Rust source {}", source_file.display()))?;
    let mut structs = Structs::default();
    structs.visit_file(&syntax);
    let mut schemas = Vec::new();

    for item in structs.0 {
        for attribute in &item.attrs {
            if attribute
                .path()
                .segments
                .last()
                .is_none_or(|s| s.ident != "struct_to_gts_schema")
            {
                continue;
            }
            let attrs = attribute
                .parse_args_with(MacroArgs::parse_terminated)
                .map_err(anyhow::Error::from)
                .and_then(macro_attrs)
                .with_context(|| {
                    format!(
                        "Invalid schema declaration {}:{}",
                        source_file.display(),
                        item.ident
                    )
                })?;
            let field_types: HashMap<_, _> = item
                .fields
                .iter()
                .filter_map(|field| {
                    field.ident.as_ref().map(|ident| {
                        // The builder recognizes compact Rust types, e.g. Option<String>.
                        (
                            ident.to_string().trim_start_matches("r#").to_owned(),
                            field.ty.to_token_stream().to_string().replace(' ', ""),
                        )
                    })
                })
                .collect();
            let struct_name = item.ident.to_string();
            let schema = build_json_schema(
                &attrs.type_id,
                &struct_name,
                attrs.description.as_deref(),
                attrs.properties.as_deref(),
                &attrs.base,
                &field_types,
            );
            schemas.push(SchemaDeclaration {
                type_id: attrs.type_id,
                schema,
                source_file: source_file.to_owned(),
                struct_name,
            });
        }
    }
    Ok(schemas)
}
