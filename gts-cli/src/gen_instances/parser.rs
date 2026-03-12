use anyhow::{Result, bail};
use regex::Regex;
use std::path::Path;

use super::attrs::{InstanceAttrs, parse_instance_attrs};
use super::struct_expr::struct_expr_to_json;

/// A parsed and validated instance annotation, ready for file generation.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ParsedInstance {
    pub attrs: InstanceAttrs,
    /// JSON body extracted from the function's struct expression (without "id" field).
    pub json_body: String,
    /// Absolute path of the source file containing this annotation.
    pub source_file: String,
    /// 1-based line number of the annotation start, for diagnostics.
    pub line: usize,
}

/// Extract all `#[gts_well_known_instance]`-annotated functions from a source text.
///
/// Three outcomes per the extraction contract:
/// 1. No annotation token found (preflight negative) → `Ok(vec![])` (fast path, no errors)
/// 2. Annotation token found, parse fails → `Err(...)` (hard error, reported upstream)
/// 3. Parse succeeds → `Ok(instances)`
///
/// # Errors
/// Returns an error if an annotation is found but cannot be parsed or validated.
///
/// # Panics
/// Panics if the annotation regex produces a match with no capture group (should never happen).
pub fn extract_instances_from_source(
    content: &str,
    source_file: &Path,
) -> Result<Vec<ParsedInstance>> {
    if !preflight_scan(content) {
        return Ok(Vec::new());
    }

    let source_file_str = source_file.to_string_lossy().to_string();
    let line_offsets = build_line_offsets(content);
    // Strip comments before parsing so annotations in doc/line/block comments
    // are never matched as real annotations. Byte offsets are preserved because
    // strip_comments replaces comment text with spaces (newlines kept).
    let stripped = strip_comments(content);
    let annotation_re = build_annotation_regex()?;

    let mut instances = Vec::new();

    for cap in annotation_re.captures_iter(&stripped) {
        let Some(full_match) = cap.get(0) else {
            continue;
        };
        let full_start = full_match.start();
        let match_end = full_match.end();
        let line = byte_offset_to_line(full_start, &line_offsets);

        let attr_body = &cap[1];
        let attrs = parse_instance_attrs(attr_body, &source_file_str, line)?;

        // The regex matches up to (but not including) the opening `{`.
        // Use brace-depth counting to extract the function body.
        let (_body_end, fn_body) = extract_fn_body_at(&stripped, match_end).ok_or_else(|| {
            anyhow::anyhow!(
                "{source_file_str}:{line}: Could not find function body for \
                 #[gts_well_known_instance] annotation. Expected `{{ ... }}` after function signature."
            )
        })?;

        let json_body = extract_json_from_fn_body(fn_body, &source_file_str, line)?;

        instances.push(ParsedInstance {
            attrs,
            json_body,
            source_file: source_file_str.clone(),
            line,
        });
    }

    // Run unsupported-form checks on the same comment-stripped content.
    check_unsupported_forms(&stripped, &source_file_str, &line_offsets)?;

    // Preflight was positive but neither the main regex nor unsupported-form
    // checks matched anything — the annotation is in a form we don't recognise
    // (e.g. applied to a const, enum, or a completely garbled attribute body).
    // This is a hard error per the extraction contract.
    if instances.is_empty() {
        let needle_line = find_needle_line(content, &line_offsets);
        bail!(
            "{source_file_str}:{needle_line}: `#[gts_well_known_instance]` annotation found \
             but could not be parsed. The annotation must be on a \
             `fn get_instance_name_v1() -> SchemaType {{ ... }}` item. \
             Check for typos, unsupported item kinds, or missing required attributes."
        );
    }

    Ok(instances)
}

/// Extract JSON from a function body by parsing the struct expression.
///
/// The function body should contain a single struct expression (the last/only
/// expression in the block). This is parsed with `syn` and converted to JSON
/// via the `struct_expr` module.
fn extract_json_from_fn_body(fn_body: &str, source_file: &str, line: usize) -> Result<String> {
    // Wrap the body in braces if it isn't already (the regex captures the content inside {})
    let block_src = format!("{{ {fn_body} }}");
    let block: syn::Block = syn::parse_str(&block_src).map_err(|e| {
        anyhow::anyhow!("{source_file}:{line}: Failed to parse function body as Rust code: {e}")
    })?;

    // Find the struct expression — it should be the last expression in the block
    // (either the trailing expression or the last statement that is an expression)
    let struct_expr = extract_struct_expr_from_block(&block).ok_or_else(|| {
        anyhow::anyhow!(
            "{source_file}:{line}: Function body must contain a struct expression \
             (e.g., `MyStruct {{ field: value }}`). Could not find a struct expression."
        )
    })?;

    let json_value = struct_expr_to_json(struct_expr).map_err(|e| {
        anyhow::anyhow!("{source_file}:{line}: Failed to convert struct expression to JSON: {e}")
    })?;

    // The JSON should be an object
    if !json_value.is_object() {
        bail!("{source_file}:{line}: Struct expression did not produce a JSON object");
    }

    serde_json::to_string(&json_value).map_err(|e| {
        anyhow::anyhow!("{source_file}:{line}: Failed to serialize struct expression to JSON: {e}")
    })
}

/// Extract the struct expression from a parsed block.
///
/// Looks for a `syn::ExprStruct` as the trailing expression of the block.
fn extract_struct_expr_from_block(block: &syn::Block) -> Option<&syn::ExprStruct> {
    // Check the trailing expression first (block without semicolon on last line)
    if let Some(syn::Stmt::Expr(expr, None)) = block.stmts.last() {
        return find_struct_expr(expr);
    }
    None
}

/// Recursively find a struct expression, unwrapping parentheses and other wrappers.
fn find_struct_expr(expr: &syn::Expr) -> Option<&syn::ExprStruct> {
    match expr {
        syn::Expr::Struct(s) => Some(s),
        syn::Expr::Paren(p) => find_struct_expr(&p.expr),
        syn::Expr::Group(g) => find_struct_expr(&g.expr),
        _ => None,
    }
}

/// Build the regex matching `#[gts_well_known_instance(...)] fn name() -> Type { ... }`
///
/// Capture groups:
/// 1. Attribute body (everything inside the outer parentheses)
/// 2. The function body content (everything inside the outermost `{ }`, extracted
///    by `extract_fn_body_at` after the regex provides the match start position)
///
/// Note: Because function bodies can contain nested braces, the regex only matches
/// up to the opening `{`. The body is then extracted by brace-depth counting.
fn build_annotation_regex() -> Result<Regex> {
    let pattern = concat!(
        // (1) Macro attribute body
        r"#\[(?:gts_macros::)?gts_well_known_instance\(([\s\S]*?)\)\]",
        // Optional additional attributes (e.g. #[allow(dead_code)])
        r"(?:\s*#\[[^\]]*\])*",
        r"\s*",
        // Optional visibility: pub / pub(crate) / pub(super) / pub(in path)
        r"(?:pub\s*(?:\([^)]*\)\s*)?)?",
        // fn name() -> ReturnType (with optional generics in return type)
        r"fn\s+\w+\s*\(\s*\)\s*->\s*[^{]+",
    );
    Ok(Regex::new(pattern)?)
}

/// Extract the function body starting from the opening `{` at or after `start_pos`.
///
/// Uses brace-depth counting to correctly handle nested braces.
/// Returns the content between the outermost braces (exclusive).
fn extract_fn_body_at(content: &str, start_pos: usize) -> Option<(usize, &str)> {
    let bytes = content.as_bytes();
    let len = bytes.len();

    // Find the opening brace
    let mut i = start_pos;
    while i < len && bytes[i] != b'{' {
        i += 1;
    }
    if i >= len {
        return None;
    }

    let body_start = i + 1; // after the opening {
    let mut depth = 1;
    i += 1;

    while i < len && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                // Skip line comments
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                // Skip block comments
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < len {
                    i += 2;
                }
                continue;
            }
            b'"' => {
                // Skip string literals
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            b'r' if i + 1 < len && (bytes[i + 1] == b'"' || bytes[i + 1] == b'#') => {
                // Skip raw string literals
                if let Some(after) = try_skip_raw_string(bytes, i) {
                    i = after;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if depth == 0 {
        let body_end = i - 1; // before the closing }
        Some((i, &content[body_start..body_end]))
    } else {
        None
    }
}

/// Token-aware scan: finds `#[gts_well_known_instance` or
/// `#[gts_macros::gts_well_known_instance` outside comments and string literals.
/// Returns `true` if at least one candidate attribute token is found.
///
/// The `#[` prefix is required — bare identifiers (e.g. in `use` statements)
/// do not trigger a positive result, preventing false hard-errors downstream.
#[must_use]
pub fn preflight_scan(content: &str) -> bool {
    // Both needles require the `#[` attribute-open prefix so that a bare
    // identifier like `use gts_macros::gts_well_known_instance;` is never
    // a match. NEEDLE_BARE covers `#[gts_well_known_instance(`,
    // NEEDLE_QUAL covers `#[gts_macros::gts_well_known_instance(`.
    const NEEDLE_BARE: &[u8] = b"#[gts_well_known_instance";
    const NEEDLE_QUAL: &[u8] = b"#[gts_macros::gts_well_known_instance";
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip line comment `// ...`
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Skip block comment `/* ... */`
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2; // skip closing */
            continue;
        }
        // Skip regular string literal `"..."`
        if bytes[i] == b'"' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Skip raw string literal `r#"..."#` (any number of hashes)
        #[allow(clippy::collapsible_if)]
        if bytes[i] == b'r' {
            if let Some(after) = try_skip_raw_string(bytes, i) {
                i = after;
                continue;
            }
        }
        // Skip char literal `'x'` / `'\n'` / `'\u{..}'` to avoid false positives
        // on e.g. `'#'` or `'['` appearing near the needle by coincidence.
        //
        // IMPORTANT: We must NOT mistake Rust lifetimes (`'a`, `'static`) for
        // char literals — doing so would scan forward until the next `'` and
        // could skip a real `#[gts_well_known_instance` annotation (false negative).
        //
        // Strategy: tentatively walk past the char body, then check whether the
        // byte at that position is actually `'` (the closing delimiter).  If it
        // is, we have a confirmed char literal and skip past it.  If it is not,
        // we are looking at a lifetime annotation — just advance past the opening
        // `'` and resume normal scanning so no content is skipped.
        if bytes[i] == b'\'' {
            let mut j = i + 1;
            if j < len && bytes[j] == b'\\' {
                // Escaped char literal: '\n', '\\', '\u{NNNN}', etc.
                j += 1; // skip backslash
                while j < len && bytes[j] != b'\'' {
                    j += 1;
                }
                // j now points at closing ' (or end of input)
                if j < len && bytes[j] == b'\'' {
                    i = j + 1; // skip past closing '
                } else {
                    i += 1; // malformed — just skip opening '
                }
            } else if j < len && bytes[j] != b'\'' {
                // Could be a single char `'x'` or a lifetime `'name`.
                // Peek one further: if bytes[j+1] == '\'' it's a 1-char literal.
                if j + 1 < len && bytes[j + 1] == b'\'' {
                    i = j + 2; // skip 'x'
                } else {
                    // Not a char literal — lifetime or other use. Skip only the
                    // opening '\'' so the rest of the token is scanned normally.
                    i += 1;
                }
            } else {
                // `''` — empty char literal (invalid Rust, but don't get stuck)
                i += 1;
            }
            continue;
        }
        // Check for attribute-syntax needle (byte comparison — both needles are pure ASCII).
        // Qualified form is checked first because it is strictly longer.
        if bytes[i..].starts_with(NEEDLE_QUAL) || bytes[i..].starts_with(NEEDLE_BARE) {
            return true;
        }
        i += 1;
    }
    false
}

/// Attempt to skip a raw string starting at `start`. Returns `Some(new_i)` on success.
fn try_skip_raw_string(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes.len();
    let mut j = start + 1; // skip 'r'
    let mut hashes = 0usize;
    while j < len && bytes[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j >= len || bytes[j] != b'"' {
        return None; // not a raw string
    }
    j += 1; // skip opening "
    loop {
        if j >= len {
            return None; // unterminated
        }
        if bytes[j] == b'"' {
            let mut k = j + 1;
            let mut closing = 0usize;
            while k < len && bytes[k] == b'#' && closing < hashes {
                closing += 1;
                k += 1;
            }
            if closing == hashes {
                return Some(k);
            }
        }
        j += 1;
    }
}

/// Detect known unsupported annotation forms and emit actionable errors.
///
/// NOTE: uses `(?s)` (dotall) flag so the attr body may span multiple lines.
fn check_unsupported_forms(content: &str, source_file: &str, line_offsets: &[usize]) -> Result<()> {
    // Old const &str form (including static)
    let const_re = Regex::new(
        r"(?s)#\[(?:gts_macros::)?gts_well_known_instance\(.*?\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s*(?:\([^)]*\)\s*)?)?(?:const|static)\s",
    )?;
    if let Some(m) = const_re.find(content) {
        let line = byte_offset_to_line(m.start(), line_offsets);
        bail!(
            "{source_file}:{line}: `#[gts_well_known_instance]` no longer supports `const` or `static` items. \
             Use a function returning a typed struct instead:\n\
             \n  fn get_instance_name_v1() -> SchemaType {{\n      SchemaType {{ id: GtsInstanceId::ID, ... }}\n  }}"
        );
    }

    Ok(())
}

/// Build a byte-offset to line number index (line 1 = offset 0).
#[must_use]
pub fn build_line_offsets(content: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Convert a byte offset to a 1-based line number.
#[must_use]
pub fn byte_offset_to_line(offset: usize, line_offsets: &[usize]) -> usize {
    match line_offsets.binary_search(&offset) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

/// Strip line and block comments from source, replacing them with whitespace
/// to preserve byte offsets (and thus line numbers).
fn strip_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut out = content.to_owned().into_bytes();
    let mut i = 0;
    while i < len {
        // Line comment: replace up to (not including) the newline.
        // Only blank ASCII bytes — non-ASCII bytes are left intact so the
        // output remains valid UTF-8 (multi-byte sequences can't be part of
        // the pure-ASCII annotation needle).
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                if bytes[i].is_ascii() {
                    out[i] = b' ';
                }
                i += 1;
            }
            continue;
        }
        // Block comment: replace including delimiters
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] != b'\n' && bytes[i].is_ascii() {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i + 1 < len {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
            }
            continue;
        }
        // Skip over string literals unchanged (so we don't blank real code)
        if bytes[i] == b'"' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        #[allow(clippy::collapsible_if)]
        if bytes[i] == b'r' {
            if let Some(after) = try_skip_raw_string(bytes, i) {
                i = after;
                continue;
            }
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| content.to_owned())
}

/// Find the 1-based line of the first `#[...gts_well_known_instance` attribute in `content`.
/// Checks the qualified form first (longer), then the bare form.
fn find_needle_line(content: &str, line_offsets: &[usize]) -> usize {
    let pos = content
        .find("#[gts_macros::gts_well_known_instance")
        .or_else(|| content.find("#[gts_well_known_instance"));
    pos.map_or(1, |p| byte_offset_to_line(p, line_offsets))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a source string with a fn-based instance annotation.
    fn src(fn_body: &str) -> String {
        format!(
            concat!(
                "#[gts_well_known_instance(\n",
                "    dir_path = \"instances\",\n",
                "    id = \"gts.x.core.events.topic.v1~x.commerce._.orders.v1.0\"\n",
                ")]\n",
                "fn get_instance_orders_v1() -> MyStruct {{\n",
                "    {}\n",
                "}}\n"
            ),
            fn_body
        )
    }

    #[test]
    fn test_preflight_positive() {
        assert!(preflight_scan("#[gts_well_known_instance(x)]"));
    }

    #[test]
    fn test_preflight_negative_in_comment() {
        assert!(!preflight_scan("// #[gts_well_known_instance]"));
    }

    #[test]
    fn test_preflight_negative_in_block_comment() {
        assert!(!preflight_scan("/* #[gts_well_known_instance] */"));
    }

    #[test]
    fn test_preflight_positive_qualified_path() {
        assert!(preflight_scan("#[gts_macros::gts_well_known_instance(x)]"));
    }

    #[test]
    fn test_preflight_negative_bare_use_statement() {
        assert!(!preflight_scan(
            "use gts_macros::gts_well_known_instance;\nconst X: u32 = 1;\n"
        ));
    }

    #[test]
    fn test_preflight_positive_after_static_lifetime() {
        let src = concat!(
            "fn foo(x: &'static str) -> u32 { 0 }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_preflight_positive_after_named_lifetime() {
        let src = concat!(
            "fn bar<'a>(x: &'a str) -> &'a str { x }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_preflight_positive_char_literal_hash() {
        let src = concat!(
            "fn check(c: char) -> bool { c == '#' }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_extract_simple_struct() {
        let content = src("MyStruct { name: String::from(\"orders\") }");
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].attrs.id,
            "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"
        );
        assert_eq!(result[0].attrs.schema_id, "gts.x.core.events.topic.v1~");
        assert_eq!(result[0].attrs.instance_segment, "x.commerce._.orders.v1.0");
        // JSON body should contain "name"
        let json: serde_json::Value = serde_json::from_str(&result[0].json_body).unwrap();
        assert_eq!(json["name"], "orders");
    }

    #[test]
    fn test_no_annotation_returns_empty() {
        let content = "fn foo() -> u32 { 42 }";
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_rejects_const_form() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    id = \"gts.x.foo.v1~x.bar.v1.0\"\n",
            ")]\n",
            "const FOO: &str = \"{}\";\n"
        );
        let err = extract_instances_from_source(content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("no longer supports"));
    }

    #[test]
    fn test_rejects_static_item() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    id = \"gts.x.foo.v1~x.bar.v1.0\"\n",
            ")]\n",
            "static FOO: &str = \"{}\";\n"
        );
        let err = extract_instances_from_source(content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("no longer supports"));
    }

    #[test]
    fn test_multiple_annotations_in_one_file() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    id = \"gts.x.core.events.topic.v1~x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "fn get_instance_orders_v1() -> MyStruct {\n",
            "    MyStruct { name: String::from(\"orders\") }\n",
            "}\n",
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    id = \"gts.x.core.events.topic.v1~x.commerce._.payments.v1.0\"\n",
            ")]\n",
            "fn get_instance_payments_v1() -> MyStruct {\n",
            "    MyStruct { name: String::from(\"payments\") }\n",
            "}\n"
        );
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_pub_visibility_accepted() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    id = \"gts.x.core.events.topic.v1~x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "pub fn get_instance_orders_v1() -> MyStruct {\n",
            "    MyStruct { name: String::from(\"orders\") }\n",
            "}\n"
        );
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_gts_instance_id_sentinel_skipped() {
        let content = src("MyStruct { id: GtsInstanceId::ID, name: String::from(\"test\") }");
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        let json: serde_json::Value = serde_json::from_str(&result[0].json_body).unwrap();
        assert!(
            json.get("id").is_none(),
            "GtsInstanceId::ID should be skipped"
        );
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_unit_placeholder_skipped() {
        let content = src("MyStruct { name: String::from(\"test\"), properties: () }");
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        let json: serde_json::Value = serde_json::from_str(&result[0].json_body).unwrap();
        assert_eq!(
            json["properties"],
            serde_json::json!({}),
            "() should produce empty object"
        );
    }

    #[test]
    fn test_nested_struct() {
        let content = src("Outer { inner: Inner { value: 99 }, name: String::from(\"test\") }");
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        let json: serde_json::Value = serde_json::from_str(&result[0].json_body).unwrap();
        assert_eq!(json["inner"]["value"], 99);
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_vec_macro_in_struct() {
        let content = src("MyStruct { tags: vec![String::from(\"a\"), String::from(\"b\")] }");
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        let json: serde_json::Value = serde_json::from_str(&result[0].json_body).unwrap();
        let tags = json["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], "a");
        assert_eq!(tags[1], "b");
    }
}
