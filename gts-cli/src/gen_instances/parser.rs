use anyhow::{Result, bail};
use regex::Regex;
use std::path::Path;

use super::attrs::{InstanceAttrs, parse_instance_attrs};
use super::string_lit::decode_string_literal;

/// A parsed and validated instance annotation, ready for file generation.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ParsedInstance {
    pub attrs: InstanceAttrs,
    /// Raw JSON body string (as written in the const value, decoded from the literal).
    pub json_body: String,
    /// Absolute path of the source file containing this annotation.
    pub source_file: String,
    /// 1-based line number of the annotation start, for diagnostics.
    pub line: usize,
}

/// Extract all `#[gts_well_known_instance]`-annotated consts from a source text.
///
/// Three outcomes per the extraction contract:
/// 1. No annotation token found (preflight negative) → `Ok(vec![])` (fast path, no errors)
/// 2. Annotation token found, parse fails → `Err(...)` (hard error, reported upstream)
/// 3. Parse succeeds → `Ok(instances)`
///
/// # Errors
/// Returns an error if an annotation is found but cannot be parsed or validated.
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
        let full_start = cap.get(0).map_or(0, |m| m.start());
        let line = byte_offset_to_line(full_start, &line_offsets);

        let attr_body = &cap[1];
        let attrs = parse_instance_attrs(attr_body, &source_file_str, line)?;

        let raw_literal = &cap[2];
        let json_body = decode_string_literal(raw_literal).map_err(|e| {
            anyhow::anyhow!("{source_file_str}:{line}: Failed to decode string literal: {e}")
        })?;

        validate_json_body(&json_body, &source_file_str, line)?;

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
    // (e.g. applied to a fn, enum, or a completely garbled attribute body).
    // This is a hard error per the extraction contract.
    if instances.is_empty() {
        let needle_line = find_needle_line(content, &line_offsets);
        bail!(
            "{source_file_str}:{needle_line}: `#[gts_well_known_instance]` annotation found \
             but could not be parsed. The annotation must be on a `const NAME: &str = <literal>;` \
             item. Check for typos, unsupported item kinds, or missing required attributes."
        );
    }

    Ok(instances)
}

/// Validate that the decoded JSON body is a non-empty object without an `"id"` field.
fn validate_json_body(json_body: &str, source_file: &str, line: usize) -> Result<()> {
    let json_val: serde_json::Value = serde_json::from_str(json_body).map_err(|e| {
        anyhow::anyhow!(
            "{}:{}: Malformed JSON in instance body: {} (at JSON line {}, col {})",
            source_file,
            line,
            e,
            e.line(),
            e.column()
        )
    })?;

    if !json_val.is_object() {
        bail!(
            "{}:{}: Instance JSON body must be a JSON object {{...}}, got {}. \
             Arrays, strings, numbers, booleans, and null are not valid instance bodies.",
            source_file,
            line,
            json_type_name(&json_val)
        );
    }

    if json_val.get("id").is_some() {
        bail!(
            "{source_file}:{line}: Instance JSON body must not contain an \"id\" field. \
             The id is automatically injected from schema_id + instance_segment. \
             Remove the \"id\" field from the JSON body."
        );
    }

    Ok(())
}

/// Build the regex matching `#[gts_well_known_instance(...)] const NAME: &str = <literal>;`
///
/// Capture groups:
/// 1. Attribute body (everything inside the outer parentheses)
/// 2. The string literal token (raw or regular)
fn build_annotation_regex() -> Result<Regex> {
    let pattern = concat!(
        // (1) Macro attribute body
        r"#\[(?:gts_macros::)?gts_well_known_instance\(([\s\S]*?)\)\]",
        // Optional additional attributes (e.g. #[allow(dead_code)])
        r"(?:\s*#\[[^\]]*\])*",
        r"\s*",
        // Optional visibility: pub / pub(crate) / pub(super) / pub(in path)
        r"(?:pub\s*(?:\([^)]*\)\s*)?)?",
        // const NAME: &str = (optional 'static lifetime)
        r"const\s+\w+\s*:\s*&\s*(?:'static\s+)?str\s*=\s*",
        // (2) String literal: raw r"..." / r#"..."# / r##"..."## (0+ hashes) or regular "..."
        "(r#*\"[\\s\\S]*?\"#*|\"(?:[^\"\\\\]|\\\\.)*\")",
        r"\s*;"
    );
    Ok(Regex::new(pattern)?)
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
    // static instead of const
    let static_re = Regex::new(
        r"(?s)#\[(?:gts_macros::)?gts_well_known_instance\(.*?\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s*(?:\([^)]*\)\s*)?)?static\s",
    )?;
    if let Some(m) = static_re.find(content) {
        let line = byte_offset_to_line(m.start(), line_offsets);
        bail!(
            "{source_file}:{line}: `#[gts_well_known_instance]` cannot be applied to `static` items. \
             Use `const NAME: &str = ...` instead."
        );
    }

    // concat!() as value
    let concat_re = Regex::new(
        r"(?s)#\[(?:gts_macros::)?gts_well_known_instance\(.*?\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s*(?:\([^)]*\)\s*)?)?const\s+\w+\s*:\s*&\s*(?:'static\s+)?str\s*=\s*concat\s*!",
    )?;
    if let Some(m) = concat_re.find(content) {
        let line = byte_offset_to_line(m.start(), line_offsets);
        bail!(
            "{source_file}:{line}: `concat!()` is not supported as the const value for \
             `#[gts_well_known_instance]`. Use a raw string literal `r#\"...\"#` instead."
        );
    }

    // const with wrong type (not &str) — checked last as it's broader
    // Note: we use a positive match for the non-&str case to avoid lookahead
    let wrong_type_re = Regex::new(
        r"(?s)#\[(?:gts_macros::)?gts_well_known_instance\(.*?\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s*(?:\([^)]*\)\s*)?)?const\s+\w+\s*:\s*&\s*(?:'static\s+)?([A-Za-z][A-Za-z0-9_]*)\b",
    )?;
    if let Some(cap) = wrong_type_re.captures(content) {
        let ty = cap.get(1).map_or("", |m| m.as_str());
        if ty != "str" {
            let start = cap.get(0).map_or(0, |m| m.start());
            let line = byte_offset_to_line(start, line_offsets);
            bail!(
                "{source_file}:{line}: `#[gts_well_known_instance]` requires `const NAME: &str`. \
             The annotated const must have type `&str`."
            );
        }
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

fn json_type_name(val: &serde_json::Value) -> &'static str {
    match val {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(body: &str) -> String {
        format!(
            concat!(
                "#[gts_well_known_instance(\n",
                "    dir_path = \"instances\",\n",
                "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
                "    instance_segment = \"{}\"\n",
                ")]\n",
                "const FOO: &str = {};\n"
            ),
            "x.commerce._.orders.v1.0", body
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
        // `use gts_macros::gts_well_known_instance;` must NOT be a positive —
        // it lacks the required `#[` attribute-open prefix.
        assert!(!preflight_scan(
            "use gts_macros::gts_well_known_instance;\nconst X: u32 = 1;\n"
        ));
    }

    #[test]
    fn test_preflight_positive_after_static_lifetime() {
        // `'static` before the annotation must NOT suppress it (false-negative fix).
        let src = concat!(
            "fn foo(x: &'static str) -> u32 { 0 }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_preflight_positive_after_named_lifetime() {
        // `'a` lifetime before the annotation must NOT suppress it.
        let src = concat!(
            "fn bar<'a>(x: &'a str) -> &'a str { x }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_preflight_positive_char_literal_hash() {
        // A char literal containing '#' must not be the needle itself.
        // But the real annotation after it must still be found.
        let src = concat!(
            "fn check(c: char) -> bool { c == '#' }\n",
            "#[gts_well_known_instance(x)]\n"
        );
        assert!(preflight_scan(src));
    }

    #[test]
    fn test_extract_regular_string() {
        let content = src(r#""{\"name\": \"orders\"}""#);
        let result = extract_instances_from_source(&content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attrs.schema_id, "gts.x.core.events.topic.v1~");
        assert_eq!(result[0].attrs.instance_segment, "x.commerce._.orders.v1.0");
    }

    #[test]
    fn test_no_annotation_returns_empty() {
        let content = "const FOO: &str = \"hello\";";
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_rejects_id_in_body() {
        let content = src(r#""{\"id\": \"bad\", \"name\": \"x\"}""#);
        let err = extract_instances_from_source(&content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("\"id\" field"));
    }

    #[test]
    fn test_rejects_non_object_json() {
        let content = src("\"[1, 2, 3]\"");
        let err = extract_instances_from_source(&content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("JSON object"));
    }

    #[test]
    fn test_rejects_malformed_json() {
        let content = src(r#""{not valid json}""#);
        let err = extract_instances_from_source(&content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("Malformed JSON"));
    }

    #[test]
    fn test_rejects_static_item() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.foo.v1~\",\n",
            "    instance_segment = \"x.bar.v1.0\"\n",
            ")]\n",
            "static FOO: &str = \"{}\";\n"
        );
        let err = extract_instances_from_source(content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("static"));
    }

    #[test]
    fn test_rejects_concat_macro() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.foo.v1~\",\n",
            "    instance_segment = \"x.bar.v1.0\"\n",
            ")]\n",
            "const FOO: &str = concat!(\"{\", \"}\");\n"
        );
        let err = extract_instances_from_source(content, Path::new("t.rs")).unwrap_err();
        assert!(err.to_string().contains("concat!()"));
    }

    #[test]
    fn test_multiple_annotations_in_one_file() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
            "    instance_segment = \"x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "const A: &str = \"{\\\"name\\\": \\\"orders\\\"}\";\n",
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
            "    instance_segment = \"x.commerce._.payments.v1.0\"\n",
            ")]\n",
            "const B: &str = \"{\\\"name\\\": \\\"payments\\\"}\";\n"
        );
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_pub_visibility_accepted() {
        let content = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
            "    instance_segment = \"x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "pub const FOO: &str = \"{\\\"name\\\": \\\"orders\\\"}\";\n"
        );
        let result = extract_instances_from_source(content, Path::new("t.rs")).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_line_number_reported() {
        let content = concat!(
            "// line 1\n",
            "// line 2\n",
            "#[gts_well_known_instance(\n", // line 3
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.foo.v1~\",\n",
            "    instance_segment = \"x.bar.v1.0\"\n",
            ")]\n",
            "const FOO: &str = \"{\\\"id\\\": \\\"bad\\\"}\";\n"
        );
        let err = extract_instances_from_source(content, Path::new("events.rs")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("events.rs"));
        // line 3 is where the annotation starts
        assert!(msg.contains(":3:"), "Expected line 3 in: {msg}");
    }
}
