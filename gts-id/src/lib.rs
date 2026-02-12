//! Shared GTS ID validation and parsing primitives.
//!
//! This crate provides the single source of truth for GTS identifier validation,
//! used by both the `gts` runtime library and the `gts-macros` proc-macro crate.

use thiserror::Error;

/// The required prefix for all GTS identifiers.
pub const GTS_PREFIX: &str = "gts.";

/// Maximum allowed length for a GTS identifier string.
pub const GTS_MAX_LENGTH: usize = 1024;

/// Errors from GTS ID validation.
#[derive(Debug, Error)]
pub enum GtsIdError {
    /// A specific segment within the ID is invalid.
    #[error("Segment #{num}: {cause}")]
    Segment {
        /// 1-based segment number.
        num: usize,
        /// Byte offset of this segment within the full ID string.
        offset: usize,
        /// The raw segment string that failed validation.
        segment: String,
        /// Human-readable description of the problem.
        cause: String,
    },

    /// The ID as a whole is invalid (prefix, case, length, etc.).
    #[error("Invalid GTS ID: {cause}")]
    Id {
        /// The raw ID string that failed validation.
        id: String,
        /// Human-readable description of the problem.
        cause: String,
    },
}

/// Result of successfully parsing a single GTS segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSegment {
    /// The raw segment string (including trailing `~` if present).
    pub raw: String,
    /// Byte offset of this segment within the full ID string.
    pub offset: usize,
    /// Vendor token (1st dot-separated token).
    pub vendor: String,
    /// Package token (2nd dot-separated token).
    pub package: String,
    /// Namespace token (3rd dot-separated token).
    pub namespace: String,
    /// Type name token (4th dot-separated token).
    pub type_name: String,
    /// Major version number.
    pub ver_major: u32,
    /// Optional minor version number.
    pub ver_minor: Option<u32>,
    /// Whether this segment ends with `~` (type marker).
    pub is_type: bool,
    /// Whether this segment contains a wildcard `*` token.
    pub is_wildcard: bool,
}

/// Expected format string for segment error messages.
///
/// Segment #1 shows the `gts.` prefix because the user writes
/// `gts.vendor.package...`; segments #2+ omit it because they
/// come after a `~` delimiter.
#[must_use]
fn expected_format(segment_num: usize) -> &'static str {
    if segment_num == 1 {
        "gts.vendor.package.namespace.type.vMAJOR[.MINOR]"
    } else {
        "vendor.package.namespace.type.vMAJOR[.MINOR]"
    }
}

/// Validates a GTS segment token without regex.
///
/// Valid tokens: start with `[a-z_]`, followed by `[a-z0-9_]*`.
#[inline]
#[must_use]
pub fn is_valid_segment_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Parse a `u32` and reject leading zeros (except `"0"` itself).
#[inline]
#[must_use]
pub fn parse_u32_exact(value: &str) -> Option<u32> {
    let parsed = value.parse::<u32>().ok()?;
    if parsed.to_string() == value {
        Some(parsed)
    } else {
        None
    }
}

/// Validate and parse a single GTS segment (the part between `~` markers).
///
/// # Arguments
/// * `segment_num` - 1-based segment number (used in error messages and format hints)
/// * `segment` - The raw segment string, possibly including a trailing `~`
/// * `allow_wildcards` - If `true`, a trailing wildcard `*` token is accepted as the final token
///
/// # Errors
/// Returns a human-readable error message if the segment is invalid.
pub fn validate_segment(
    segment_num: usize,
    segment: &str,
    allow_wildcards: bool,
) -> Result<ParsedSegment, String> {
    let mut seg = segment.to_owned();
    let mut is_type = false;

    // Check for type marker (~)
    if seg.contains('~') {
        let tilde_count = seg.matches('~').count();
        if tilde_count > 1 {
            return Err("Too many '~' characters".to_owned());
        }
        if seg.ends_with('~') {
            is_type = true;
            seg.pop();
        } else {
            return Err("'~' must be at the end".to_owned());
        }
    }

    let tokens: Vec<&str> = seg.split('.').collect();
    let fmt = expected_format(segment_num);

    if tokens.len() > 6 {
        return Err(format!(
            "Too many tokens (got {}, max 6). Expected format: {fmt}",
            tokens.len()
        ));
    }

    let ends_with_wildcard = allow_wildcards && seg.ends_with('*');

    if !ends_with_wildcard && tokens.len() < 5 {
        return Err(format!(
            "Too few tokens (got {}, min 5). Expected format: {fmt}",
            tokens.len()
        ));
    }

    // Detect extra name token before version (e.g., vendor.package.namespace.type.extra.v1)
    if !ends_with_wildcard && tokens.len() == 6 {
        let has_wildcard = allow_wildcards && tokens.contains(&"*");
        if !has_wildcard
            && !tokens[4].starts_with('v')
            && tokens[5].starts_with('v')
            && is_valid_segment_token(tokens[4])
        {
            return Err(format!(
                "Too many name tokens before version (got 5, expected 4). Expected format: {fmt}"
            ));
        }
    }

    // Validate first 4 tokens (vendor, package, namespace, type).
    // A trailing '*' wildcard is allowed as the final token, but all tokens
    // before it must still pass validation. Wildcards in the middle
    // (e.g., "x.*.ns.type.v1") are rejected because '*' fails is_valid_segment_token.
    for (i, token) in tokens.iter().take(4).enumerate() {
        if allow_wildcards && *token == "*" {
            if i == tokens.len() - 1 {
                break; // '*' as final token is handled in the parsing section below
            }
            return Err("Wildcard '*' is only allowed as the final token".to_owned());
        }
        if !is_valid_segment_token(token) {
            let token_name = match i {
                0 => "vendor",
                1 => "package",
                2 => "namespace",
                3 => "type",
                _ => "token",
            };
            return Err(format!(
                "Invalid {token_name} token '{token}'. \
                 Must start with [a-z_] and contain only [a-z0-9_]"
            ));
        }
    }

    // Build the result, parsing tokens progressively.
    // Offset is set to 0 here; callers like validate_gts_id() override it
    // with the actual position within the full ID string.
    let mut result = ParsedSegment {
        raw: segment.to_owned(),
        offset: 0,
        vendor: String::new(),
        package: String::new(),
        namespace: String::new(),
        type_name: String::new(),
        ver_major: 0,
        ver_minor: None,
        is_type,
        is_wildcard: false,
    };

    if !tokens.is_empty() {
        if allow_wildcards && tokens[0] == "*" {
            result.is_wildcard = true;
            return Ok(result);
        }
        tokens[0].clone_into(&mut result.vendor);
    }

    if tokens.len() > 1 {
        if allow_wildcards && tokens[1] == "*" {
            result.is_wildcard = true;
            return Ok(result);
        }
        tokens[1].clone_into(&mut result.package);
    }

    if tokens.len() > 2 {
        if allow_wildcards && tokens[2] == "*" {
            result.is_wildcard = true;
            return Ok(result);
        }
        tokens[2].clone_into(&mut result.namespace);
    }

    if tokens.len() > 3 {
        if allow_wildcards && tokens[3] == "*" {
            result.is_wildcard = true;
            return Ok(result);
        }
        tokens[3].clone_into(&mut result.type_name);
    }

    if tokens.len() > 4 {
        if allow_wildcards && tokens[4] == "*" {
            if 4 != tokens.len() - 1 {
                return Err("Wildcard '*' is only allowed as the final token".to_owned());
            }
            result.is_wildcard = true;
            return Ok(result);
        }

        if !tokens[4].starts_with('v') {
            return Err("Major version must start with 'v'".to_owned());
        }

        let major_str = &tokens[4][1..];
        result.ver_major = parse_u32_exact(major_str)
            .ok_or_else(|| format!("Major version must be an integer, got '{major_str}'"))?;
    }

    if tokens.len() > 5 {
        if allow_wildcards && tokens[5] == "*" {
            result.is_wildcard = true;
            return Ok(result);
        }

        result.ver_minor = Some(
            parse_u32_exact(tokens[5])
                .ok_or_else(|| format!("Minor version must be an integer, got '{}'", tokens[5]))?,
        );
    }

    Ok(result)
}

/// Validate a full GTS identifier string.
///
/// Checks the `gts.` prefix, lowercase, no hyphens, length, then splits
/// by `~` and validates each segment via [`validate_segment`].
///
/// # Arguments
/// * `id` - The raw GTS identifier string
/// * `allow_wildcards` - If `true`, wildcard `*` tokens are accepted
///
/// # Errors
/// Returns [`GtsIdError`] on validation failure.
pub fn validate_gts_id(id: &str, allow_wildcards: bool) -> Result<Vec<ParsedSegment>, GtsIdError> {
    let raw = id.trim();

    if !raw.starts_with(GTS_PREFIX) {
        return Err(GtsIdError::Id {
            id: id.to_owned(),
            cause: format!("must start with '{GTS_PREFIX}'"),
        });
    }

    if raw != raw.to_lowercase() {
        return Err(GtsIdError::Id {
            id: id.to_owned(),
            cause: "must be lowercase".to_owned(),
        });
    }

    if raw.contains('-') {
        return Err(GtsIdError::Id {
            id: id.to_owned(),
            cause: "must not contain '-'".to_owned(),
        });
    }

    if raw.len() > GTS_MAX_LENGTH {
        return Err(GtsIdError::Id {
            id: id.to_owned(),
            cause: format!("too long ({} chars, max {GTS_MAX_LENGTH})", raw.len()),
        });
    }

    let remainder = &raw[GTS_PREFIX.len()..];
    let tilde_parts: Vec<&str> = remainder.split('~').collect();

    let mut segments_raw = Vec::new();
    for i in 0..tilde_parts.len() {
        if i < tilde_parts.len() - 1 {
            segments_raw.push(format!("{}~", tilde_parts[i]));
            if i == tilde_parts.len() - 2 && tilde_parts[i + 1].is_empty() {
                break;
            }
        } else {
            segments_raw.push(tilde_parts[i].to_owned());
        }
    }

    if segments_raw.is_empty() {
        return Err(GtsIdError::Id {
            id: id.to_owned(),
            cause: "no segments found".to_owned(),
        });
    }

    let mut parsed_segments = Vec::new();
    let mut offset = GTS_PREFIX.len();
    for (i, seg) in segments_raw.iter().enumerate() {
        if seg.is_empty() || seg == "~" {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: format!("segment #{} @ offset {offset} is empty", i + 1),
            });
        }

        let mut parsed =
            validate_segment(i + 1, seg, allow_wildcards).map_err(|cause| GtsIdError::Segment {
                num: i + 1,
                offset,
                segment: seg.clone(),
                cause,
            })?;
        parsed.offset = offset;
        offset += seg.len();
        parsed_segments.push(parsed);
    }

    Ok(parsed_segments)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ---- is_valid_segment_token ----

    #[test]
    fn test_valid_tokens() {
        assert!(is_valid_segment_token("abc"));
        assert!(is_valid_segment_token("a1b2"));
        assert!(is_valid_segment_token("_private"));
        assert!(is_valid_segment_token("a_b_c"));
    }

    #[test]
    fn test_invalid_tokens() {
        assert!(!is_valid_segment_token(""));
        assert!(!is_valid_segment_token("1abc"));
        assert!(!is_valid_segment_token("ABC"));
        assert!(!is_valid_segment_token("a-b"));
        assert!(!is_valid_segment_token("a.b"));
    }

    // ---- parse_u32_exact ----

    #[test]
    fn test_parse_u32_exact_valid() {
        assert_eq!(parse_u32_exact("0"), Some(0));
        assert_eq!(parse_u32_exact("1"), Some(1));
        assert_eq!(parse_u32_exact("42"), Some(42));
    }

    #[test]
    fn test_parse_u32_exact_rejects_leading_zeros() {
        assert_eq!(parse_u32_exact("01"), None);
        assert_eq!(parse_u32_exact("007"), None);
    }

    #[test]
    fn test_parse_u32_exact_rejects_non_numeric() {
        assert_eq!(parse_u32_exact("abc"), None);
        assert_eq!(parse_u32_exact(""), None);
    }

    // ---- validate_segment ----

    #[test]
    fn test_valid_segment_basic() {
        let parsed = validate_segment(1, "x.core.events.event.v1~", false).unwrap();
        assert_eq!(parsed.vendor, "x");
        assert_eq!(parsed.package, "core");
        assert_eq!(parsed.namespace, "events");
        assert_eq!(parsed.type_name, "event");
        assert_eq!(parsed.ver_major, 1);
        assert_eq!(parsed.ver_minor, None);
        assert!(parsed.is_type);
        assert!(!parsed.is_wildcard);
    }

    #[test]
    fn test_valid_segment_with_minor() {
        let parsed = validate_segment(1, "x.core.events.event.v1.2~", false).unwrap();
        assert_eq!(parsed.ver_major, 1);
        assert_eq!(parsed.ver_minor, Some(2));
    }

    #[test]
    fn test_segment_too_many_tildes() {
        let err = validate_segment(1, "x.core.events.event.v1~~", false).unwrap_err();
        assert!(err.contains("Too many '~' characters"), "got: {err}");
    }

    #[test]
    fn test_segment_tilde_not_at_end() {
        let err = validate_segment(1, "x.core~mid.events.event.v1", false).unwrap_err();
        assert!(err.contains("'~' must be at the end"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_tokens() {
        let err = validate_segment(1, "x.core.events.event.v1.2.extra~", false).unwrap_err();
        assert!(err.contains("Too many tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_few_tokens() {
        let err = validate_segment(1, "x.core.events.event~", false).unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_name_tokens() {
        let err = validate_segment(2, "x.core.ns.type.extra.v1~", false).unwrap_err();
        assert!(
            err.contains("Too many name tokens before version"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_without_v() {
        let err = validate_segment(1, "x.core.events.event.1~", false).unwrap_err();
        assert!(
            err.contains("Major version must start with 'v'"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_not_integer() {
        let err = validate_segment(1, "x.core.events.event.vX~", false).unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_leading_zeros() {
        let err = validate_segment(1, "x.core.events.event.v01~", false).unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_invalid_vendor_token() {
        let err = validate_segment(1, "1bad.core.events.event.v1~", false).unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    // ---- expected_format ----

    #[test]
    fn test_segment1_format_has_gts_prefix() {
        let err = validate_segment(1, "x.core.events.event~", false).unwrap_err();
        assert!(
            err.contains("gts.vendor.package.namespace.type.vMAJOR"),
            "segment #1 format should include gts. prefix, got: {err}"
        );
    }

    #[test]
    fn test_segment2_format_no_gts_prefix() {
        let err = validate_segment(2, "x.core.events.event~", false).unwrap_err();
        assert!(
            !err.contains("gts.vendor"),
            "segment #2 format should NOT include gts. prefix, got: {err}"
        );
        assert!(
            err.contains("vendor.package.namespace.type.vMAJOR"),
            "segment #2 should show vendor.package format, got: {err}"
        );
    }

    // ---- wildcards ----

    #[test]
    fn test_wildcard_at_vendor() {
        let parsed = validate_segment(1, "*", true).unwrap();
        assert!(parsed.is_wildcard);
    }

    #[test]
    fn test_wildcard_at_package() {
        let parsed = validate_segment(1, "x.*", true).unwrap();
        assert!(parsed.is_wildcard);
        assert_eq!(parsed.vendor, "x");
    }

    #[test]
    fn test_wildcard_invalid_token_before_star() {
        // Tokens before '*' must still be validated
        let err = validate_segment(1, "1bad.*", true).unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    #[test]
    fn test_wildcard_in_middle_rejected() {
        // '*' in a non-final position must be rejected
        let err = validate_segment(1, "x.*.ns.type.v1", true).unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_wildcard_at_version_position_not_final() {
        // '*' at version position (4) with extra token after it must be rejected
        let err = validate_segment(1, "x.pkg.ns.type.*.extra", true).unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_wildcard_rejected_without_flag() {
        let err = validate_segment(1, "x.*", false).unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    // ---- validate_gts_id ----

    #[test]
    fn test_valid_gts_id() {
        let segments = validate_gts_id("gts.x.core.events.event.v1~", false).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].vendor, "x");
        assert!(segments[0].is_type);
    }

    #[test]
    fn test_valid_gts_id_chained() {
        let segments = validate_gts_id(
            "gts.x.core.events.type.v1~vendor.app._.custom_event.v1~",
            false,
        )
        .unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].vendor, "x");
        assert_eq!(segments[1].vendor, "vendor");
    }

    #[test]
    fn test_gts_id_missing_prefix() {
        let err = validate_gts_id("x.core.events.event.v1~", false).unwrap_err();
        match err {
            GtsIdError::Id { cause, .. } => {
                assert!(cause.contains("must start with 'gts.'"), "got: {cause}");
            }
            GtsIdError::Segment { .. } => panic!("expected Id error, got: {err}"),
        }
    }

    #[test]
    fn test_gts_id_uppercase() {
        let err = validate_gts_id("gts.X.core.events.event.v1~", false).unwrap_err();
        match err {
            GtsIdError::Id { cause, .. } => {
                assert!(cause.contains("lowercase"), "got: {cause}");
            }
            GtsIdError::Segment { .. } => panic!("expected Id error, got: {err}"),
        }
    }

    #[test]
    fn test_gts_id_hyphen() {
        let err = validate_gts_id("gts.x-vendor.core.events.event.v1~", false).unwrap_err();
        match err {
            GtsIdError::Id { cause, .. } => {
                assert!(cause.contains("'-'"), "got: {cause}");
            }
            GtsIdError::Segment { .. } => panic!("expected Id error, got: {err}"),
        }
    }

    #[test]
    fn test_gts_id_segment_error_carries_num_and_offset() {
        let err = validate_gts_id(
            "gts.x.core.modkit.plugin.v1~x.core.license_enforcer.integration.plugin.v1~",
            false,
        )
        .unwrap_err();
        match err {
            GtsIdError::Segment {
                num, offset, cause, ..
            } => {
                assert_eq!(num, 2);
                // offset = "gts.".len() + "x.core.modkit.plugin.v1~".len() = 4 + 24 = 28
                assert_eq!(offset, 28);
                assert!(
                    cause.contains("Too many name tokens before version"),
                    "got: {cause}"
                );
            }
            GtsIdError::Id { .. } => panic!("expected Segment error, got: {err}"),
        }
    }

    #[test]
    fn test_gts_id_instance_no_tilde_end() {
        let segments = validate_gts_id("gts.x.core.events.event.v1~a.b.c.d.v1.0", false).unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments[0].is_type);
        assert!(!segments[1].is_type);
    }

    #[test]
    fn test_gts_id_whitespace_trimmed() {
        let segments = validate_gts_id("  gts.x.core.events.event.v1~  ", false).unwrap();
        assert_eq!(segments.len(), 1);
    }
}
