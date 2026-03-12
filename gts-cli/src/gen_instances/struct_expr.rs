use anyhow::{Result, bail};
use serde_json::Value;

/// Convert a `syn::ExprStruct` (parsed from a function body) to a `serde_json::Value` object.
///
/// Handles the field value types that are valid in instance definitions:
/// - String literals: `"hello"` → JSON string
/// - `String::from("hello")` / `"hello".to_string()` / `"hello".to_owned()` → JSON string
/// - Integer literals: `42` → JSON number
/// - Float literals: `3.14` → JSON number
/// - Boolean literals: `true` / `false` → JSON boolean
/// - Unit value `()` → empty JSON object `{}` (used for generic type parameter placeholders)
/// - `GtsInstanceId::ID` → skipped (sentinel value, replaced by CLI with real ID)
/// - Nested struct expressions → recursive JSON object
/// - `Vec` / array expressions → JSON array
///
/// # Errors
/// Returns an error if a field value cannot be converted to JSON.
pub fn struct_expr_to_json(expr: &syn::ExprStruct) -> Result<Value> {
    let mut map = serde_json::Map::new();

    for field in &expr.fields {
        let field_name = field.member.clone();
        let name = match &field_name {
            syn::Member::Named(ident) => ident.to_string(),
            syn::Member::Unnamed(idx) => idx.index.to_string(),
        };

        // Convert the field value
        match expr_to_json(&field.expr) {
            Ok(Some(value)) => {
                map.insert(name, value);
            }
            Ok(None) => {
                // Skipped value (e.g., GtsInstanceId::ID, ())
            }
            Err(e) => bail!("Field '{name}': {e}"),
        }
    }

    Ok(Value::Object(map))
}

/// Convert a `syn::Expr` to a `serde_json::Value`.
///
/// Returns `Ok(None)` for values that should be skipped (sentinel values, unit).
fn expr_to_json(expr: &syn::Expr) -> Result<Option<Value>> {
    match expr {
        // String literal: "hello"
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Ok(Some(Value::String(s.value()))),

        // Integer literal: 42, -1
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(i),
            ..
        }) => {
            // Try parsing as i64 first, then u64
            if let Ok(n) = i.base10_parse::<i64>() {
                Ok(Some(Value::Number(serde_json::Number::from(n))))
            } else if let Ok(n) = i.base10_parse::<u64>() {
                Ok(Some(Value::Number(serde_json::Number::from(n))))
            } else {
                bail!("Cannot parse integer literal: {i}")
            }
        }

        // Float literal: 3.14
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Float(f),
            ..
        }) => {
            let n: f64 = f.base10_parse()?;
            let num = serde_json::Number::from_f64(n)
                .ok_or_else(|| anyhow::anyhow!("Cannot represent float as JSON: {f}"))?;
            Ok(Some(Value::Number(num)))
        }

        // Boolean literal: true / false
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Bool(b),
            ..
        }) => Ok(Some(Value::Bool(b.value))),

        // Unary negation: -42
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr: inner,
            ..
        }) => match inner.as_ref() {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(i),
                ..
            }) => {
                let n: i64 = i.base10_parse::<i64>().map(|v| -v)?;
                Ok(Some(Value::Number(serde_json::Number::from(n))))
            }
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(f),
                ..
            }) => {
                let n: f64 = f.base10_parse::<f64>().map(|v| -v)?;
                let num = serde_json::Number::from_f64(n)
                    .ok_or_else(|| anyhow::anyhow!("Cannot represent float as JSON: -{f}"))?;
                Ok(Some(Value::Number(num)))
            }
            _ => bail!("Unsupported negation expression: {}", quote::quote!(#expr)),
        },

        // Unit expression: () — produce empty object (used for generic placeholders)
        syn::Expr::Tuple(tuple) if tuple.elems.is_empty() => {
            Ok(Some(Value::Object(serde_json::Map::new())))
        }

        // Path expressions: check for GtsInstanceId::ID sentinel
        syn::Expr::Path(path) => {
            if is_gts_instance_id_sentinel(path) {
                Ok(None) // Sentinel — skipped, CLI injects real ID
            } else if path.path.is_ident("true") {
                Ok(Some(Value::Bool(true)))
            } else if path.path.is_ident("false") {
                Ok(Some(Value::Bool(false)))
            } else {
                bail!(
                    "Unsupported path expression: {}. Only literal values, String::from(), \
                     and GtsInstanceId::ID are supported.",
                    quote::quote!(#expr)
                )
            }
        }

        // Function/method calls: String::from("..."), "...".to_string(), "...".to_owned(), vec![...]
        syn::Expr::Call(call) => handle_call_expr(call),
        syn::Expr::MethodCall(method) => handle_method_call_expr(method),

        // Nested struct expression: NestedStruct { field: value, ... }
        syn::Expr::Struct(s) => {
            let obj = struct_expr_to_json(s)?;
            Ok(Some(obj))
        }

        // Array/Vec literal: vec![...] or [...]
        syn::Expr::Array(arr) => {
            let items: Result<Vec<Value>> = arr
                .elems
                .iter()
                .filter_map(|e| match expr_to_json(e) {
                    Ok(Some(v)) => Some(Ok(v)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                })
                .collect();
            Ok(Some(Value::Array(items?)))
        }

        // Macro invocations: vec![...]
        syn::Expr::Macro(mac) => handle_macro_expr(mac),

        // Reference expressions: &"hello" (strip the reference)
        syn::Expr::Reference(syn::ExprReference { expr: inner, .. })
        // Group expressions (parenthesized): (expr)
        | syn::Expr::Group(syn::ExprGroup { expr: inner, .. })
        | syn::Expr::Paren(syn::ExprParen { expr: inner, .. }) => expr_to_json(inner),

        _ => bail!(
            "Unsupported expression type in instance body: {}. \
             Only literal values, String::from(), struct expressions, \
             vec![], arrays, and GtsInstanceId::ID are supported.",
            quote::quote!(#expr)
        ),
    }
}

/// Check if a path expression is `GtsInstanceId::ID` (or `gts::GtsInstanceId::ID`)
fn is_gts_instance_id_sentinel(path: &syn::ExprPath) -> bool {
    let segs: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    // Match: GtsInstanceId::ID or gts::GtsInstanceId::ID
    (segs.len() == 2 && segs[0] == "GtsInstanceId" && segs[1] == "ID")
        || (segs.len() == 3 && segs[0] == "gts" && segs[1] == "GtsInstanceId" && segs[2] == "ID")
}

/// Handle function call expressions like `String::from("hello")`
fn handle_call_expr(call: &syn::ExprCall) -> Result<Option<Value>> {
    // Check for String::from("...")
    if let syn::Expr::Path(func_path) = call.func.as_ref() {
        let segments: Vec<String> = func_path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();

        if segments.len() == 2
            && segments[0] == "String"
            && segments[1] == "from"
            && call.args.len() == 1
        {
            // SAFETY: len == 1 checked above, so first() is always Some
            return expr_to_json(&call.args[0]);
        }
    }

    bail!(
        "Unsupported function call: {}. Only String::from() is supported.",
        quote::quote!(#call)
    )
}

/// Handle method call expressions like `"hello".to_string()`, `"hello".to_owned()`
fn handle_method_call_expr(method: &syn::ExprMethodCall) -> Result<Option<Value>> {
    let method_name = method.method.to_string();
    if (method_name == "to_string" || method_name == "to_owned" || method_name == "into")
        && method.args.is_empty()
    {
        return expr_to_json(&method.receiver);
    }

    bail!(
        "Unsupported method call: {}. Only .to_string(), .to_owned(), and .into() are supported.",
        quote::quote!(#method)
    )
}

/// Parse a token stream as a comma-separated list of expressions.
fn parse_comma_separated_exprs(tokens: proc_macro2::TokenStream) -> Result<Vec<syn::Expr>> {
    #![allow(clippy::needless_pass_by_value)]
    // Wrap in brackets to make it parseable as an array expression
    let wrapped: proc_macro2::TokenStream = quote::quote! { [ #tokens ] };
    let arr: syn::ExprArray = syn::parse2(wrapped)
        .map_err(|e| anyhow::anyhow!("Failed to parse comma-separated expressions: {e}"))?;
    Ok(arr.elems.into_iter().collect())
}

/// Handle macro invocations like `vec![...]`
fn handle_macro_expr(mac: &syn::ExprMacro) -> Result<Option<Value>> {
    let path = &mac.mac.path;
    let path_str = quote::quote!(#path).to_string();

    if path_str == "vec" {
        let exprs = parse_comma_separated_exprs(mac.mac.tokens.clone())?;

        let values: Result<Vec<Value>> = exprs
            .iter()
            .filter_map(|e| match expr_to_json(e) {
                Ok(Some(v)) => Some(Ok(v)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect();
        return Ok(Some(Value::Array(values?)));
    }

    bail!("Unsupported macro invocation: {path_str}. Only vec![] is supported.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_simple_struct() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                name: String::from("orders"),
                count: 42,
                active: true
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert_eq!(json["name"], "orders");
        assert_eq!(json["count"], 42);
        assert_eq!(json["active"], true);
    }

    #[test]
    fn test_string_methods() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                a: String::from("hello"),
                b: "world".to_string(),
                c: "foo".to_owned()
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert_eq!(json["a"], "hello");
        assert_eq!(json["b"], "world");
        assert_eq!(json["c"], "foo");
    }

    #[test]
    fn test_gts_instance_id_skipped() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                id: GtsInstanceId::ID,
                name: String::from("test")
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert!(json.get("id").is_none());
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_unit_skipped() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                name: String::from("test"),
                properties: ()
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert_eq!(json["properties"], serde_json::json!({}));
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_negative_number() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                offset: -10
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert_eq!(json["offset"], -10);
    }

    #[test]
    fn test_float_value() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                rate: 3.15
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        let rate = json["rate"].as_f64().unwrap();
        assert!((rate - 3.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_vec_macro() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                tags: vec![String::from("a"), String::from("b")]
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        let tags = json["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], "a");
        assert_eq!(tags[1], "b");
    }

    #[test]
    fn test_array_literal() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                items: [1, 2, 3]
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        let items = json["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], 1);
    }

    #[test]
    fn test_nested_struct() {
        let expr: syn::ExprStruct = parse_quote! {
            Outer {
                name: String::from("parent"),
                inner: Inner {
                    value: 99
                }
            }
        };
        let json = struct_expr_to_json(&expr).unwrap();
        assert_eq!(json["name"], "parent");
        assert_eq!(json["inner"]["value"], 99);
    }

    #[test]
    fn test_unsupported_expr_errors() {
        let expr: syn::ExprStruct = parse_quote! {
            MyStruct {
                name: some_function()
            }
        };
        assert!(struct_expr_to_json(&expr).is_err());
    }
}
