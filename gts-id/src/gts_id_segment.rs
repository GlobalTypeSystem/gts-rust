//! Parsed segments of a GTS identifier.
//!
//! A segment is the part between `~` markers. There are two segment types:
//!
//! * [`GtsIdSegment`] — a **concrete** segment (a
//!   `vendor.package.namespace.type.version` segment or a trailing
//!   anonymous-instance UUID). This is what a concrete [`GtsId`](crate::GtsId)
//!   is made of, so it has no notion of a wildcard.
//! * [`GtsIdPatternSegment`] — what a [`GtsIdPattern`](crate::GtsIdPattern) is
//!   made of: either a concrete [`GtsIdSegment`] or a trailing `*` wildcard.
//!
//! Both are enums whose variant payloads ([`GtsIdSegmentParts`], [`GtsUuidTail`])
//! have no public constructors, so the only way to obtain a segment is through
//! validated parsing: the parser's invariants (validated tokens, canonical
//! versions, well-formed UUID tails) always hold and cannot be forged by
//! downstream crates. Inspect a segment through its accessor methods or by
//! matching.

use crate::parse::{expected_format, is_valid_segment_token, parse_u32_exact};

/// The parsed name and version components shared by concrete and wildcard
/// segments.
///
/// For a wildcard segment these fields hold the (possibly partial) prefix that
/// precedes the `*` token — e.g. `x.core.*` fills `vendor` and `package` and
/// leaves the rest empty. Empty strings, a zero `ver_major`, and a `None`
/// `ver_minor` therefore mean "unspecified" in the wildcard case.
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
}

impl GtsIdSegmentParts {
    /// The raw segment string as it appeared in the source.
    ///
    /// This includes any trailing `~` for a type segment.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The vendor token, or `""` when unspecified in a wildcard segment.
    #[must_use]
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// The package token, or `""` when unspecified in a wildcard segment.
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    /// The namespace token, or `""` when unspecified in a wildcard segment.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The type token, or `""` when unspecified in a wildcard segment.
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The major version, or `0` when unspecified in a wildcard segment.
    #[must_use]
    pub fn ver_major(&self) -> u32 {
        self.ver_major
    }

    /// The minor version, when present.
    #[must_use]
    pub fn ver_minor(&self) -> Option<u32> {
        self.ver_minor
    }

    /// `true` when the segment is a type definition (ended with `~`).
    #[must_use]
    pub fn is_type(&self) -> bool {
        self.raw.ends_with('~')
    }
}

/// A read-only view of a segment's parsed fields.
///
/// Implemented by both [`GtsIdSegment`] and [`GtsIdPatternSegment`] so the
/// pattern matcher can compare a pattern against a candidate of either kind
/// (a concrete id or another pattern) with the same field-level logic.
pub trait SegmentView {
    fn vendor(&self) -> &str;
    fn package(&self) -> &str;
    fn namespace(&self) -> &str;
    fn type_name(&self) -> &str;
    fn ver_major(&self) -> u32;
    fn ver_minor(&self) -> Option<u32>;
    fn is_type(&self) -> bool;
    fn uuid_tail(&self) -> Option<&str>;
}

/// A trailing anonymous-instance UUID (combined anonymous instance).
///
/// Opaque: the inner string is guaranteed to be a well-formed lowercase UUID
/// because the only constructor is the validated parser. Downstream crates can
/// match on [`GtsIdSegment::UuidTail`] but cannot forge one with a non-UUID
/// string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsUuidTail(String);

impl GtsUuidTail {
    /// The UUID string (a well-formed lowercase UUID).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The parsed UUID. The string was validated at parse time, so this is
    /// `Some` for any value obtained from the parser.
    ///
    /// Requires the `uuid` feature.
    #[cfg(feature = "uuid")]
    #[must_use]
    pub fn uuid(&self) -> Option<uuid::Uuid> {
        uuid::Uuid::parse_str(&self.0).ok()
    }
}

/// A single concrete `~`-delimited segment of a parsed GTS identifier.
///
/// A concrete segment is either a `vendor.package.namespace.type.version`
/// segment or a trailing anonymous-instance UUID — never a wildcard. This is
/// what [`GtsId`](crate::GtsId) is composed of.
///
/// Both payloads ([`GtsIdSegmentParts`] and [`GtsUuidTail`]) are unconstructable
/// outside this crate, so a segment can only be produced by the validated
/// parser and its invariants always hold. Inspect a segment through its accessor
/// methods or by matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GtsIdSegment {
    /// A concrete vendor.package.namespace.type.version segment.
    Concrete(GtsIdSegmentParts),
    /// A trailing anonymous-instance UUID.
    UuidTail(GtsUuidTail),
}

impl GtsIdSegment {
    /// The parsed name/version parts, or `None` for a UUID-tail segment.
    #[must_use]
    fn parts(&self) -> Option<&GtsIdSegmentParts> {
        match self {
            GtsIdSegment::Concrete(p) => Some(p),
            GtsIdSegment::UuidTail(_) => None,
        }
    }

    /// The raw segment string as it appeared in the source.
    ///
    /// For a concrete segment this includes any trailing `~`; for a UUID tail it
    /// is the UUID itself.
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            GtsIdSegment::Concrete(p) => &p.raw,
            GtsIdSegment::UuidTail(uuid) => uuid.as_str(),
        }
    }

    /// The vendor token, or `""` for a UUID tail.
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

    /// The major version, or `0` for a UUID tail.
    #[must_use]
    pub fn ver_major(&self) -> u32 {
        self.parts().map_or(0, |p| p.ver_major)
    }

    /// The minor version, when present.
    #[must_use]
    pub fn ver_minor(&self) -> Option<u32> {
        self.parts().and_then(|p| p.ver_minor)
    }

    /// `true` when the segment is a type definition (ended with `~`).
    #[must_use]
    pub fn is_type(&self) -> bool {
        // Derived from the raw string: a type segment ends with a `~` marker (a
        // UUID tail never does).
        self.raw().ends_with('~')
    }

    /// The UUID string when this is a UUID-tail segment, else `None`.
    #[must_use]
    pub fn uuid_tail(&self) -> Option<&str> {
        match self {
            GtsIdSegment::UuidTail(uuid) => Some(uuid.as_str()),
            GtsIdSegment::Concrete(_) => None,
        }
    }

    /// The deterministic UUID parsed from a UUID-tail segment.
    ///
    /// Returns `None` for any other segment kind. The stored string was
    /// validated as a well-formed UUID when the segment was parsed, so this
    /// never fails for a UUID tail.
    ///
    /// Requires the `uuid` feature.
    #[cfg(feature = "uuid")]
    #[must_use]
    pub fn uuid(&self) -> Option<uuid::Uuid> {
        self.uuid_tail().and_then(|s| uuid::Uuid::parse_str(s).ok())
    }

    /// Construct a UUID-tail segment from an already-validated UUID string.
    pub(crate) fn uuid_tail_segment(uuid: &str) -> Self {
        GtsIdSegment::UuidTail(GtsUuidTail(uuid.to_owned()))
    }

    /// Parse a single **concrete** GTS segment (the part between `~` markers).
    ///
    /// Wildcards are rejected: a concrete identifier never contains `*`. Use
    /// [`GtsIdPatternSegment::parse`] for pattern segments.
    ///
    /// # Arguments
    /// * `num` - 1-based segment number (used in error messages and format hints)
    /// * `segment` - The raw segment string, possibly including a trailing `~`
    ///
    /// # Errors
    /// Returns a human-readable error message if the segment is invalid.
    pub(crate) fn parse(num: usize, segment: &str) -> Result<Self, String> {
        // `allow_wildcards = false` guarantees a concrete result.
        let (parts, _is_wildcard) = parse_segment_parts(num, segment, false)?;
        Ok(GtsIdSegment::Concrete(parts))
    }
}

/// A single segment of a parsed GTS identifier **pattern**.
///
/// A pattern segment is either a concrete [`GtsIdSegment`] or a trailing `*`
/// wildcard carrying the (possibly partial) prefix that precedes the `*`. This
/// is what [`GtsIdPattern`](crate::GtsIdPattern) is composed of; the wildcard
/// can only ever be the final segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GtsIdPatternSegment {
    /// A concrete (non-wildcard) segment.
    Segment(GtsIdSegment),
    /// A trailing `*` wildcard with its (possibly partial) prefix constraints.
    Wildcard(GtsIdSegmentParts),
}

impl GtsIdPatternSegment {
    /// `true` when this is a wildcard segment.
    #[must_use]
    pub fn is_wildcard(&self) -> bool {
        matches!(self, GtsIdPatternSegment::Wildcard(_))
    }

    /// The raw segment string as it appeared in the source.
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            GtsIdPatternSegment::Segment(s) => s.raw(),
            GtsIdPatternSegment::Wildcard(p) => &p.raw,
        }
    }

    /// The vendor token, or `""` when unspecified.
    #[must_use]
    pub fn vendor(&self) -> &str {
        match self {
            GtsIdPatternSegment::Segment(s) => s.vendor(),
            GtsIdPatternSegment::Wildcard(p) => &p.vendor,
        }
    }

    /// The package token, or `""` when unspecified.
    #[must_use]
    pub fn package(&self) -> &str {
        match self {
            GtsIdPatternSegment::Segment(s) => s.package(),
            GtsIdPatternSegment::Wildcard(p) => &p.package,
        }
    }

    /// The namespace token, or `""` when unspecified.
    #[must_use]
    pub fn namespace(&self) -> &str {
        match self {
            GtsIdPatternSegment::Segment(s) => s.namespace(),
            GtsIdPatternSegment::Wildcard(p) => &p.namespace,
        }
    }

    /// The type token, or `""` when unspecified.
    #[must_use]
    pub fn type_name(&self) -> &str {
        match self {
            GtsIdPatternSegment::Segment(s) => s.type_name(),
            GtsIdPatternSegment::Wildcard(p) => &p.type_name,
        }
    }

    /// The major version, or `0` when unspecified.
    #[must_use]
    pub fn ver_major(&self) -> u32 {
        match self {
            GtsIdPatternSegment::Segment(s) => s.ver_major(),
            GtsIdPatternSegment::Wildcard(p) => p.ver_major,
        }
    }

    /// The minor version, when present.
    #[must_use]
    pub fn ver_minor(&self) -> Option<u32> {
        match self {
            GtsIdPatternSegment::Segment(s) => s.ver_minor(),
            GtsIdPatternSegment::Wildcard(p) => p.ver_minor,
        }
    }

    /// `true` when the segment is a type definition (ended with `~`).
    /// Always `false` for a wildcard segment (a wildcard never carries a `~`).
    #[must_use]
    pub fn is_type(&self) -> bool {
        self.raw().ends_with('~')
    }

    /// The UUID string when this wraps a UUID-tail segment, else `None`.
    #[must_use]
    pub fn uuid_tail(&self) -> Option<&str> {
        match self {
            GtsIdPatternSegment::Segment(s) => s.uuid_tail(),
            GtsIdPatternSegment::Wildcard(_) => None,
        }
    }

    /// Construct a UUID-tail pattern segment from an already-validated UUID.
    pub(crate) fn uuid_tail_segment(uuid: &str) -> Self {
        GtsIdPatternSegment::Segment(GtsIdSegment::uuid_tail_segment(uuid))
    }

    /// Parse a single GTS **pattern** segment (the part between `~` markers).
    ///
    /// A trailing `*` wildcard is accepted as the final token; otherwise the
    /// segment is concrete.
    ///
    /// # Arguments
    /// * `num` - 1-based segment number (used in error messages and format hints)
    /// * `segment` - The raw segment string, possibly including a trailing `~`
    ///
    /// # Errors
    /// Returns a human-readable error message if the segment is invalid.
    pub(crate) fn parse(num: usize, segment: &str) -> Result<Self, String> {
        let (parts, is_wildcard) = parse_segment_parts(num, segment, true)?;
        if is_wildcard {
            Ok(GtsIdPatternSegment::Wildcard(parts))
        } else {
            Ok(GtsIdPatternSegment::Segment(GtsIdSegment::Concrete(parts)))
        }
    }
}

/// Parse a segment's tokens into [`GtsIdSegmentParts`], reporting whether it is
/// a wildcard.
///
/// This is the shared token-validation logic backing both
/// [`GtsIdSegment::parse`] (`allow_wildcards = false`) and
/// [`GtsIdPatternSegment::parse`] (`allow_wildcards = true`). When
/// `allow_wildcards` is `false` the returned flag is always `false`.
fn parse_segment_parts(
    num: usize,
    segment: &str,
    allow_wildcards: bool,
) -> Result<(GtsIdSegmentParts, bool), String> {
    let mut seg = segment.to_owned();

    // Strip the type marker (~) for tokenization. It stays in `raw` (the
    // original `segment`), so `is_type` is derived from there — no stored flag.
    if seg.contains('~') {
        let tilde_count = seg.matches('~').count();
        if tilde_count > 1 {
            return Err("Too many '~' characters".to_owned());
        }
        if seg.ends_with('~') {
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
    };

    if !tokens.is_empty() {
        if allow_wildcards && tokens[0] == "*" {
            return Ok((parts, true));
        }
        tokens[0].clone_into(&mut parts.vendor);
    }

    if tokens.len() > 1 {
        if allow_wildcards && tokens[1] == "*" {
            return Ok((parts, true));
        }
        tokens[1].clone_into(&mut parts.package);
    }

    if tokens.len() > 2 {
        if allow_wildcards && tokens[2] == "*" {
            return Ok((parts, true));
        }
        tokens[2].clone_into(&mut parts.namespace);
    }

    if tokens.len() > 3 {
        if allow_wildcards && tokens[3] == "*" {
            return Ok((parts, true));
        }
        tokens[3].clone_into(&mut parts.type_name);
    }

    if tokens.len() > 4 {
        if allow_wildcards && tokens[4] == "*" {
            if 4 != tokens.len() - 1 {
                return Err("Wildcard '*' is only allowed as the final token".to_owned());
            }
            return Ok((parts, true));
        }

        // Glued version wildcard `v*` — the only wildcard form not standing as
        // its own `*` token. The `v` is the mandatory marker that begins every
        // version, so `v*` means "any version" (GTS spec §10 rule 4) and is
        // equivalent to a `*` at this position. Only valid as the final token.
        if allow_wildcards && tokens[4] == "v*" {
            if 4 != tokens.len() - 1 {
                return Err("Wildcard '*' is only allowed as the final token".to_owned());
            }
            return Ok((parts, true));
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
            return Ok((parts, true));
        }

        parts.ver_minor = Some(
            parse_u32_exact(tokens[5])
                .ok_or_else(|| format!("Minor version must be an integer, got '{}'", tokens[5]))?,
        );
    }

    Ok((parts, false))
}

// Field views delegate to each type's inherent accessors (inherent methods take
// priority over trait methods in method-call resolution, so there is no
// recursion here).
impl SegmentView for GtsIdSegment {
    fn vendor(&self) -> &str {
        self.vendor()
    }
    fn package(&self) -> &str {
        self.package()
    }
    fn namespace(&self) -> &str {
        self.namespace()
    }
    fn type_name(&self) -> &str {
        self.type_name()
    }
    fn ver_major(&self) -> u32 {
        self.ver_major()
    }
    fn ver_minor(&self) -> Option<u32> {
        self.ver_minor()
    }
    fn is_type(&self) -> bool {
        self.is_type()
    }
    fn uuid_tail(&self) -> Option<&str> {
        self.uuid_tail()
    }
}

impl SegmentView for GtsIdPatternSegment {
    fn vendor(&self) -> &str {
        self.vendor()
    }
    fn package(&self) -> &str {
        self.package()
    }
    fn namespace(&self) -> &str {
        self.namespace()
    }
    fn type_name(&self) -> &str {
        self.type_name()
    }
    fn ver_major(&self) -> u32 {
        self.ver_major()
    }
    fn ver_minor(&self) -> Option<u32> {
        self.ver_minor()
    }
    fn is_type(&self) -> bool {
        self.is_type()
    }
    fn uuid_tail(&self) -> Option<&str> {
        self.uuid_tail()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ---- concrete segments ----

    #[test]
    fn test_valid_segment_basic() {
        let parsed = GtsIdSegment::parse(1, "x.core.events.event.v1~").unwrap();
        assert_eq!(parsed.vendor(), "x");
        assert_eq!(parsed.package(), "core");
        assert_eq!(parsed.namespace(), "events");
        assert_eq!(parsed.type_name(), "event");
        assert_eq!(parsed.ver_major(), 1);
        assert_eq!(parsed.ver_minor(), None);
        assert!(parsed.is_type());
    }

    #[test]
    fn test_valid_segment_with_minor() {
        let parsed = GtsIdSegment::parse(1, "x.core.events.event.v1.2~").unwrap();
        assert_eq!(parsed.ver_major(), 1);
        assert_eq!(parsed.ver_minor(), Some(2));
    }

    #[test]
    fn test_segment_too_many_tildes() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v1~~").unwrap_err();
        assert!(err.contains("Too many '~' characters"), "got: {err}");
    }

    #[test]
    fn test_segment_tilde_not_at_end() {
        let err = GtsIdSegment::parse(1, "x.core~mid.events.event.v1").unwrap_err();
        assert!(err.contains("'~' must be at the end"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_tokens() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v1.2.extra~").unwrap_err();
        assert!(err.contains("Too many tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_few_tokens() {
        let err = GtsIdSegment::parse(1, "x.core.events.event~").unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    #[test]
    fn test_segment_too_many_name_tokens() {
        let err = GtsIdSegment::parse(2, "x.core.ns.type.extra.v1~").unwrap_err();
        assert!(
            err.contains("Too many name tokens before version"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_without_v() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.1~").unwrap_err();
        assert!(
            err.contains("Major version must start with 'v'"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_not_integer() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.vX~").unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_version_leading_zeros() {
        let err = GtsIdSegment::parse(1, "x.core.events.event.v01~").unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_segment_invalid_vendor_token() {
        let err = GtsIdSegment::parse(1, "1bad.core.events.event.v1~").unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    #[test]
    fn test_concrete_parse_rejects_wildcard() {
        // `GtsIdSegment` is concrete only: a `*` is just an invalid token here.
        let err = GtsIdSegment::parse(1, "x.*").unwrap_err();
        assert!(err.contains("Too few tokens"), "got: {err}");
    }

    // ---- expected_format (surfaced through segment parsing) ----

    #[test]
    fn test_segment1_format_has_gts_prefix() {
        let err = GtsIdSegment::parse(1, "x.core.events.event~").unwrap_err();
        let expected = format!(
            "{}vendor.package.namespace.type.vMAJOR",
            crate::GTS_ID_PREFIX
        );
        assert!(
            err.contains(&expected),
            "segment #1 format should include configured prefix, got: {err}"
        );
    }

    #[test]
    fn test_segment2_format_no_gts_prefix() {
        let err = GtsIdSegment::parse(2, "x.core.events.event~").unwrap_err();
        assert!(
            !err.contains(&format!("{}vendor", crate::GTS_ID_PREFIX)),
            "segment #2 format should NOT include configured prefix, got: {err}"
        );
        assert!(
            err.contains("vendor.package.namespace.type.vMAJOR"),
            "segment #2 should show vendor.package format, got: {err}"
        );
    }

    // ---- pattern segments / wildcards ----

    #[test]
    fn test_wildcard_at_vendor() {
        let parsed = GtsIdPatternSegment::parse(1, "*").unwrap();
        assert!(parsed.is_wildcard());
    }

    #[test]
    fn test_wildcard_at_package() {
        let parsed = GtsIdPatternSegment::parse(1, "x.*").unwrap();
        assert!(parsed.is_wildcard());
        assert_eq!(parsed.vendor(), "x");
    }

    #[test]
    fn test_pattern_concrete_segment_is_not_wildcard() {
        let parsed = GtsIdPatternSegment::parse(1, "x.core.events.event.v1~").unwrap();
        assert!(!parsed.is_wildcard());
        assert_eq!(parsed.vendor(), "x");
        assert!(parsed.is_type());
    }

    #[test]
    fn test_wildcard_invalid_token_before_star() {
        // Tokens before '*' must still be validated
        let err = GtsIdPatternSegment::parse(1, "1bad.*").unwrap_err();
        assert!(err.contains("Invalid vendor token"), "got: {err}");
    }

    #[test]
    fn test_wildcard_in_middle_rejected() {
        // '*' in a non-final position must be rejected
        let err = GtsIdPatternSegment::parse(1, "x.*.ns.type.v1").unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_wildcard_at_version_position_not_final() {
        // '*' at version position (4) with extra token after it must be rejected
        let err = GtsIdPatternSegment::parse(1, "x.pkg.ns.type.*.extra").unwrap_err();
        assert!(
            err.contains("only allowed as the final token"),
            "got: {err}"
        );
    }

    #[test]
    fn test_glued_version_wildcard() {
        // `v*` is the one wildcard form glued to a token: the version marker `v`
        // plus `*` means "any version". It parses as a wildcard segment carrying
        // the vendor..type prefix and no version.
        let parsed = GtsIdPatternSegment::parse(1, "x.pkg.ns.type.v*").unwrap();
        assert!(parsed.is_wildcard());
        assert_eq!(parsed.vendor(), "x");
        assert_eq!(parsed.package(), "pkg");
        assert_eq!(parsed.namespace(), "ns");
        assert_eq!(parsed.type_name(), "type");
        assert_eq!(parsed.ver_major(), 0);
        assert_eq!(parsed.ver_minor(), None);
    }

    #[test]
    fn test_glued_version_wildcard_only_v_star() {
        // Only the bare `v*` is the glued form. A partial major like `v1*` is
        // not a wildcard — it fails as a malformed version.
        let err = GtsIdPatternSegment::parse(1, "x.pkg.ns.type.v1*").unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_glued_version_wildcard_rejected_at_minor() {
        // `v*` is only the major-version wildcard; at the minor position it is a
        // malformed minor, not a wildcard.
        let err = GtsIdPatternSegment::parse(1, "x.pkg.ns.type.v1.v*").unwrap_err();
        assert!(
            err.contains("Minor version must be an integer"),
            "got: {err}"
        );
    }

    #[test]
    fn test_glued_version_wildcard_rejected_for_concrete() {
        // A concrete segment never accepts `v*` — wildcards need `allow_wildcards`.
        let err = GtsIdSegment::parse(1, "x.pkg.ns.type.v*").unwrap_err();
        assert!(
            err.contains("Major version must be an integer"),
            "got: {err}"
        );
    }

    // ---- UUID tail ----

    #[test]
    fn test_uuid_tail_segment_accessors() {
        const UUID_TAIL: &str = "7a1d2f34-5678-49ab-9012-abcdef123456";

        let seg = GtsIdSegment::uuid_tail_segment(UUID_TAIL);
        assert_eq!(seg.uuid_tail(), Some(UUID_TAIL));
        assert_eq!(seg.raw(), UUID_TAIL);
        assert!(!seg.is_type());
        assert_eq!(seg.vendor(), "");
        assert_eq!(seg.ver_major(), 0);
        assert_eq!(seg.ver_minor(), None);

        #[cfg(feature = "uuid")]
        {
            let expected = uuid::Uuid::parse_str(UUID_TAIL).ok();
            assert_eq!(seg.uuid(), expected);

            let GtsIdSegment::UuidTail(tail) = &seg else {
                panic!("expected uuid-tail segment");
            };
            assert_eq!(tail.uuid(), expected);
        }
    }

    #[test]
    fn test_concrete_and_wildcard_have_no_uuid_tail() {
        let concrete = GtsIdSegment::parse(1, "x.core.events.event.v1~").unwrap();
        assert_eq!(concrete.uuid_tail(), None);
        #[cfg(feature = "uuid")]
        assert_eq!(concrete.uuid(), None);

        let wildcard = GtsIdPatternSegment::parse(1, "x.*").unwrap();
        assert_eq!(wildcard.uuid_tail(), None);
    }

    #[test]
    fn test_segment_parts_accessors() {
        let concrete = GtsIdSegment::parse(1, "x.core.events.event.v1.2~").unwrap();
        let GtsIdSegment::Concrete(parts) = concrete else {
            panic!("expected concrete segment");
        };

        assert_eq!(parts.raw(), "x.core.events.event.v1.2~");
        assert_eq!(parts.vendor(), "x");
        assert_eq!(parts.package(), "core");
        assert_eq!(parts.namespace(), "events");
        assert_eq!(parts.type_name(), "event");
        assert_eq!(parts.ver_major(), 1);
        assert_eq!(parts.ver_minor(), Some(2));
        assert!(parts.is_type());
    }
}
