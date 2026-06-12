//! Construction macros for GTS instances — `gts_instance!` and
//! `gts_instance_raw!`.
//!
//! `gts_instance!` accepts a Rust struct literal where the GTS instance id
//! is supplied as a string literal in the dedicated id field
//! (`id` / `gts_id` / `gtsId` — see [`crate::ID_FIELD_NAMES`]). The macro
//! parses the literal, validates its shape per GTS spec §2.2 / §3.7,
//! splits it into prefix + segment, and rewrites the field's value to a
//! `GtsInstanceId::new(prefix, segment)` call. The prefix half is then
//! const-asserted against `<S as GtsSchema>::TYPE_ID` so a literal that
//! claims to belong to a different schema fails the build.
//!
//! For chained schemas the const-assert target is derived from the struct
//! literal's turbofish: the macro descends through angle-bracketed type
//! args, picking the deepest non-generic path as the conforming type. So
//! `BaseV1::<LeafV1> { ... }` targets `<LeafV1 as GtsSchema>::TYPE_ID`
//! (the full chain), and `BaseV1::<MiddleV1<LeafV1>> { ... }` likewise
//! targets `LeafV1`. `BaseV1::<()> { ... }` keeps the carrier itself as
//! the target (base-level instance). For chained schemas the turbofish is
//! mandatory — bare `BaseV1 { ... }` fails to compile because Rust needs
//! explicit generics in trait position.
//!
//! `#[gts_static(NAME)]` is the only outer attribute supported: it wraps
//! the produced value in a `pub static NAME: LazyLock<T>` binding (item
//! form).
//!
//! `gts_instance_raw!` is a separate, JSON-shaped form for instances that
//! have no Rust struct counterpart; its syntax is not affected by this
//! file's typed-form rework.
//!
//! The `#[proc_macro]` entry points must live at the crate root (Rust
//! restriction); see `lib.rs` for the thin shims that call into
//! [`expand_gts_instance`] and [`expand_gts_instance_raw`] here.

use crate::ID_FIELD_NAMES;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Expr, ExprLit, ExprStruct, FieldValue, GenericArgument, Ident, Lit, LitStr, Path,
    PathArguments, Token, Type, parse2,
};

/// Validate an `instance_id` literal against the full GTS spec via the
/// shared `gts-id` validator. Catches malformed segments (missing
/// `<vendor>.<package>.<namespace>.<type>.v<N>` shape, bad version
/// suffix, etc.) at compile time with span pointing at the literal.
fn validate_instance_id_format(instance_id: &LitStr) -> syn::Result<()> {
    let raw = instance_id.value();
    if let Err(e) = gts_id::parse_gts_id(&raw, false) {
        let msg = format!("Invalid GTS instance ID: {e}");
        return Err(syn::Error::new_spanned(instance_id, msg));
    }
    Ok(())
}

/// Split a full `instance_id` literal into `(type_id, segment)` per GTS
/// spec §2.2 / §3.7. The id must contain at least one `~` and must not end
/// with `~` (that form denotes a schema). `type_id` includes the trailing
/// `~`; `segment` is everything after it. Assumes the literal is already
/// format-validated via [`validate_instance_id_format`].
fn split_instance_id(instance_id: &LitStr) -> syn::Result<(String, String)> {
    let raw = instance_id.value();
    if raw.ends_with('~') {
        return Err(syn::Error::new_spanned(
            instance_id,
            "instance id literal must not end with `~` (that denotes a schema, not an instance)",
        ));
    }
    let Some(last_tilde) = raw.rfind('~') else {
        return Err(syn::Error::new_spanned(
            instance_id,
            "instance id literal must contain at least one `~` (chained form: gts.<type>~<instance>)",
        ));
    };
    let type_id = raw[..=last_tilde].to_owned();
    let segment = raw[last_tilde + 1..].to_owned();
    Ok((type_id, segment))
}

/// Strip turbofish (`Foo::<T>`) from a path so it can be used in *type*
/// position (`Foo<T>`). Used both for the const-assert target (qualified
/// path syntax `<X as Trait>::ASSOC` requires type position) and for the
/// `LazyLock<T>` binding when `#[gts_static(NAME)]` is given.
fn path_for_type_position(path: &Path) -> Path {
    let mut cloned = path.clone();
    for seg in &mut cloned.segments {
        if let PathArguments::AngleBracketed(args) = &mut seg.arguments {
            args.colon2_token = None;
        }
    }
    cloned
}

/// Derive the const-assert target type from the struct literal's path by
/// recursively descending into the last angle-bracketed type argument
/// until a path with no angle args is reached.
///
/// Examples (left = struct path, right = derived target):
/// - `TopicV1`                      → `TopicV1` (no descent)
/// - `BaseV1::<LeafV1>`             → `LeafV1`
/// - `BaseV1::<MiddleV1<LeafV1>>`   → `LeafV1`
/// - `BaseV1::<()>`                 → `BaseV1::<()>` (carrier kept; `()`
///   has empty `TYPE_ID`, so const-assert must hit the carrier)
///
/// The deepest type is expected to carry `struct_to_gts_schema` (i.e.
/// implement `GtsSchema`); if it doesn't, the emitted `<X as GtsSchema>`
/// reference fails to typecheck — surfacing a clear error at the call
/// site without any extra macro logic.
fn derive_schema_target_from_path(path: &Path) -> Path {
    let Some(last_seg) = path.segments.last() else {
        return path.clone();
    };
    let PathArguments::AngleBracketed(args) = &last_seg.arguments else {
        return path.clone();
    };
    let last_type_arg = args.args.iter().rev().find_map(|arg| match arg {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    match last_type_arg {
        Some(Type::Path(type_path)) => derive_schema_target_from_path(&type_path.path),
        // `()` and other non-path types: stop, keep the carrier (with its
        // explicit args) as the assert target.
        _ => path.clone(),
    }
}

/// Parsed args for `gts_instance!`. Input shape:
///
/// ```text
/// #[gts_static(StaticName)]?
/// StructPath { id: "...", ...other fields... }
/// ```
struct TypedInstanceArgs {
    /// The struct literal as the user wrote it. Mutated downstream to
    /// rewrite the id-field's string-literal value into a
    /// `GtsInstanceId::new(prefix, segment)` call.
    instance: ExprStruct,
    /// Optional binding name from `#[gts_static(NAME)]`; when set, the
    /// macro emits a `pub static NAME: LazyLock<T>` instead of a bare
    /// expression.
    static_name: Option<Ident>,
}

impl Parse for TypedInstanceArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let mut static_name: Option<Ident> = None;

        for attr in attrs {
            if !attr.path().is_ident("gts_static") {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "unknown attribute on gts_instance! struct literal; expected `#[gts_static(...)]`",
                ));
            }
            // Outer-style attrs only — inner (`#![...]`) doesn't make
            // sense on a struct literal and would be confusing.
            if matches!(attr.style, syn::AttrStyle::Inner(_)) {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "gts_instance! attributes must use outer style `#[...]`, not inner `#![...]`",
                ));
            }
            if static_name.is_some() {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "duplicate `#[gts_static(...)]`",
                ));
            }
            static_name = Some(attr.parse_args::<Ident>().map_err(|e| {
                syn::Error::new(
                    e.span(),
                    "expected `#[gts_static(NAME)]` - an identifier for the emitted `pub static`",
                )
            })?);
        }

        let instance: ExprStruct = input.parse().map_err(|e| {
            syn::Error::new(
                e.span(),
                "expected a struct literal: `StructPath { id: \"gts...\", ...other fields... }`",
            )
        })?;
        if !input.is_empty() {
            return Err(input.error(
                "unexpected tokens after struct literal; gts_instance! takes a single struct literal optionally preceded by `#[gts_static(...)]`",
            ));
        }
        Ok(Self {
            instance,
            static_name,
        })
    }
}

/// Locate the GTS instance-id field in the user's struct literal and
/// extract its string-literal value. Returns the field's index in
/// `instance.fields`, the chosen id-field identifier, and the parsed
/// `LitStr`. Errors on missing / duplicate / non-string-literal id field
/// with diagnostics that point at the offending span.
fn extract_id_field(instance: &ExprStruct) -> syn::Result<(usize, Ident, LitStr)> {
    if let Some(rest) = &instance.rest {
        return Err(syn::Error::new_spanned(
            rest,
            "struct update syntax (`..rest`) is not supported; list all fields explicitly",
        ));
    }

    let mut found: Option<(usize, Ident, LitStr)> = None;
    for (idx, field) in instance.fields.iter().enumerate() {
        let syn::Member::Named(ident) = &field.member else {
            continue;
        };
        let name = ident.to_string();
        if !ID_FIELD_NAMES.contains(&name.as_str()) {
            continue;
        }
        if let Some((_, prev_ident, _)) = &found {
            return Err(syn::Error::new_spanned(
                field,
                format!(
                    "ambiguous id field: both `{prev_ident}` and `{name}` are reserved GTS instance-id field names; use exactly one"
                ),
            ));
        }
        let Expr::Lit(ExprLit {
            lit: Lit::Str(lit_str),
            ..
        }) = &field.expr
        else {
            return Err(syn::Error::new_spanned(
                &field.expr,
                format!(
                    "`{name}:` must be a string literal containing the full GTS instance id (e.g. \"gts.acme.core.events.topic.v1~vendor.app.x.v1\")"
                ),
            ));
        };
        found = Some((idx, ident.clone(), lit_str.clone()));
    }

    found.ok_or_else(|| {
        syn::Error::new_spanned(
            &instance.path,
            format!(
                "missing GTS instance-id field; the struct literal must contain exactly one of: {}",
                ID_FIELD_NAMES.join(", ")
            ),
        )
    })
}

/// Build the typed instance as a block expression: the struct literal
/// with the id-field's string rewritten to `GtsInstanceId::new(...)`,
/// preceded by a const-asserted prefix check. Validates the id literal's
/// shape (full GTS-spec format + `~` rules) and rejects struct-update
/// syntax (`..rest`) and missing/ambiguous/non-literal id fields.
fn build_typed_instance_block(args: &TypedInstanceArgs) -> syn::Result<TokenStream2> {
    let (id_idx, id_ident, instance_id_lit) = extract_id_field(&args.instance)?;
    validate_instance_id_format(&instance_id_lit)?;
    let (prefix_str, segment_str) = split_instance_id(&instance_id_lit)?;
    let prefix_lit = LitStr::new(&prefix_str, instance_id_lit.span());
    let segment_lit = LitStr::new(&segment_str, instance_id_lit.span());

    // Replace the id field's string literal with a GtsInstanceId::new call.
    let mut struct_expr = args.instance.clone();
    let new_field: FieldValue = syn::parse_quote! {
        #id_ident: ::gts::GtsInstanceId::new(#prefix_lit, #segment_lit)
    };
    let mut new_fields: Punctuated<FieldValue, Token![,]> = Punctuated::new();
    for (idx, field) in struct_expr.fields.iter().enumerate() {
        if idx == id_idx {
            new_fields.push(new_field.clone());
        } else {
            new_fields.push(field.clone());
        }
    }
    struct_expr.fields = new_fields;

    // The const-assert target type is derived from the struct literal's
    // path: descend through angle args to the deepest non-generic type
    // (the conforming schema in chained generics). For non-generic
    // carriers the path is used as-is. Turbofish is stripped so the path
    // is valid in `<X as Trait>` type position.
    let schema_path = path_for_type_position(&derive_schema_target_from_path(&args.instance.path));

    Ok(quote! {
        {
            const _: () = {
                const fn __gts_validate_id_prefix(id: &str, schema: &str) -> bool {
                    let id_b = id.as_bytes();
                    let s_b = schema.as_bytes();
                    // schema_id must end with `~`.
                    if s_b.is_empty() || s_b[s_b.len() - 1] != b'~' {
                        return false;
                    }
                    // instance_id must extend the schema by at least one byte.
                    if id_b.len() <= s_b.len() {
                        return false;
                    }
                    // Prefix must match exactly.
                    let mut i = 0;
                    while i < s_b.len() {
                        if id_b[i] != s_b[i] {
                            return false;
                        }
                        i += 1;
                    }
                    // The remainder (segment) must not contain `~`.
                    let mut j = s_b.len();
                    while j < id_b.len() {
                        if id_b[j] == b'~' {
                            return false;
                        }
                        j += 1;
                    }
                    true
                }
                assert!(
                    __gts_validate_id_prefix(
                        #instance_id_lit,
                        <#schema_path as ::gts::GtsSchema>::TYPE_ID,
                    ),
                    "instance id literal must equal the type's GtsSchema::TYPE_ID followed by a single non-empty segment (no extra `~`); for chained schemas, write the full type as a turbofish on the struct literal (e.g. `BaseV1::<LeafV1>` rather than bare `BaseV1`) so the macro can derive the conforming schema"
                );
            };
            // Compile-time guard: an abstract type is not directly instantiable.
            // `#schema_path` is the rightmost (most-derived) target type.
            const _: () = {
                if <#schema_path as ::gts::GtsSchema>::GTS_ABSTRACT {
                    panic!(
                        "gts_instance: cannot create an instance of an abstract type \
                         (it declares `gts_abstract = true` / `x-gts-abstract: true`); \
                         instantiate a concrete derived type instead"
                    );
                }
            };
            #struct_expr
        }
    })
}

/// Implementation of the typed [`gts_instance!`] macro. The proc-macro
/// entry point in `lib.rs` is a thin shim around this function.
pub fn expand_gts_instance(input: TokenStream2) -> syn::Result<TokenStream2> {
    let args: TypedInstanceArgs = parse2(input)?;
    let block_expr = build_typed_instance_block(&args)?;

    if let Some(name) = &args.static_name {
        let type_path = path_for_type_position(&args.instance.path);
        Ok(quote! {
            #[allow(non_upper_case_globals)]
            pub static #name: ::std::sync::LazyLock<#type_path> =
                ::std::sync::LazyLock::new(|| #block_expr);
        })
    } else {
        Ok(block_expr)
    }
}

/// One key:value entry inside a `gts_instance_raw!` JSON object literal.
/// Keys must be string literals (JSON-shape); values are captured as raw
/// token streams so that nested objects, arrays, and arbitrary expressions
/// pass through to `serde_json::json!` unchanged. Top-level entries are
/// separated by `,` — `Group` token trees are atomic, so commas inside
/// nested `{}` / `[]` / `()` are not visible at the top level and don't
/// interfere with this loop.
struct JsonEntry {
    key: LitStr,
    value: TokenStream2,
}

impl Parse for JsonEntry {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: LitStr = input.parse().map_err(|e| {
            syn::Error::new(
                e.span(),
                "expected a JSON-style string-literal key (e.g. \"id\", \"name\")",
            )
        })?;
        let _: Token![:] = input.parse()?;
        let mut value = TokenStream2::new();
        while !input.is_empty() && !input.peek(Token![,]) {
            let tt: proc_macro2::TokenTree = input.parse()?;
            value.extend(std::iter::once(tt));
        }
        Ok(Self { key, value })
    }
}

/// Args for [`gts_instance_raw!`]. The macro takes a single brace-delimited
/// JSON object literal where one of the top-level keys is `"id"` carrying
/// a string-literal value. The id is extracted at proc-macro time for
/// validation; the entire body is also spliced into a runtime
/// `serde_json::json!({ ... })` call (the macro then unconditionally
/// overwrites the `"id"` slot at runtime so the validated literal stays
/// authoritative even if the body is re-edited).
struct RawInstanceArgs {
    instance_id: LitStr,
    body: TokenStream2,
}

impl Parse for RawInstanceArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // Outer braces are required: gts_instance_raw!({ "id": "...", ... })
        let content;
        syn::braced!(content in input);
        if !input.is_empty() {
            return Err(input.error(
                "unexpected tokens after JSON object literal; gts_instance_raw! takes a single `{ ... }` body",
            ));
        }
        // Capture the original body as a token stream for json! splicing.
        let body: TokenStream2 = content.fork().parse()?;
        // Re-parse the same content as a list of JSON entries so we can
        // walk top-level keys.
        let entries: Punctuated<JsonEntry, Token![,]> = Punctuated::parse_terminated(&content)?;

        let mut found_id: Option<LitStr> = None;
        for entry in &entries {
            if entry.key.value() != "id" {
                continue;
            }
            if let Some(prev) = &found_id {
                let mut err = syn::Error::new_spanned(
                    &entry.key,
                    "duplicate top-level `\"id\"` key in gts_instance_raw! body",
                );
                err.combine(syn::Error::new_spanned(prev, "first `\"id\"` key was here"));
                return Err(err);
            }
            let id_lit: LitStr = parse2(entry.value.clone()).map_err(|_| {
                syn::Error::new_spanned(
                    &entry.value,
                    "`\"id\"` must be a string literal containing the full GTS instance id (e.g. \"gts.acme.core.events.topic.v1~vendor.app.x.v1\")",
                )
            })?;
            found_id = Some(id_lit);
        }

        let instance_id = found_id.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing top-level `\"id\"` key in gts_instance_raw! body; the JSON object must contain `\"id\": \"<full GTS instance id>\"`",
            )
        })?;

        Ok(Self { instance_id, body })
    }
}

/// Implementation of the raw-JSON [`gts_instance_raw!`] macro. The
/// proc-macro entry point in `lib.rs` is a thin shim around this function.
pub fn expand_gts_instance_raw(input: TokenStream2) -> syn::Result<TokenStream2> {
    let args: RawInstanceArgs = parse2(input)?;
    // Full GTS-spec format check + `~` rules. Both produce span-anchored
    // syn errors pointing at the bad literal. The split's return value is
    // unused; we splice the original literal into the generated JSON.
    validate_instance_id_format(&args.instance_id)?;
    let _ = split_instance_id(&args.instance_id)?;
    let instance_id_lit = &args.instance_id;
    let body_tokens = &args.body;
    // Build the JSON object first, then unconditionally overwrite the
    // `"id"` key with the validated literal. The user's body already
    // contains an `"id"` entry (we rejected the macro otherwise), and
    // serde_json's `json!` lets the later key win on duplicates — but
    // we don't rely on that ordering. The runtime `insert` makes the
    // validated literal authoritative regardless of what's in the body.
    Ok(quote! {
        {
            let mut __gts_value = ::serde_json::json!({ #body_tokens });
            __gts_value
                .as_object_mut()
                .expect("gts_instance_raw! body must be a JSON object")
                .insert(
                    "id".to_owned(),
                    ::serde_json::Value::String((#instance_id_lit).to_owned()),
                );
            __gts_value
        }
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests for the syntactic descent in
    //! [`derive_schema_target_from_path`]. These exercise paths that
    //! `struct_to_gts_schema` itself can't produce (multi-param carriers,
    //! lifetimes, const generics) — useful both as documentation of the
    //! algorithm's contract and as a safety net for any future loosening
    //! of the macro's single-type-param restriction.

    use super::derive_schema_target_from_path;
    use quote::quote;
    use syn::{Path, parse2};

    fn parse_path(tokens: proc_macro2::TokenStream) -> Path {
        parse2(tokens).expect("valid path tokens")
    }

    fn render(p: &Path) -> String {
        quote!(#p).to_string()
    }

    #[test]
    fn descent_picks_last_type_arg_when_first_is_non_gts() {
        // `L1OuterV1<T, L3LeafV1>` — `T` (non-GTS) is first, the GTS
        // chain leaf is last. The descent rule is "last type arg", so
        // the target is `L3LeafV1` regardless of `T`'s nature. If `T`
        // happened to be the GTS one and `L3LeafV1` were last but not
        // schema-bearing, the const-assert would fail at typecheck (as
        // a deliberate signal — the caller wrote the wrong arg order).
        let path = parse_path(quote!(L1OuterV1<T, L3LeafV1>));
        let target = derive_schema_target_from_path(&path);
        assert_eq!(render(&target), "L3LeafV1");
    }

    #[test]
    fn descent_picks_last_through_nested_multi_param() {
        // `Outer<T, Mid<U, Leaf>>` — descent: Outer → last arg is
        // `Mid<U, Leaf>`, recurse → last arg is `Leaf`, no angle args →
        // stop. `T` and `U` are ignored at every level because they are
        // not the final type-position arg.
        let path = parse_path(quote!(Outer<T, Mid<U, Leaf>>));
        let target = derive_schema_target_from_path(&path);
        assert_eq!(render(&target), "Leaf");
    }

    #[test]
    fn descent_skips_lifetime_args() {
        // `Carrier<'a, Leaf>` — `find_map` over reversed args picks the
        // last `Type` arg, ignoring lifetime args entirely.
        let path = parse_path(quote!(Carrier<'a, Leaf>));
        let target = derive_schema_target_from_path(&path);
        assert_eq!(render(&target), "Leaf");
    }

    #[test]
    fn descent_keeps_carrier_when_last_is_unit_type() {
        // `BaseV1<()>` — the descent stops on `()` (a `Type::Tuple`,
        // not `Type::Path`) and the carrier with explicit args is
        // kept. This is the base-instance case: `()::TYPE_ID = ""`,
        // so falling back to the carrier hits the right target.
        let path = parse_path(quote!(BaseV1<()>));
        let target = derive_schema_target_from_path(&path);
        // Comparing rendered tokens because syn::Path doesn't impl
        // PartialEq.
        assert_eq!(render(&target), render(&path));
    }

    #[test]
    fn descent_handles_const_generic_only() {
        // `Buf<N>` where `N` is a const generic — there's no Type arg,
        // so the descent finds nothing to recurse into and keeps the
        // carrier unchanged.
        let path = parse_path(quote!(Buf<{ 4 }>));
        let target = derive_schema_target_from_path(&path);
        assert_eq!(render(&target), render(&path));
    }
}
