//! Field names as Schemars derives them when its container naming rule replaces serde's.

use syn::{Meta, Token, parse::ParseStream, parse_quote, punctuated::Punctuated};

/// `input` with the container's `#[schemars(rename_all ...)]` in place of serde's
/// `rename_all`, as `schemars_derive` merges them; `!rename_all` only removes serde's.
/// `None` without that directive, or if an attribute cannot be read.
pub fn schemars_rename_all(input: &syn::DeriveInput) -> Option<syn::DeriveInput> {
    fn parse_option(input: ParseStream) -> syn::Result<(bool, Meta)> {
        let negated = input.parse::<Option<Token![!]>>()?.is_some();
        Ok((negated, input.parse()?))
    }

    let mut directive = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("schemars")) {
        let options = attr
            .parse_args_with(|input: ParseStream| {
                Punctuated::<_, Token![,]>::parse_terminated_with(input, parse_option)
            })
            .ok()?;
        directive = options
            .into_iter()
            .filter(|(_, meta)| meta.path().is_ident("rename_all"))
            .last()
            .or(directive);
    }
    let (negated, directive) = directive?;

    let mut view = input.clone();
    let mut attrs = Vec::with_capacity(view.attrs.len() + 1);
    for attr in view.attrs.drain(..) {
        if !attr.path().is_ident("serde") {
            attrs.push(attr);
            continue;
        }
        let kept: Vec<Meta> = attr
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .ok()?
            .into_iter()
            .filter(|meta| !meta.path().is_ident("rename_all"))
            .collect();
        if !kept.is_empty() {
            attrs.push(parse_quote!(#[serde(#(#kept),*)]));
        }
    }
    if !negated {
        attrs.push(parse_quote!(#[serde(#directive)]));
    }
    view.attrs = attrs;
    Some(view)
}
