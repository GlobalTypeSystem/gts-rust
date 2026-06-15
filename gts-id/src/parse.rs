//! Low-level parsing of GTS identifier strings into structured segments.
//!
//! Following the "parse, don't validate" principle, these functions don't just
//! check validity — they produce a structured [`GtsIdSegment`] (or a `Vec` of
//! them) from the raw string. Callers that only care about validity simply
//! inspect the `Result` and discard the parsed value.

use crate::{GtsIdError, GtsIdPatternSegment, GtsIdSegment};

/// The required prefix for all GTS identifiers.
pub const GTS_PREFIX: &str = "gts.";

/// Maximum allowed length for a GTS identifier string.
pub const GTS_MAX_LENGTH: usize = 1024;

/// Expected format string for segment error messages.
///
/// Segment #1 shows the `gts.` prefix because the user writes
/// `gts.vendor.package...`; segments #2+ omit it because they
/// come after a `~` delimiter.
#[must_use]
pub fn expected_format(segment_num: usize) -> &'static str {
    if segment_num == 1 {
        "gts.vendor.package.namespace.type.vMAJOR[.MINOR]"
    } else {
        "vendor.package.namespace.type.vMAJOR[.MINOR]"
    }
}

/// Checks whether a string matches the UUID format
/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` (hex digits and dashes only).
#[inline]
#[must_use]
pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
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

/// Validate the shared GTS string structure and split it into raw segment
/// strings plus an optional trailing UUID.
///
/// This is the shared structural pass behind [`parse_id`] and [`parse_pattern`];
/// per-segment parsing and the issue #37 single-segment rule live in those.
///
/// # Errors
/// Returns [`GtsIdError`] on structural failure.
fn split_raw_segments(
    id: &str,
    allow_wildcards: bool,
) -> Result<(Vec<String>, Option<String>), GtsIdError> {
    let raw = id.trim();

    if !raw.starts_with(GTS_PREFIX) {
        return Err(GtsIdError::new(
            id,
            format!("must start with '{GTS_PREFIX}'"),
        ));
    }

    if raw != raw.to_lowercase() {
        return Err(GtsIdError::new(id, "must be lowercase"));
    }

    if raw.len() > GTS_MAX_LENGTH {
        return Err(GtsIdError::new(
            id,
            format!("too long ({} chars, max {GTS_MAX_LENGTH})", raw.len()),
        ));
    }

    if allow_wildcards {
        let wildcards_num = raw.matches('*').count();
        if wildcards_num > 1 {
            return Err(GtsIdError::new(
                id,
                "The wildcard '*' token is allowed only once",
            ));
        }
        if wildcards_num > 0 && !raw.ends_with('*') {
            return Err(GtsIdError::new(
                id,
                "The wildcard '*' token is allowed only at the end of the pattern",
            ));
        }
    }

    let remainder = &raw[GTS_PREFIX.len()..];
    let tilde_parts: Vec<&str> = remainder.split('~').collect();

    // Detect combined anonymous instance: last tilde-part is a UUID.
    let uuid_tail: Option<&str> = {
        let last = tilde_parts.last().copied().unwrap_or("");
        if is_uuid(last) && tilde_parts.len() >= 2 {
            Some(last)
        } else {
            None
        }
    };

    // Reject hyphens in the GTS segments portion (hyphens are only allowed in the UUID tail).
    let segments_portion = match uuid_tail {
        Some(uuid) => &raw[..raw.len() - uuid.len() - 1], // strip "~<uuid>"
        None => raw,
    };
    if segments_portion.contains('-') {
        return Err(GtsIdError::new(id, "must not contain '-'"));
    }

    // Build the list of raw segment strings, excluding the UUID tail.
    // When a UUID tail is present, every preceding tilde-part was followed by '~'
    // in the original string, so each is a type segment — append '~' to all of them.
    // Otherwise use the standard reconstruction (last part may or may not have '~').
    let seg_count = tilde_parts.len() - usize::from(uuid_tail.is_some());
    let mut segments_raw: Vec<String> = Vec::new();
    for (i, &part) in tilde_parts.iter().enumerate().take(seg_count) {
        let is_last = i == seg_count - 1;
        if part.is_empty() {
            // The only allowed empty part is the single trailing one produced by a
            // type-marker `~` at the end (e.g. "gts.v.p.n.t.v1~"). Any other empty
            // part means consecutive tildes (e.g. "~~") or a leading tilde, which
            // are invalid.
            if !(is_last && uuid_tail.is_none()) {
                return Err(GtsIdError::new(
                    id,
                    format!("empty segment at tilde-part #{}", i + 1),
                ));
            }
        } else if is_last && uuid_tail.is_none() {
            segments_raw.push(part.to_owned());
        } else {
            segments_raw.push(format!("{part}~"));
        }
    }

    if segments_raw.is_empty() {
        return Err(GtsIdError::new(id, "no segments found"));
    }

    Ok((segments_raw, uuid_tail.map(str::to_owned)))
}

/// Error returned when a single-segment instance id is encountered (issue #37).
fn single_segment_instance_error(id: &str) -> GtsIdError {
    GtsIdError::new(
        id,
        "Single-segment instance IDs are prohibited. Instance IDs must be chained with at least one type segment (e.g., 'type~instance')",
    )
}

/// Parse a **concrete** GTS identifier into its [`GtsIdSegment`]s.
///
/// Wildcards are rejected (a `*` is an invalid segment token). Also enforces
/// issue #37: a single-segment instance id is prohibited (an instance must be
/// chained with at least one type segment); a trailing UUID tail is exempt.
///
/// This backs [`GtsId::try_new`](crate::GtsId::try_new). On failure the returned
/// [`GtsIdError`] carries the 1-based number and byte offset of the offending
/// segment within `id`.
///
/// # Errors
/// Returns [`GtsIdError`] on parse failure or invariant violation.
pub fn parse_id(id: &str) -> Result<Vec<GtsIdSegment>, GtsIdError> {
    let (segments_raw, uuid_tail) = split_raw_segments(id, false)?;

    let mut segments = Vec::new();
    let mut offset = GTS_PREFIX.len();
    for (i, seg) in segments_raw.iter().enumerate() {
        let parsed = GtsIdSegment::parse(i + 1, seg)
            .map_err(|cause| GtsIdError::new(id, cause).with_segment(i + 1, offset, seg.clone()))?;
        offset += seg.len();
        segments.push(parsed);
    }

    if let Some(ref uuid) = uuid_tail {
        segments.push(GtsIdSegment::uuid_tail_segment(uuid));
    }

    // Issue #37: a concrete id has no wildcard exemption.
    if uuid_tail.is_none() && segments.len() == 1 && !segments[0].is_type() {
        return Err(single_segment_instance_error(id));
    }

    Ok(segments)
}

/// Parse a GTS identifier **pattern** into its [`GtsIdPatternSegment`]s.
///
/// A trailing `*` wildcard is accepted. Also enforces issue #37, with wildcard
/// segments (and UUID tails) exempt from the single-segment rule.
///
/// This backs [`GtsIdPattern::try_new`](crate::GtsIdPattern::try_new). On failure
/// the returned [`GtsIdError`] carries the 1-based number and byte offset of the
/// offending segment within `id`.
///
/// # Errors
/// Returns [`GtsIdError`] on parse failure or invariant violation.
pub fn parse_pattern(id: &str) -> Result<Vec<GtsIdPatternSegment>, GtsIdError> {
    let (segments_raw, uuid_tail) = split_raw_segments(id, true)?;

    let mut segments = Vec::new();
    let mut offset = GTS_PREFIX.len();
    for (i, seg) in segments_raw.iter().enumerate() {
        let parsed = GtsIdPatternSegment::parse(i + 1, seg)
            .map_err(|cause| GtsIdError::new(id, cause).with_segment(i + 1, offset, seg.clone()))?;
        offset += seg.len();
        segments.push(parsed);
    }

    if let Some(ref uuid) = uuid_tail {
        segments.push(GtsIdPatternSegment::uuid_tail_segment(uuid));
    }

    // Issue #37: wildcard segments are exempt.
    if uuid_tail.is_none()
        && segments.len() == 1
        && !segments[0].is_type()
        && !segments[0].is_wildcard()
    {
        return Err(single_segment_instance_error(id));
    }

    Ok(segments)
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

    // ---- parse_id ----

    #[test]
    fn test_valid_gts_id() {
        let segments = parse_id("gts.x.core.events.event.v1~").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].vendor(), "x");
        assert!(segments[0].is_type());
    }

    #[test]
    fn test_valid_gts_id_chained() {
        let segments = parse_id("gts.x.core.events.type.v1~vendor.app._.custom_event.v1~").unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].vendor(), "x");
        assert_eq!(segments[1].vendor(), "vendor");
    }

    #[test]
    fn test_gts_id_missing_prefix() {
        let err = parse_id("x.core.events.event.v1~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("must start with 'gts.'"), "got: {err}");
    }

    #[test]
    fn test_gts_id_uppercase() {
        let err = parse_id("gts.X.core.events.event.v1~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("lowercase"), "got: {err}");
    }

    #[test]
    fn test_gts_id_hyphen() {
        let err = parse_id("gts.x-vendor.core.events.event.v1~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("'-'"), "got: {err}");
    }

    #[test]
    fn test_gts_id_segment_error_carries_num_and_offset() {
        let err =
            parse_id("gts.x.core.modkit.plugin.v1~x.core.license_enforcer.integration.plugin.v1~")
                .unwrap_err();
        let seg = err.segment.as_ref().expect("expected segment-level error");
        assert_eq!(seg.num, 2);
        // offset = "gts.".len() + "x.core.modkit.plugin.v1~".len() = 4 + 24 = 28
        assert_eq!(seg.offset, 28);
        assert!(
            err.cause.contains("Too many name tokens before version"),
            "got: {err}"
        );
    }

    #[test]
    fn test_gts_id_instance_no_tilde_end() {
        let segments = parse_id("gts.x.core.events.event.v1~a.b.c.d.v1.0").unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments[0].is_type());
        assert!(!segments[1].is_type());
    }

    #[test]
    fn test_gts_id_double_tilde_rejected() {
        let err = parse_id("gts.x.test1.events.type.v1.0~~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("empty segment"), "got: {err}");
    }

    #[test]
    fn test_gts_id_whitespace_trimmed() {
        let segments = parse_id("  gts.x.core.events.event.v1~  ").unwrap();
        assert_eq!(segments.len(), 1);
    }

    // ---- is_uuid ----

    #[test]
    fn test_is_uuid_valid() {
        assert!(is_uuid("7a1d2f34-5678-49ab-9012-abcdef123456"));
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(is_uuid("ffffffff-ffff-ffff-ffff-ffffffffffff"));
    }

    #[test]
    fn test_is_uuid_invalid() {
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("7a1d2f34-5678-49ab-9012-abcdef12345")); // too short
        assert!(!is_uuid("7a1d2f34-5678-49ab-9012-abcdef1234567")); // too long
        assert!(!is_uuid("7a1d2f34-5678-49ab-9012-abcdef12345g")); // non-hex char
        assert!(!is_uuid("7a1d2f3405678-49ab-9012-abcdef123456")); // dash in wrong place
    }

    // ---- combined anonymous instance ----

    #[test]
    fn test_combined_anonymous_instance_valid() {
        let segments = parse_id("gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~7a1d2f34-5678-49ab-9012-abcdef123456")
        .unwrap();
        assert_eq!(segments.len(), 3);
        assert!(segments[0].is_type());
        assert!(segments[1].is_type());
        assert!(segments[2].uuid_tail().is_some());
        assert!(!segments[2].is_type());
        assert_eq!(segments[2].raw(), "7a1d2f34-5678-49ab-9012-abcdef123456");
    }

    #[test]
    fn test_combined_anonymous_instance_single_prefix_valid() {
        let segments =
            parse_id("gts.x.core.events.type.v1~7a1d2f34-5678-49ab-9012-abcdef123456").unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments[0].is_type());
        assert!(segments[1].uuid_tail().is_some());
    }

    #[test]
    fn test_combined_anonymous_instance_hyphen_in_segments_rejected() {
        let err = parse_id("gts.x-vendor.core.events.type.v1~x.commerce.orders.order_placed.v1.0~7a1d2f34-5678-49ab-9012-abcdef123456")
        .unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("'-'"), "got: {err}");
    }

    #[test]
    fn test_uuid_alone_without_prefix_rejected() {
        // A bare UUID with no GTS prefix is not a valid GTS ID
        let err = parse_id("7a1d2f34-5678-49ab-9012-abcdef123456").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("must start with 'gts.'"), "got: {err}");
    }

    #[test]
    fn test_uuid_tail_without_preceding_tilde_rejected() {
        // UUID as the only segment (no preceding ~) must be rejected
        // "gts." + UUID has no tilde_parts.len() >= 2
        let err = parse_id("gts.7a1d2f34-5678-49ab-9012-abcdef123456").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("'-'"), "got: {err}");
    }

    // ---- issue #37: single-segment instance prohibition ----

    #[test]
    fn test_single_segment_instance_rejected() {
        // A lone instance segment (no '~', not a wildcard) is prohibited by #37.
        let err = parse_id("gts.x.pkg.ns.type.v1.0").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("Single-segment instance"), "got: {err}");
    }

    #[test]
    fn test_single_segment_wildcard_allowed() {
        // Wildcards are exempt from #37, so "gts.a.b.*" is accepted.
        let segments = parse_pattern("gts.a.b.*").unwrap();
        assert_eq!(segments.len(), 1);
        assert!(segments[0].is_wildcard());
    }

    // ---- wildcard placement rules live in the parser (id-level errors) ----

    #[test]
    fn test_parse_pattern_multistar_rejected() {
        let err = parse_pattern("gts.*.*.*.*").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("only once"), "got: {err}");
    }

    #[test]
    fn test_parse_pattern_star_not_at_end_rejected() {
        let err = parse_pattern("gts.*.core.events.event.v1~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("only at the end"), "got: {err}");
    }

    #[test]
    fn test_parse_pattern_version_wildcard_accepted() {
        // `v*` ends the string with `*`, so the structural gate admits it; the
        // segment parser turns it into a single wildcard segment.
        let segments = parse_pattern("gts.x.llm.chat.message.v*").unwrap();
        assert_eq!(segments.len(), 1);
        assert!(segments[0].is_wildcard());
    }

    #[test]
    fn test_parse_pattern_star_then_tilde_rejected() {
        // `…*~` does not end with `*`, so the gate rejects it id-level rather
        // than the segment parser silently stripping the trailing `~`.
        let err = parse_pattern("gts.x.llm.chat.message.v1.*~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("only at the end"), "got: {err}");
    }

    #[test]
    fn test_parse_pattern_midchain_wildcard_rejected() {
        // A wildcard that is the final token of a non-final chain segment is only
        // catchable structurally — the per-segment parser sees it as valid.
        let err = parse_pattern("gts.x.*~a.b.c.d.v1~").unwrap_err();
        assert!(err.segment.is_none(), "expected id-level error, got: {err}");
        assert!(err.cause.contains("only at the end"), "got: {err}");
    }

    #[test]
    fn test_parse_pattern_wildcard_rules_off_without_flag() {
        // With wildcards disabled, '*' is just an invalid segment token,
        // reported as a segment-level error.
        let err = parse_id("gts.*.*.*.*").unwrap_err();
        assert!(
            err.segment.is_some(),
            "expected segment-level error, got: {err}"
        );
    }
}
