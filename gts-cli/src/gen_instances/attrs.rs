use anyhow::{Result, bail};
use regex::Regex;
use std::collections::HashSet;

/// Parsed and validated attributes from `#[gts_well_known_instance(...)]`.
#[derive(Debug, Clone)]
pub struct InstanceAttrs {
    pub dir_path: String,
    pub schema_id: String,
    pub instance_segment: String,
}

/// Parse and validate instance annotation attribute body.
///
/// # Errors
/// - Any required attribute (`dir_path`, `schema_id`, `instance_segment`) is missing
/// - `schema_id` does not end with `~`
/// - `instance_segment` ends with `~` or is a bare wildcard `*`
/// - The composed `schema_id + instance_segment` fails GTS ID validation
pub fn parse_instance_attrs(
    attr_body: &str,
    source_file: &str,
    line: usize,
) -> Result<InstanceAttrs> {
    check_duplicate_attr_keys(attr_body, source_file, line)?;

    let dir_path = extract_str_attr(attr_body, "dir_path").ok_or_else(|| {
        anyhow::anyhow!("{source_file}:{line}: Missing required attribute 'dir_path' in #[gts_well_known_instance]")
    })?;

    let schema_id = extract_str_attr(attr_body, "schema_id").ok_or_else(|| {
        anyhow::anyhow!("{source_file}:{line}: Missing required attribute 'schema_id' in #[gts_well_known_instance]")
    })?;

    let instance_segment = extract_str_attr(attr_body, "instance_segment").ok_or_else(|| {
        anyhow::anyhow!("{source_file}:{line}: Missing required attribute 'instance_segment' in #[gts_well_known_instance]")
    })?;

    if !schema_id.ends_with('~') {
        bail!(
            "{source_file}:{line}: schema_id '{schema_id}' must end with '~' (type marker). \
             Instance IDs are composed as schema_id + instance_segment."
        );
    }

    if instance_segment.ends_with('~') {
        bail!(
            "{source_file}:{line}: instance_segment '{instance_segment}' must not end with '~' -- \
             that is a schema/type marker, not valid in an instance segment."
        );
    }

    if instance_segment == "*" {
        bail!(
            "{source_file}:{line}: instance_segment must not be a bare wildcard '*'. \
             Wildcards are not valid in generated instance IDs."
        );
    }

    let composed = format!("{schema_id}{instance_segment}");
    if let Err(e) = gts_id::validate_gts_id(&composed, false) {
        let msg = match &e {
            gts_id::GtsIdError::Id { cause, .. } => cause.clone(),
            gts_id::GtsIdError::Segment { num, cause, .. } => {
                format!("segment #{num}: {cause}")
            }
        };
        bail!("{source_file}:{line}: Invalid composed instance ID '{composed}': {msg}");
    }

    Ok(InstanceAttrs {
        dir_path,
        schema_id,
        instance_segment,
    })
}

/// Error if any of the known attribute keys appears more than once in the body.
///
/// String literal content is stripped before scanning so that `key =` text
/// inside a string value (e.g. `dir_path = "schema_id = x"`) does not
/// trigger a false duplicate.
fn check_duplicate_attr_keys(attr_body: &str, source_file: &str, line: usize) -> Result<()> {
    let key_re = Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*=").ok();
    let Some(re) = key_re else {
        return Ok(());
    };
    let known: HashSet<&str> = ["dir_path", "schema_id", "instance_segment"]
        .iter()
        .copied()
        .collect();
    // Blank out string literal content so `key =` inside a value can't match.
    let stripped = blank_string_literals(attr_body);
    let mut seen: HashSet<String> = HashSet::new();
    for cap in re.captures_iter(&stripped) {
        let key = cap.get(1).map_or("", |m| m.as_str());
        if !known.contains(key) {
            continue;
        }
        if !seen.insert(key.to_owned()) {
            bail!(
                "{source_file}:{line}: Duplicate attribute '{key}' in \
                 #[gts_well_known_instance]. Each attribute must appear exactly once."
            );
        }
    }
    Ok(())
}

/// Replace the content of every string literal in `s` with spaces,
/// preserving byte positions so that other offsets remain valid.
/// Handles both regular `"..."` and raw `r#"..."#` (any number of hashes).
fn blank_string_literals(s: &str) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = s.to_owned().into_bytes();
    let mut pos = 0;
    while pos < len {
        // Raw string literal: r"..." or r#"..."# or r##"..."##, etc.
        if bytes[pos] == b'r' {
            let mut hash_end = pos + 1;
            while hash_end < len && bytes[hash_end] == b'#' {
                hash_end += 1;
            }
            let hashes = hash_end - (pos + 1);
            if hash_end < len && bytes[hash_end] == b'"' {
                // Found r<hashes>", now scan for closing "<hashes>#
                let content_start = hash_end + 1;
                let mut scan = content_start;
                let mut close_span: Option<(usize, usize)> = None;
                'raw: while scan < len {
                    if bytes[scan] == b'"' {
                        // Check for the required number of closing hashes
                        let mut close = scan + 1;
                        let mut count = 0;
                        while close < len && bytes[close] == b'#' && count < hashes {
                            count += 1;
                            close += 1;
                        }
                        if count == hashes {
                            close_span = Some((scan, close));
                            break 'raw;
                        }
                    }
                    scan += 1;
                }
                if let Some((content_end, close)) = close_span {
                    for byte in &mut out[content_start..content_end] {
                        if byte.is_ascii() {
                            *byte = b' ';
                        }
                    }
                    pos = close;
                } else {
                    // Unterminated raw string: advance to avoid infinite loop.
                    pos += 1;
                }
                continue;
            }
            // Not a raw string — fall through to normal char handling
        }
        // Regular string literal: "..."
        if bytes[pos] == b'"' {
            pos += 1;
            while pos < len {
                if bytes[pos] == b'\\' {
                    // Replace both the backslash and the escaped char with spaces.
                    if bytes[pos].is_ascii() {
                        out[pos] = b' ';
                    }
                    pos += 1;
                    if pos < len && bytes[pos].is_ascii() {
                        out[pos] = b' ';
                    }
                    pos += 1;
                    continue;
                }
                if bytes[pos] == b'"' {
                    pos += 1;
                    break;
                }
                if bytes[pos].is_ascii() {
                    out[pos] = b' ';
                }
                pos += 1;
            }
        } else {
            pos += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// Extract a `key = "value"` string attribute from an attribute body.
fn extract_str_attr(attr_body: &str, key: &str) -> Option<String> {
    let pattern = format!(r#"{key}\s*=\s*"([^"\\]*(?:\\.[^"\\]*)*)""#);
    let re = Regex::new(&pattern).ok()?;
    re.captures(attr_body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_attrs() {
        let body = r#"dir_path = "instances", schema_id = "gts.x.core.events.topic.v1~", instance_segment = "x.commerce._.orders.v1.0""#;
        let attrs = parse_instance_attrs(body, "test.rs", 1).unwrap();
        assert_eq!(attrs.dir_path, "instances");
        assert_eq!(attrs.schema_id, "gts.x.core.events.topic.v1~");
        assert_eq!(attrs.instance_segment, "x.commerce._.orders.v1.0");
    }

    #[test]
    fn test_missing_dir_path() {
        let body = r#"schema_id = "gts.x.foo.v1~", instance_segment = "x.bar.v1.0""#;
        let err = parse_instance_attrs(body, "test.rs", 5).unwrap_err();
        assert!(err.to_string().contains("dir_path"));
    }

    #[test]
    fn test_missing_schema_id() {
        let body = r#"dir_path = "instances", instance_segment = "x.bar.v1.0""#;
        let err = parse_instance_attrs(body, "test.rs", 5).unwrap_err();
        assert!(err.to_string().contains("schema_id"));
    }

    #[test]
    fn test_missing_instance_segment() {
        let body = r#"dir_path = "instances", schema_id = "gts.x.foo.v1~""#;
        let err = parse_instance_attrs(body, "test.rs", 5).unwrap_err();
        assert!(err.to_string().contains("instance_segment"));
    }

    #[test]
    fn test_schema_id_missing_tilde() {
        let body = r#"dir_path = "instances", schema_id = "gts.x.foo.v1", instance_segment = "x.bar.v1.0""#;
        let err = parse_instance_attrs(body, "test.rs", 1).unwrap_err();
        assert!(err.to_string().contains("must end with '~'"));
    }

    #[test]
    fn test_instance_segment_with_tilde() {
        let body = r#"dir_path = "instances", schema_id = "gts.x.foo.v1~", instance_segment = "x.bar.v1~""#;
        let err = parse_instance_attrs(body, "test.rs", 1).unwrap_err();
        assert!(err.to_string().contains("must not end with '~'"));
    }

    #[test]
    fn test_instance_segment_bare_wildcard() {
        let body = r#"dir_path = "instances", schema_id = "gts.x.foo.v1~", instance_segment = "*""#;
        let err = parse_instance_attrs(body, "test.rs", 1).unwrap_err();
        assert!(err.to_string().contains("wildcard"));
    }

    #[test]
    fn test_error_contains_file_and_line() {
        let body = r#"schema_id = "gts.x.foo.v1~", instance_segment = "x.bar.v1.0""#;
        let err = parse_instance_attrs(body, "src/events.rs", 42).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("src/events.rs"));
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_key_in_string_value_not_false_duplicate() {
        // dir_path value contains "schema_id = x" — must not trigger a false duplicate.
        let body = r#"dir_path = "schema_id = x", schema_id = "gts.x.core.events.topic.v1~", instance_segment = "x.commerce._.orders.v1.0""#;
        let attrs = parse_instance_attrs(body, "test.rs", 1).unwrap();
        assert_eq!(attrs.dir_path, "schema_id = x");
    }

    #[test]
    fn test_blank_string_literals_blanks_raw_strings() {
        // Raw string content containing key= must be blanked so duplicate detection
        // can't see it. Attribute values always use regular "..." in practice, but
        // blank_string_literals is defensive.
        // Input: r#"schema_id = x"# rest
        let s = "r#\"schema_id = x\"# rest";
        let blanked = blank_string_literals(s);
        // The content between r#" and "# must be spaces; the surrounding tokens intact.
        assert!(
            !blanked.contains("schema_id"),
            "raw string content should be blanked, got: {blanked:?}"
        );
    }

    #[test]
    fn test_real_duplicate_key_is_rejected() {
        let body = r#"dir_path = "instances", dir_path = "other", schema_id = "gts.x.core.events.topic.v1~", instance_segment = "x.commerce._.orders.v1.0""#;
        let err = parse_instance_attrs(body, "test.rs", 1).unwrap_err();
        assert!(err.to_string().contains("Duplicate attribute"));
    }
}
