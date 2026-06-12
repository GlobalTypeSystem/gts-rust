//! A single parsed segment of a GTS identifier.
//!
//! [`GtsIdSegment`] is the structured view of one segment (the part between
//! `~` markers). It is a sum type: a segment is either a [concrete]
//! vendor.package.namespace.type.version segment, a [wildcard] pattern
//! segment, or a trailing anonymous-instance [UUID]. Segments are produced by
//! the parser ([`GtsIdSegment::parse`]); callers obtain them by parsing a full
//! [`GtsId`](crate::GtsId) or [`GtsIdWildcard`](crate::GtsIdWildcard), never by
//! constructing one directly — the variants and their fields are private so the
//! parser's invariants (validated tokens, canonical versions, well-formed UUID
//! tails) always hold.
//!
//! [concrete]: GtsIdSegment::Concrete
//! [wildcard]: GtsIdSegment::Wildcard
//! [UUID]: GtsIdSegment::UuidTail

use crate::parse::{expected_format, is_valid_segment_token, parse_u32_exact};

/// The parsed name and version components shared by concrete and wildcard
/// segments.
///
/// For a [`Wildcard`](GtsIdSegment::Wildcard) segment these fields hold the
/// (possibly partial) prefix that precedes the `*` token — e.g. `x.core.*`
/// fills `vendor` and `package` and leaves the rest empty. Empty strings, a
/// zero `ver_major`, and a `None` `ver_minor` therefore mean "unspecified" in
/// the wildcard case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsIdSegmentParts {
    /// The raw segment string as it appeared in the source (including any
    /// trailing `~` for a type segment).
    raw: String,
    vendor: String,
    package: String,
    namespace: String,
    type_name: String,
    ver_major: u32,
    ver_minor: Option<u32>,
    /// `true` when the segment ended with a `~` type marker.
    is_type: bool,
}

/// A single `~`-delimited segment of a parsed GTS identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GtsIdSegment {
    /// A concrete vendor.package.namespace.type.version segment.
    Concrete(GtsIdSegmentParts),
    /// A wildcard pattern segment: a (possibly empty) prefix followed by `*`.
    /// Only produced when parsing with wildcards enabled.
    Wildcard(GtsIdSegmentParts),
    /// A trailing anonymous-instance UUID (combined anonymous instance). The
    /// string is guaranteed to be a well-formed lowercase UUID.
    UuidTail(String),
}

impl GtsIdSegment {
    /// The parsed name/version parts, when this is a concrete or wildcard
    /// segment. `None` for a [`UuidTail`](Self::UuidTail).
    #[must_use]
    fn parts(&self) -> Option<&GtsIdSegmentParts> {
        match self {
            GtsIdSegment::Concrete(p) | GtsIdSegment::Wildcard(p) => Some(p),
            GtsIdSegment::UuidTail(_) => None,
        }
    }

    /// The raw segment string as it appeared in the source.
    ///
    /// For a concrete or wildcard segment this includes any trailing `~`; for a
    /// UUID tail it is the UUID itself.
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            GtsIdSegment::Concrete(p) | GtsIdSegment::Wildcard(p) => &p.raw,
            GtsIdSegment::UuidTail(uuid) => uuid,
        }
    }

    /// The vendor token, or `""` for a wildcard segment that doesn't reach the
    /// vendor position or for a UUID tail.
    #[must_use]
    pub fn vendor(&self) -> &str {
        self.parts().map_or("", |p| &p.vendor)
    }

    /// The package token, or `""` when unspecified.
    #[must_use]
    pub fn package(&self) -> &str {
        self.parts().map_or("", |p| &p.package)
    }

    /// The namespace token, or `""` when unspecified.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.parts().map_or("", |p| &p.namespace)
    }

    /// The type token, or `""` when unspecified.
    #[must_use]
    pub fn type_name(&self) -> &str {
        self.parts().map_or("", |p| &p.type_name)
    }

    /// The major version, or `0` when unspecified (wildcard / UUID tail).
    #[must_use]
    pub fn ver_major(&self) -> u32 {
        self.parts().map_or(0, |p| p.ver_major)
    }

    /// The minor version, when present.
    #[must_use]
    pub fn ver_minor(&self) -> Option<u32> {
        self.parts().and_then(|p| p.ver_minor)
    }

    /// `true` when the segment is a type/schema definition (ended with `~`).
    #[must_use]
    pub fn is_type(&self) -> bool {
        self.parts().is_some_and(|p| p.is_type)
    }

    /// `true` when this is a [`Wildcard`](Self::Wildcard) segment.
    #[must_use]
    pub fn is_wildcard(&self) -> bool {
        matches!(self, GtsIdSegment::Wildcard(_))
    }

    /// The UUID string when this is a [`UuidTail`](Self::UuidTail), else `None`.
    #[must_use]
    pub fn uuid_tail(&self) -> Option<&str> {
        match self {
            GtsIdSegment::UuidTail(uuid) => Some(uuid),
            _ => None,
        }
    }

    /// The deterministic UUID parsed from a [`UuidTail`](Self::UuidTail) segment.
    ///
    /// Returns `None` for any other variant. The stored string was validated as
    /// a well-formed UUID when the segment was parsed, so this never fails for a
    /// UUID tail.
    ///
    /// Requires the `uuid` feature.
    #[cfg(feature = "uuid")]
    #[must_use]
    pub fn uuid(&self) -> Option<uuid::Uuid> {
        self.uuid_tail().and_then(|s| uuid::Uuid::parse_str(s).ok())
    }

    /// Construct a [`UuidTail`](Self::UuidTail) segment from an already-validated
    /// UUID string.
    pub(crate) fn uuid_tail_segment(uuid: &str) -> Self {
        GtsIdSegment::UuidTail(uuid.to_owned())
    }

    /// Parse a single GTS segment (the part between `~` markers).
    ///
    /// This is the sole constructor for concrete and wildcard segments: the
    /// only way to obtain one is through validated parsing, so the variants'
    /// invariants always hold.
    ///
    /// # Arguments
    /// * `num` - 1-based segment number (used in error messages and format hints)
    /// * `segment` - The raw segment string, possibly including a trailing `~`
    /// * `allow_wildcards` - If `true`, a trailing wildcard `*` token is accepted as the final token
    ///
    /// # Errors
    /// Returns a human-readable error message if the segment is invalid.
    pub(crate) fn parse(num: usize, segment: &str, allow_wildcards: bool) -> Result<Self, String> {
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
        let fmt = expected_format(num);

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

        // Build the parts, parsing tokens progressively. A `*` token at any
        // position turns the segment into a wildcard and ends parsing there.
        let mut parts = GtsIdSegmentParts {
            raw: segment.to_owned(),
            vendor: String::new(),
            package: String::new(),
            namespace: String::new(),
            type_name: String::new(),
            ver_major: 0,
            ver_minor: None,
            is_type,
        };

        if !tokens.is_empty() {
            if allow_wildcards && tokens[0] == "*" {
                return Ok(GtsIdSegment::Wildcard(parts));
            }
            tokens[0].clone_into(&mut parts.vendor);
        }

        if tokens.len() > 1 {
            if allow_wildcards && tokens[1] == "*" {
                return Ok(GtsIdSegment::Wildcard(parts));
            }
            tokens[1].clone_into(&mut parts.package);
        }

        if tokens.len() > 2 {
            if allow_wildcards && tokens[2] == "*" {
                return Ok(GtsIdSegment::Wildcard(parts));
            }
            tokens[2].clone_into(&mut parts.namespace);
        }

        if tokens.len() > 3 {
            if allow_wildcards && tokens[3] == "*" {
                return Ok(GtsIdSegment::Wildcard(parts));
            }
            tokens[3].clone_into(&mut parts.type_name);
        }

        if tokens.len() > 4 {
            if allow_wildcards && tokens[4] == "*" {
                if 4 != tokens.len() - 1 {
                    return Err("Wildcard '*' is only allowed as the final token".to_owned());
                }
                return Ok(GtsIdSegment::Wildcard(parts));
            }

            if !tokens[4].starts_with('v') {
                return Err("Major version must start with 'v'".to_owned());
            }

            let major_str = &tokens[4][1..];
            parts.ver_major = parse_u32_exact(major_str)
                .ok_or_else(|| format!("Major version must be an integer, got '{major_str}'"))?;
        }

        if tokens.len() > 5 {
            if allow_wildcards && tokens[5] == "*" {
                return Ok(GtsIdSegment::Wildcard(parts));
            }

            parts.ver_minor =
                Some(parse_u32_exact(tokens[5]).ok_or_else(|| {
                    format!("Minor version must be an integer, got '{}'", tokens[5])
                })?);
        }

        Ok(GtsIdSegment::Concrete(parts))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_segment_basic() {
        let parsed = GtsIdSegment::parse(1, "x.core.events.event.v1~", false).unwrap();
        assert_eq!(parsed.vendor(), "x");
        assert_eq!(parsed.package(), "core");
        assert_eq!(parsed.namespace(), "events");
        assert_eq!(parsed.type_name(), "event");
        assert_eq!(parsed.ver_major(), 1);
        assert_eq!(parsed.ver_minor(), None);
        assert!(parsed.is_type());
        assert!(!parsed.is_wildcard());
    }

    #[test]
    fn test_valid_segment_with_minor() {
        let parsed = GtsIdSegment::parse(1, "x.core.events.event.v1.2~", false).unwrap();
        assert_eq!(parsed.ver_major(), 1);
        assert_eq!(parsed.ver_minor(), Some(2));
    }

    #[test]
    fn test_segment_too_many_tildes() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v1~~", false).unwrap_err();
        assert!(err.contains("Too many '~' characters"), "got: {err}");
    }

    #[test]
    fn test_segment_tilde_not_at_end() {
        let err = GtsIdSegment::parse(1, "x.core~mid.events.event.v1", false).unwrap_err();
        assert!(err.contains("'~' must be at the end"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_tokens() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v1.2.extra~", false).unwrap_err();
        assert!(err.contains("Too many tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_few_tokens() {
        let err = GtsIdSegment::parse(1, "x.core.events.event~", false).unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_name_tokens() {
        let err = GtsIdSegment::parse(2, "x.core.ns.type.extra.v1~", false).unwrap_err();
        assert!(
            err.contains("Too many name tokens before version"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_without_v() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.1~", false).unwrap_err();
        assert!(
            err.contains("Major version must start with 'v'"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_not_integer() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.vX~", false).unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_leading_zeros() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v01~", false).unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_invalid_vendor_token() {
        let err = GtsIdSegment::parse(1, "1bad.core.events.event.v1~", false).unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    // ---- expected_format (surfaced through segment parsing) ----

    #[test]
    fn test_segment1_format_has_gts_prefix() {
        let err = GtsIdSegment::parse(1, "x.core.events.event~", false).unwrap_err();
        assert!(
            err.contains("gts.vendor.package.namespace.type.vMAJOR"),
            "segment #1 format should include gts. prefix, got: {err}"
        );
    }

    #[test]
    fn test_segment2_format_no_gts_prefix() {
        let err = GtsIdSegment::parse(2, "x.core.events.event~", false).unwrap_err();
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
        let parsed = GtsIdSegment::parse(1, "*", true).unwrap();
        assert!(parsed.is_wildcard());
    }

    #[test]
    fn test_wildcard_at_package() {
        let parsed = GtsIdSegment::parse(1, "x.*", true).unwrap();
        assert!(parsed.is_wildcard());
        assert_eq!(parsed.vendor(), "x");
    }

    #[test]
    fn test_wildcard_invalid_token_before_star() {
        // Tokens before '*' must still be validated
        let err = GtsIdSegment::parse(1, "1bad.*", true).unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    #[test]
    fn test_wildcard_in_middle_rejected() {
        // '*' in a non-final position must be rejected
        let err = GtsIdSegment::parse(1, "x.*.ns.type.v1", true).unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_wildcard_at_version_position_not_final() {
        // '*' at version position (4) with extra token after it must be rejected
        let err = GtsIdSegment::parse(1, "x.pkg.ns.type.*.extra", true).unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_wildcard_rejected_without_flag() {
        let err = GtsIdSegment::parse(1, "x.*", false).unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    // ---- UUID tail ----

    #[test]
    fn test_uuid_tail_segment_accessors() {
        let seg = GtsIdSegment::uuid_tail_segment("7a1d2f34-5678-49ab-9012-abcdef123456");
        assert_eq!(
            seg.uuid_tail(),
            Some("7a1d2f34-5678-49ab-9012-abcdef123456")
        );
        assert_eq!(seg.raw(), "7a1d2f34-5678-49ab-9012-abcdef123456");
        assert!(!seg.is_wildcard());
        assert!(!seg.is_type());
        assert_eq!(seg.vendor(), "");
        assert_eq!(seg.ver_major(), 0);
        assert_eq!(seg.ver_minor(), None);
    }

    #[test]
    fn test_concrete_and_wildcard_have_no_uuid_tail() {
        let concrete = GtsIdSegment::parse(1, "x.core.events.event.v1~", false).unwrap();
        assert_eq!(concrete.uuid_tail(), None);
        let wildcard = GtsIdSegment::parse(1, "x.*", true).unwrap();
        assert_eq!(wildcard.uuid_tail(), None);
    }
}
