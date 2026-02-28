use anyhow::{Result, bail};

/// Decode a Rust string literal token to its actual string content.
///
/// Supports:
/// - Raw strings: `r#"..."#`, `r##"..."##`, etc. (content is verbatim)
/// - Regular strings: `"..."` with standard Rust escape sequences
///
/// # Errors
/// Returns an error if the token is not a recognized string literal form or contains invalid escapes.
pub fn decode_string_literal(token: &str) -> Result<String> {
    if token.starts_with('r') {
        decode_raw_string(token)
    } else if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
        let inner = &token[1..token.len() - 1];
        decode_string_escapes(inner)
    } else {
        bail!(
            "Unrecognized string literal form: {}",
            &token[..token.len().min(40)]
        )
    }
}

fn decode_raw_string(token: &str) -> Result<String> {
    let after_r = &token[1..];
    let hash_count = after_r.chars().take_while(|&c| c == '#').count();
    let inner = &after_r[hash_count..];
    let inner = inner
        .strip_prefix('"')
        .ok_or_else(|| anyhow::anyhow!("Invalid raw string literal: missing opening quote"))?;
    let closing = format!("\"{}", "#".repeat(hash_count));
    let inner = inner.strip_suffix(closing.as_str()).ok_or_else(|| {
        anyhow::anyhow!("Invalid raw string literal: missing closing quote+hashes")
    })?;
    Ok(inner.to_owned())
}

fn decode_string_escapes(s: &str) -> Result<String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('\\') => result.push('\\'),
            Some('"') => result.push('"'),
            Some('\'') => result.push('\''),
            Some('0') => result.push('\0'),
            Some('u') => {
                if chars.next() != Some('{') {
                    bail!("Invalid unicode escape: expected {{");
                }
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let code = u32::from_str_radix(&hex, 16)
                    .map_err(|_| anyhow::anyhow!("Invalid unicode escape \\u{{{hex}}}"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| anyhow::anyhow!("Invalid unicode code point {code}"))?;
                result.push(ch);
            }
            Some(c) => bail!("Unsupported escape sequence: \\{c}"),
            None => bail!("Unexpected end of string after backslash"),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_string_no_hashes() {
        let token = "r\"{\\\"k\\\": 1}\"";
        // r"{\"k\": 1}" — content is verbatim including backslashes
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "{\\\"k\\\": 1}");
    }

    #[test]
    fn test_raw_string_one_hash() {
        // Simulated: r#"{"k": 1}"#
        let token = "r#\"{\\\"k\\\": 1}\"#";
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "{\\\"k\\\": 1}");
    }

    #[test]
    fn test_regular_string_simple() {
        let token = "\"hello world\"";
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_regular_string_escapes() {
        let token = "\"line1\\nline2\\ttab\"";
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "line1\nline2\ttab");
    }

    #[test]
    fn test_regular_string_escaped_quote() {
        let token = r#""{\"name\":\"foo\"}""#;
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "{\"name\":\"foo\"}");
    }

    #[test]
    fn test_unicode_escape() {
        let token = "\"\\u{1F600}\"";
        let result = decode_string_literal(token).unwrap();
        assert_eq!(result, "\u{1F600}");
    }

    #[test]
    fn test_invalid_escape() {
        let token = "\"\\q\"";
        assert!(decode_string_literal(token).is_err());
    }

    #[test]
    fn test_unrecognized_form() {
        let token = "b\"bytes\"";
        assert!(decode_string_literal(token).is_err());
    }
}
