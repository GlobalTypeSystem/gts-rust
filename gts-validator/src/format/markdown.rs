//! Markdown file scanner for GTS identifiers.
//!
//! Uses a two-stage approach:
//! 1. Discovery regex finds candidates
//! 2. `normalize_candidate()` → `validate_candidate()` validates them

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::ValidationError;
use crate::normalize::normalize_candidate;
use crate::validator::{is_bad_example_context, is_wildcard_context, validate_candidate};

/// Markdown parsing state for code block tracking
#[derive(Debug, Clone, PartialEq, Eq)]
enum MarkdownState {
    Prose,
    FencedBlock {
        skip: bool,
        fence_char: char,
        opening_fence_len: usize,
    },
}

fn parse_fence(trimmed_line: &str) -> Option<(char, usize)> {
    let fence_char = match trimmed_line.as_bytes().first() {
        Some(b'`') => '`',
        Some(b'~') => '~',
        _ => return None,
    };

    let fence_len = trimmed_line
        .chars()
        .take_while(|&c| c == fence_char)
        .count();
    if fence_len >= 3 {
        Some((fence_char, fence_len))
    } else {
        None
    }
}

/// Discovery regex (relaxed): finds strings that LOOK like GTS identifiers.
/// This is intentionally broader than the spec — validation is done by `GtsID::new()`.
///
/// Strategy: Match gts. followed by 4+ dot-separated segments where at least one
/// segment looks like a version (starts with 'v' followed by digit).
/// This catches both valid and malformed IDs for validation (more errors reported).
/// Stops at tilde followed by non-alphanumeric to avoid matching filenames like "id.v1~.schema.json"
static GTS_DISCOVERY_PATTERN_RELAXED: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(concat!(
        r"(?:gts://)?",                    // optional URI prefix
        r"\bgts\.", // mandatory gts. prefix (word boundary prevents xgts. match)
        r"(?:[a-z_*][a-z0-9_*.-]*\.){3,}", // at least 3 segments (permissive: allows -, .)
        r"[a-z_*][a-z0-9_*.-]*", // final segment before version
        r"\.v[0-9]+", // version segment (required anchor)
        r"(?:\.[0-9]+)?", // optional minor version
        r"(?:~[a-z_][a-z0-9_.-]*)*", // optional chained segments (permissive)
        r"~?",      // optional trailing tilde (but not if followed by .)
    )) {
        Ok(regex) => regex,
        Err(err) => panic!("Invalid discovery regex: {err}"),
    }
});

/// Discovery regex (well-formed): only matches well-formed GTS identifiers.
/// Requires exactly 5 segments with proper structure (fewer errors reported).
static GTS_DISCOVERY_PATTERN_WELL_FORMED: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(concat!(
        r"(?:gts://)?",          // optional URI prefix
        r"\bgts\.",              // mandatory gts. prefix (word boundary prevents xgts. match)
        r"[a-z_*][a-z0-9_*]*\.", // vendor
        r"[a-z_*][a-z0-9_*]*\.", // package
        r"[a-z_*][a-z0-9_*]*\.", // namespace
        r"[a-z_*][a-z0-9_*]*\.", // type
        r"v[0-9]+",              // major version (required)
        r"(?:\.[0-9]+)?",        // optional minor version
        r"(?:~[a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*\.v[0-9]+(?:\.[0-9]+)?)*", // chained segments
        r"~?", // optional trailing tilde
    )) {
        Ok(regex) => regex,
        Err(err) => panic!("Invalid discovery regex: {err}"),
    }
});

/// Scan markdown content for GTS identifiers.
pub fn scan_markdown_content(
    content: &str,
    path: &Path,
    vendor: Option<&str>,
    heuristic: bool,
    skip_tokens: &[String],
) -> Vec<ValidationError> {
    let pattern = if heuristic {
        &*GTS_DISCOVERY_PATTERN_RELAXED
    } else {
        &*GTS_DISCOVERY_PATTERN_WELL_FORMED
    };
    let mut errors = Vec::new();
    let mut state = MarkdownState::Prose;
    let mut seen_candidates: HashSet<(usize, String)> = HashSet::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_number = line_num + 1; // 1-indexed

        // Update markdown state for code blocks (``` and ~~~ per CommonMark spec)
        let trimmed_line = line.trim_start();
        if let Some((fence_char, fence_len)) = parse_fence(trimmed_line) {
            match &state {
                MarkdownState::Prose => {
                    // Entering a fenced block
                    let language = trimmed_line[fence_len..].trim().to_lowercase();

                    // Skip grammar/pattern definition blocks
                    let skip = matches!(
                        language.as_str(),
                        "ebnf" | "regex" | "bnf" | "abnf" | "grammar"
                    );

                    state = MarkdownState::FencedBlock {
                        skip,
                        fence_char,
                        opening_fence_len: fence_len,
                    };
                    continue;
                }
                MarkdownState::FencedBlock {
                    fence_char: open_fence_char,
                    opening_fence_len,
                    ..
                } => {
                    // Exiting a fenced block requires matching delimiter with sufficient length.
                    if fence_char == *open_fence_char && fence_len >= *opening_fence_len {
                        state = MarkdownState::Prose;
                        continue;
                    }
                }
            }
        }

        // Skip lines inside skip blocks
        if let MarkdownState::FencedBlock { skip: true, .. } = state {
            continue;
        }

        // Find all GTS candidates on this line
        for mat in pattern.find_iter(line) {
            let candidate_str = mat.as_str();
            let match_start = mat.start();

            // Deduplicate: skip if we've seen this candidate on this line
            if !seen_candidates.insert((line_number, candidate_str.to_owned())) {
                continue;
            }

            // Skip validation if this is a "bad example" context
            if is_bad_example_context(line, mat.start()) {
                continue;
            }

            // Check consumer-provided skip tokens
            if !skip_tokens.is_empty()
                && let Some(before) = line.get(..mat.start())
            {
                let before_lower = before.to_lowercase();
                if skip_tokens
                    .iter()
                    .any(|token| before_lower.contains(&token.to_lowercase()))
                {
                    continue;
                }
            }

            // Normalize the candidate
            let candidate = match normalize_candidate(candidate_str) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(ValidationError {
                        file: path.to_owned(),
                        line: line_number,
                        column: match_start + 1, // 1-indexed
                        json_path: String::new(),
                        raw_value: candidate_str.to_owned(),
                        normalized_id: String::new(),
                        error: e,
                        context: line.to_owned(),
                    });
                    continue;
                }
            };

            // Check if wildcards are allowed in this context
            let allow_wildcards = is_wildcard_context(line, match_start);

            // Validate the candidate
            let validation_errors = validate_candidate(&candidate, vendor, allow_wildcards);
            for err in validation_errors {
                errors.push(ValidationError {
                    file: path.to_owned(),
                    line: line_number,
                    column: match_start + 1, // 1-indexed
                    json_path: String::new(),
                    raw_value: candidate.original.clone(),
                    normalized_id: candidate.gts_id.clone(),
                    error: err,
                    context: line.to_owned(),
                });
            }
        }
    }

    errors
}

/// Scan a markdown file for GTS identifiers (file-based convenience wrapper).
#[cfg(test)]
pub fn scan_markdown_file(
    path: &Path,
    vendor: Option<&str>,
    max_file_size: u64,
    heuristic: bool,
) -> Vec<ValidationError> {
    // Check file size
    if let Ok(metadata) = std::fs::metadata(path)
        && metadata.len() > max_file_size
    {
        return vec![];
    }

    // Read as UTF-8; skip file on encoding error
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_e) => return vec![],
    };

    scan_markdown_content(&content, path, vendor, heuristic, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_md(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_scan_markdown_valid_id() {
        let file = create_temp_md("The type is gts.x.core.events.type.v1~");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_scan_markdown_invalid_id() {
        let file = create_temp_md("The type is gts.x.core.events.type.v1");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            !errors.is_empty(),
            "Single-segment instance ID should be rejected"
        );
    }

    #[test]
    fn test_scan_markdown_skip_ebnf_block() {
        let content = r"
```ebnf
gts.invalid.pattern.here.v1~
```
";
        let file = create_temp_md(content);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "EBNF blocks should be skipped");
    }

    #[test]
    fn test_scan_markdown_validate_json_block() {
        let content = r#"
```json
{"$id": "gts://gts.x.core.events.type.v1~"}
```
"#;
        let file = create_temp_md(content);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "JSON blocks should be validated");
    }

    #[test]
    fn test_scan_markdown_skip_invalid_context() {
        let file = create_temp_md("\u{274c} gts.invalid.id.here.v1");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "Invalid examples should be skipped");
    }

    #[test]
    fn test_scan_markdown_wildcard_in_pattern_context() {
        let file = create_temp_md("pattern: gts.x.core.events.type.v1~");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "Valid IDs in pattern context should be allowed"
        );
    }

    #[test]
    fn test_scan_markdown_wildcard_not_in_pattern_context() {
        let file = create_temp_md("The type is gts.x.core.events.type.v1~");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "Valid IDs should pass");
    }

    #[test]
    fn test_scan_markdown_gts_uri() {
        let file = create_temp_md(r#"Use "$id": "gts://gts.x.core.events.type.v1~""#);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "gts:// URIs should be normalized and validated"
        );
    }

    #[test]
    fn test_scan_markdown_vendor_mismatch() {
        let file = create_temp_md("The type is gts.hx.core.events.type.v1~");
        let errors = scan_markdown_file(file.path(), Some("x"), 10_485_760, false);
        assert!(!errors.is_empty());
        assert!(errors[0].error.contains("Vendor mismatch"));
    }

    #[test]
    fn test_scan_markdown_example_vendor_tolerated() {
        let file = create_temp_md("Example: gts.acme.core.events.type.v1~");
        let errors = scan_markdown_file(file.path(), Some("x"), 10_485_760, false);
        assert!(errors.is_empty(), "Example vendors should be tolerated");
    }

    #[test]
    fn test_scan_markdown_deduplication() {
        // Use an invalid ID (wrong vendor) twice on the same line — dedup should produce exactly 1 error
        let file = create_temp_md(
            "gts.wrongvendor.core.events.type.v1~ and gts.wrongvendor.core.events.type.v1~ again",
        );
        let errors = scan_markdown_file(file.path(), Some("x"), 10_485_760, false);
        assert_eq!(
            errors.len(),
            1,
            "Duplicate invalid ID on same line should produce exactly 1 error, got: {errors:?}"
        );
    }

    #[test]
    fn test_scan_markdown_error_after_gts_id() {
        let file = create_temp_md("gts.x.core.events.type.v1~ handles error cases");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "Valid ID should not be suppressed by 'error' appearing after it"
        );
    }

    #[test]
    fn test_scan_markdown_invalid_before_gts_id() {
        let file = create_temp_md("invalid: gts.bad.format.here.v1");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "Invalid examples should be skipped");
    }

    #[test]
    fn test_scan_markdown_heuristic_mode_catches_malformed() {
        let file = create_temp_md("The type is gts.my-vendor.core.events.type.v1~");
        let errors_heuristic = scan_markdown_file(file.path(), None, 10_485_760, true);
        let errors_normal = scan_markdown_file(file.path(), None, 10_485_760, false);

        assert!(
            !errors_heuristic.is_empty(),
            "Heuristic mode should catch malformed ID with hyphens"
        );
        assert!(
            errors_normal.is_empty(),
            "Normal mode won't match malformed pattern"
        );
    }

    #[test]
    fn test_scan_markdown_heuristic_mode_catches_extra_dots() {
        let file = create_temp_md("The type is gts.x.core.events.type.name.v1~");
        let errors_heuristic = scan_markdown_file(file.path(), None, 10_485_760, true);

        assert!(
            !errors_heuristic.is_empty(),
            "Heuristic mode should catch ID with extra segments"
        );
    }

    #[test]
    fn test_scan_markdown_normal_mode_well_formed_only() {
        let file = create_temp_md("Valid: gts.x.core.events.type.v1~ and malformed: gts.bad-id.v1");
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);

        assert!(
            errors.is_empty(),
            "Normal mode should only validate well-formed patterns"
        );
    }

    #[test]
    fn test_scan_markdown_skip_tokens() {
        // skip_tokens should suppress validation when the token appears before the candidate
        let content = "**given** gts.bad.format.here.v1~";
        let errors = scan_markdown_content(
            content,
            Path::new("test.md"),
            None,
            true, // heuristic mode to ensure the relaxed regex would catch it
            &["**given**".to_owned()],
        );
        assert!(
            errors.is_empty(),
            "skip_tokens should suppress validation: {errors:?}"
        );

        // Same content without skip_tokens should produce errors (vendor mismatch)
        let content_mismatch = "**given** gts.y.core.pkg.mytype.v1~ is registered";
        let errors_no_skip = scan_markdown_content(
            content_mismatch,
            Path::new("test.md"),
            Some("x"),
            false,
            &[],
        );
        assert!(
            !errors_no_skip.is_empty(),
            "Without skip_tokens, vendor mismatch should be reported"
        );

        // With skip_tokens, the same content should be suppressed
        let errors_with_skip = scan_markdown_content(
            content_mismatch,
            Path::new("test.md"),
            Some("x"),
            false,
            &["**given**".to_owned()],
        );
        assert!(
            errors_with_skip.is_empty(),
            "With skip_tokens, vendor mismatch should be suppressed: {errors_with_skip:?}"
        );
    }

    #[test]
    fn test_scan_markdown_tilde_fence() {
        // ~~~ fences should be handled the same as ``` fences
        let content = "~~~ebnf\ngts.invalid.pattern.here.v1~\n~~~\n";
        let file = create_temp_md(content);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "~~~ EBNF blocks should be skipped: {errors:?}"
        );
    }

    #[test]
    fn test_scan_markdown_tilde_fence_json_validated() {
        // ~~~json blocks should be validated (same as ```json)
        let content = "~~~json\n{\"$id\": \"gts://gts.x.core.events.type.v1~\"}\n~~~\n";
        let file = create_temp_md(content);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "~~~json blocks should be validated and pass: {errors:?}"
        );
    }

    #[test]
    fn test_scan_markdown_mismatched_fence_does_not_close_block() {
        // A ~~~ line must not close a ``` block.
        let content = "```ebnf\n~~~\ngts.bad.format.here.v1~\n```\n";
        let file = create_temp_md(content);
        let errors = scan_markdown_file(file.path(), None, 10_485_760, true);
        assert!(
            errors.is_empty(),
            "Mismatched fence should not close block; content inside ebnf block must be skipped: {errors:?}"
        );
    }

    #[test]
    fn test_scan_markdown_word_boundary() {
        // Regex should NOT match "xgts.x.core.events.type.v1~" (no word boundary)
        let content = "The identifier xgts.x.core.events.type.v1~ is wrong";
        let errors = scan_markdown_content(content, Path::new("test.md"), None, false, &[]);
        assert!(
            errors.is_empty(),
            "Word boundary should prevent matching xgts.*: {errors:?}"
        );
    }
}
