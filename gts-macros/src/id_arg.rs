//! Shared parsing for GTS-id macro arguments.
//!
//! Several macros accept a GTS identifier as input: `struct_to_gts_schema`
//! (`type_id = ...`), `gts_instance!` (`id: ...`), and `gts_instance_raw!`
//! (`"id": ...`). Each historically required the *full* identifier string
//! literal, including the configured prefix (`gts.` by default).
//!
//! To avoid hard-coding the prefix at every call site, those macros also accept
//! the [`PREFIX_MACRO`] marker form `gts_id!("<suffix>")`, where `<suffix>` is
//! the identifier *without* the prefix. The macros parse their own token
//! streams, so they recognize this shape directly and prepend
//! [`gts_id::GTS_ID_PREFIX`] themselves — they do not rely on the compiler to
//! expand `gts_id!` (which would not happen inside another macro's input).
//!
//! The same name is also exported as a real expression macro (see the
//! `gts_id` `#[proc_macro]` in `lib.rs`) so it works in ordinary expression
//! position too (e.g. building expected ids in tests).

use gts_id::GTS_ID_PREFIX;
use syn::parse::ParseStream;
use syn::{Expr, ExprLit, ExprMacro, Lit, LitStr, Path};

/// Name of the marker / helper macro that prepends the configured prefix.
pub const PREFIX_MACRO: &str = "gts_id";

/// Returns `true` if `path` is `gts_id` or ends with `::gts_id` (i.e. a
/// qualified path whose last segment is the marker macro name).
fn is_prefix_macro_path(path: &Path) -> bool {
    path.segments
        .last()
        .is_some_and(|seg| seg.ident == PREFIX_MACRO)
}

/// Build the full id literal from a suffix written inside `gts_id!("...")`,
/// using a macro-definition span so lint passes can distinguish it from a
/// user-written hardcoded prefixed literal.
pub fn build_prefixed_lit(suffix: &LitStr) -> LitStr {
    LitStr::new(
        &format!("{GTS_ID_PREFIX}{}", suffix.value()),
        proc_macro2::Span::mixed_site(),
    )
}

/// Parse a GTS-id macro argument from a parse stream, accepting either a full
/// string literal or the `gts_id!("<suffix>")` marker form. Returns a `LitStr`
/// holding the full identifier.
pub fn parse_gts_id_arg(input: ParseStream) -> syn::Result<LitStr> {
    let expr: Expr = input.parse()?;
    gts_id_lit_from_expr(&expr)
}

/// Extract the full id `LitStr` from an already-parsed expression, accepting
/// the literal and `gts_id!("...")` forms. Any other expression is an error.
pub fn gts_id_lit_from_expr(expr: &Expr) -> syn::Result<LitStr> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Ok(s.clone()),
        Expr::Macro(ExprMacro { mac, .. }) if is_prefix_macro_path(&mac.path) => {
            let suffix: LitStr = mac.parse_body().map_err(|_| {
                syn::Error::new_spanned(
                    mac,
                    format!(
                        "`{PREFIX_MACRO}!` takes a single string-literal suffix, \
                         e.g. `{PREFIX_MACRO}!(\"x.core.events.topic.v1~\")`"
                    ),
                )
            })?;
            Ok(build_prefixed_lit(&suffix))
        }
        other => Err(syn::Error::new_spanned(
            other,
            format!("expected a string literal or `{PREFIX_MACRO}!(\"...\")`"),
        )),
    }
}
