//! JSONC (JSON with Comments) support.
//!
//! JSONC extends JSON with `//` line comments, `/* */` block comments and
//! trailing commas before a closing `}` or `]`. [`strip`] blanks those out in
//! place: every removed byte becomes a space and line breaks are kept, so the
//! result is strict JSON with the same byte offsets as the input and
//! `serde_json` errors keep pointing at the original line and column.
//!
//! Only `*.jsonc` files (see [`is_jsonc_path`]) and request bodies sent as
//! [`JSONC_MEDIA_TYPE`] are parsed as JSONC; `*.json` stays strict JSON.

use std::borrow::Cow;
use std::path::Path;

use serde::de::DeserializeOwned;

/// File extension (without the dot) of JSONC documents.
pub const JSONC_EXTENSION: &str = "jsonc";

/// Media type of JSONC request bodies.
pub const JSONC_MEDIA_TYPE: &str = "application/jsonc";

/// Whether `path` names a JSONC document, i.e. has a `.jsonc` extension (case-insensitive).
#[must_use]
pub fn is_jsonc_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(JSONC_EXTENSION))
}

/// Turn JSONC into strict JSON by blanking comments, trailing commas and a
/// leading UTF-8 byte order mark.
///
/// Byte offsets and line breaks are preserved. Malformed input (e.g. an
/// unterminated block comment or string) is left as is for the JSON parser to
/// reject at the right position.
#[must_use]
pub fn strip(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    let mut out: Option<Vec<u8>> = None;
    let mut blank = |range: std::ops::Range<usize>| {
        let out = out.get_or_insert_with(|| bytes.to_vec());
        for b in &mut out[range] {
            if !matches!(*b, b'\n' | b'\r') {
                *b = b' ';
            }
        }
    };

    // Last significant byte seen outside strings and comments, and the
    // position of a comma that may turn out to be trailing.
    let mut prev = 0u8;
    let mut pending_comma: Option<usize> = None;
    let mut i = 0;
    if input.starts_with('\u{feff}') {
        i = '\u{feff}'.len_utf8();
        blank(0..i);
    }
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1)) {
            (b'"', _) => {
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => break,
                        _ => i += 1,
                    }
                }
                i += 1;
                prev = b'"';
                pending_comma = None;
            }
            (b'/', Some(b'/')) => {
                let end = bytes[i..]
                    .iter()
                    .position(|b| matches!(b, b'\n' | b'\r'))
                    .map_or(bytes.len(), |len| i + len);
                blank(i..end);
                i = end;
            }
            (b'/', Some(b'*')) => {
                let Some(len) = input[i + 2..].find("*/") else {
                    break;
                };
                let end = i + 2 + len + 2;
                blank(i..end);
                i = end;
            }
            (b' ' | b'\t' | b'\n' | b'\r', _) => i += 1,
            (b, _) => {
                match b {
                    b',' => {
                        pending_comma =
                            (!matches!(prev, 0 | b'[' | b'{' | b',' | b':')).then_some(i);
                    }
                    b'}' | b']' => {
                        if let Some(comma) = pending_comma.take() {
                            blank(comma..comma + 1);
                        }
                    }
                    _ => pending_comma = None,
                }
                prev = b;
                i += 1;
            }
        }
    }

    // Only whole characters were replaced by ASCII spaces, so the output is
    // still valid UTF-8 and the lossy fallback is unreachable.
    out.map_or(Cow::Borrowed(input), |out| {
        Cow::Owned(
            String::from_utf8(out)
                .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned()),
        )
    })
}

/// Deserialize a JSONC document.
///
/// # Errors
///
/// Returns the `serde_json` error for input that is not valid JSONC or does
/// not match `T`; line and column refer to the original input.
pub fn from_str<T: DeserializeOwned>(input: &str) -> serde_json::Result<T> {
    serde_json::from_str(&strip(input))
}

/// Deserialize `content` read from `path`: as JSONC for `*.jsonc`, as strict JSON otherwise.
///
/// # Errors
///
/// Returns the `serde_json` error for content that does not parse or does not match `T`.
pub fn from_str_for_path<T: DeserializeOwned>(path: &Path, content: &str) -> serde_json::Result<T> {
    if is_jsonc_path(path) {
        from_str(content)
    } else {
        serde_json::from_str(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn parse(input: &str) -> serde_json::Result<Value> {
        from_str(input)
    }

    #[test]
    fn strict_json_is_borrowed_unchanged() {
        let input = r#"{"a": [1, 2], "b": "x"}"#;
        assert!(matches!(strip(input), Cow::Borrowed(s) if s == input));
    }

    #[test]
    fn line_and_block_comments_are_removed() {
        let input = "// Invalid: reason\n{\n  /* block\n  comment */ \"a\": 1, // trailing\n  \"b\": 2\n}\n";
        assert_eq!(parse(input).unwrap(), json!({"a": 1, "b": 2}));
    }

    #[test]
    fn comments_with_crlf_and_eof() {
        assert_eq!(parse("{\"a\": 1}\r\n// end").unwrap(), json!({"a": 1}));
        assert_eq!(parse("[1 /**/, 2]").unwrap(), json!([1, 2]));
    }

    #[test]
    fn trailing_commas_are_removed() {
        assert_eq!(
            parse("{\"a\": [1, 2,], \"b\": {\"c\": 3, },}").unwrap(),
            json!({"a": [1, 2], "b": {"c": 3}})
        );
        assert_eq!(parse("[1, // c\n /* d */ ]").unwrap(), json!([1]));
        assert_eq!(parse("[\"s\",\n]").unwrap(), json!(["s"]));
    }

    #[test]
    fn comment_markers_inside_strings_are_kept() {
        let input = r#"{"url": "gts://gts.x.a.b.c.v1~", "c": "/* not */", "e": "q\"//,]"}"#;
        assert!(matches!(strip(input), Cow::Borrowed(_)));
        assert_eq!(
            parse(input).unwrap(),
            json!({"url": "gts://gts.x.a.b.c.v1~", "c": "/* not */", "e": "q\"//,]"})
        );
    }

    #[test]
    fn non_ascii_in_comments_and_strings() {
        let input = "{\"name\": \"caf\u{e9}\" /* \u{fc}ber */, // \u{65e5}\u{672c}\n}";
        assert_eq!(parse(input).unwrap(), json!({"name": "caf\u{e9}"}));
    }

    #[test]
    fn leading_byte_order_mark_is_removed() {
        let input = "\u{feff}// c\n{\"a\": 1,}";
        assert_eq!(strip(input).len(), input.len());
        assert_eq!(parse(input).unwrap(), json!({"a": 1}));
        assert_eq!(parse("\u{feff}[1]").unwrap(), json!([1]));
        // Only a leading mark is a byte order mark.
        assert!(parse("[1]\u{feff}").is_err());
    }

    #[test]
    fn offsets_and_lines_are_preserved() {
        let input = "{\n  // c\n  \"a\": 1,\n}";
        let stripped = strip(input);
        assert_eq!(stripped.len(), input.len());
        assert_eq!(stripped.lines().count(), input.lines().count());

        let err = parse("// header\n{\n  \"a\": nope\n}").unwrap_err();
        let strict = serde_json::from_str::<Value>("\n{\n  \"a\": nope\n}").unwrap_err();
        assert_eq!(err.line(), 3);
        assert_eq!((err.line(), err.column()), (strict.line(), strict.column()));
    }

    #[test]
    fn malformed_input_is_still_rejected() {
        for input in [
            "{\"a\": 1 /* unterminated",
            "[,]",
            "[1,,]",
            "{\"a\":,}",
            "{,}",
            ",",
            "[1 2]",
            "{'a': 1}",
            "\"unterminated",
        ] {
            assert!(parse(input).is_err(), "{input:?} must not parse");
        }
    }

    #[test]
    fn path_detection_and_dispatch() {
        assert!(is_jsonc_path(Path::new("a/b.jsonc")));
        assert!(is_jsonc_path(Path::new("B.JSONC")));
        assert!(!is_jsonc_path(Path::new("b.json")));
        assert!(!is_jsonc_path(Path::new("jsonc")));

        let content = "{\"a\": 1, // c\n}";
        assert_eq!(
            from_str_for_path::<Value>(Path::new("x.jsonc"), content).unwrap(),
            json!({"a": 1})
        );
        assert!(from_str_for_path::<Value>(Path::new("x.json"), content).is_err());
    }
}
