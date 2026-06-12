//! GTS identifier match patterns.
//!
//! [`GtsIdPattern`] is a validated GTS identifier that may end in a single
//! trailing `*` wildcard token, but may also be fully concrete — a zero-wildcard
//! pattern matches exactly one identifier (with minor-version flexibility). It
//! supports prefix-based coverage reasoning via [`covers`](GtsIdPattern::covers);
//! actual ID-vs-pattern matching lives on [`GtsId::matches_pattern`].

use std::fmt;
use std::str::FromStr;

use crate::gts_id_segment::SegmentView;
use crate::{GtsIdError, GtsIdPatternSegment};

/// A GTS identifier match pattern (exact or trailing-`*` wildcard).
#[derive(Debug, Clone, PartialEq)]
pub struct GtsIdPattern {
    pattern: String,
    segments: Vec<GtsIdPatternSegment>,
}

impl GtsIdPattern {
    /// Creates a new GTS identifier match pattern.
    ///
    /// The pattern may be fully concrete or end in a single trailing `*`
    /// wildcard. All validation is delegated to the parser
    /// ([`crate::parse::parse_pattern`]), so a pattern is
    /// parsed identically to the same string passed through any other entry
    /// point.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the string is not a valid GTS identifier pattern.
    pub fn try_new(pattern: &str) -> Result<Self, GtsIdError> {
        let p = pattern.trim();

        // Parse with the pattern parser (not GtsId::try_new, which rejects
        // wildcards) since a pattern is an identifier-shaped string that may
        // legitimately contain a trailing '*'.
        let segments = crate::parse::parse_pattern(p)?;

        Ok(GtsIdPattern {
            pattern: p.to_owned(),
            segments,
        })
    }

    /// The validated pattern string.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// The parsed segments of this pattern.
    #[must_use]
    pub fn segments(&self) -> &[GtsIdPatternSegment] {
        &self.segments
    }

    /// Consumes the pattern, returning its parsed segments.
    #[must_use]
    pub fn into_segments(self) -> Vec<GtsIdPatternSegment> {
        self.segments
    }

    /// Returns `true` if `self` (used as a candidate) matches `pattern`.
    ///
    /// Mirrors [`GtsId::matches_pattern`] for the case where the candidate is
    /// itself a pattern: the candidate's fixed prefix is matched field-by-field
    /// against `pattern` (with minor-version flexibility).
    #[must_use]
    pub fn matches_pattern(&self, pattern: &GtsIdPattern) -> bool {
        pattern.matches_views(self.segments())
    }

    /// Returns `true` if this pattern matches the given candidate segments.
    ///
    /// This is the core matching primitive; [`GtsId::matches_pattern`] and
    /// [`GtsIdPattern::matches_pattern`] both delegate to it. A non-wildcard
    /// pattern segment must match exactly (with minor-version flexibility); a
    /// wildcard segment accepts anything from that point on. The candidate's
    /// segments are read through [`SegmentView`], so they may be concrete
    /// ([`GtsIdSegment`](crate::GtsIdSegment)) or pattern segments.
    ///
    /// [`GtsId::matches_pattern`]: crate::GtsId::matches_pattern
    pub(crate) fn matches_views<C: SegmentView>(&self, candidate: &[C]) -> bool {
        let pattern_segs = &self.segments;
        // If pattern is longer than candidate, no match
        if pattern_segs.len() > candidate.len() {
            return false;
        }

        for (i, p_seg) in pattern_segs.iter().enumerate() {
            let c_seg = &candidate[i];

            // If pattern segment is a wildcard, only its specified (non-empty)
            // prefix fields must match; it then accepts anything after this point.
            if p_seg.is_wildcard() {
                if !p_seg.vendor().is_empty() && p_seg.vendor() != c_seg.vendor() {
                    return false;
                }
                if !p_seg.package().is_empty() && p_seg.package() != c_seg.package() {
                    return false;
                }
                if !p_seg.namespace().is_empty() && p_seg.namespace() != c_seg.namespace() {
                    return false;
                }
                if !p_seg.type_name().is_empty() && p_seg.type_name() != c_seg.type_name() {
                    return false;
                }
                if p_seg.ver_major() != 0 && p_seg.ver_major() != c_seg.ver_major() {
                    return false;
                }
                if let Some(p_minor) = p_seg.ver_minor()
                    && Some(p_minor) != c_seg.ver_minor()
                {
                    return false;
                }
                // No `is_type` check here: a wildcard segment never carries a
                // type marker. `parse_pattern` only accepts `*` as the final
                // token of a pattern ending in `.*`/`~*`, so a `*` tilde-part is
                // always the last one and never gets a trailing `~` appended.
                // A wildcard therefore matches a candidate position regardless
                // of whether that candidate segment is a type or an instance.
                return true;
            }

            // Non-wildcard UUID tail - compare raw segment string (the actual UUID)
            if p_seg.uuid_tail().is_some() && p_seg.uuid_tail() != c_seg.uuid_tail() {
                return false;
            }

            // Non-wildcard segment - all fields must match exactly
            if p_seg.vendor() != c_seg.vendor() {
                return false;
            }
            if p_seg.package() != c_seg.package() {
                return false;
            }
            if p_seg.namespace() != c_seg.namespace() {
                return false;
            }
            if p_seg.type_name() != c_seg.type_name() {
                return false;
            }

            // Check version matching
            if p_seg.ver_major() != c_seg.ver_major() {
                return false;
            }

            // Minor version: if pattern has no minor version, accept any minor in candidate
            if let Some(p_minor) = p_seg.ver_minor()
                && Some(p_minor) != c_seg.ver_minor()
            {
                return false;
            }

            // Check is_type flag matches
            if p_seg.is_type() != c_seg.is_type() {
                return false;
            }
        }

        true
    }

    /// Returns the non-wildcard prefix of a pattern string.
    ///
    /// For `"gts.x.core.srr.resource.v1~*"` returns `"gts.x.core.srr.resource.v1~"`.
    /// For an exact pattern (no `*`) returns the full string.
    fn prefix_str(pattern: &str) -> &str {
        match pattern.find('*') {
            Some(idx) => &pattern[..idx],
            None => pattern,
        }
    }

    /// Returns `true` if `self` covers `other`: every GTS ID matched by the
    /// `other` pattern is also matched by `self`.
    ///
    /// In other words `self` is the broader (less specific) pattern and `other`
    /// is contained within it. This holds exactly when `self`'s fixed prefix is a
    /// prefix of `other`'s fixed prefix. An exact pattern covers only itself.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let broad  = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*")?;
    /// let narrow = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~acme.*")?;
    /// assert!(broad.covers(&narrow));   // "*" covers "acme.*"
    /// assert!(!narrow.covers(&broad));  // but not the other way round
    ///
    /// let other  = GtsIdPattern::try_new("gts.x.core.other.resource.v1~*")?;
    /// assert!(!broad.covers(&other));   // different base type — no coverage
    /// ```
    #[must_use]
    pub fn covers(&self, other: &GtsIdPattern) -> bool {
        let self_prefix = Self::prefix_str(&self.pattern);
        let other_prefix = Self::prefix_str(&other.pattern);
        other_prefix.starts_with(self_prefix)
    }

    /// Checks if a string is a valid GTS identifier pattern.
    #[must_use]
    pub fn is_valid(s: &str) -> bool {
        Self::try_new(s).is_ok()
    }
}

impl fmt::Display for GtsIdPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pattern)
    }
}

impl FromStr for GtsIdPattern {
    type Err = GtsIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl AsRef<str> for GtsIdPattern {
    fn as_ref(&self) -> &str {
        &self.pattern
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::GtsId;

    #[test]
    fn test_gts_wildcard_simple() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_no_match() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let id = GtsId::try_new("gts.y.core.events.event.v1~").expect("test");
        assert!(!id.matches_pattern(&pattern));
    }

    #[test]
    fn test_trailing_wildcard_ignores_type_marker() {
        // A trailing `*` matches the candidate position whether the candidate
        // segment there is a type (`~`) or an instance. A wildcard segment never
        // carries its own type marker, so it imposes no `is_type` constraint.
        let pattern = GtsIdPattern::try_new("gts.x.core.events.topic.v1~*").expect("test");

        let type_candidate =
            GtsId::try_new("gts.x.core.events.topic.v1~vendor.app.orders.thing.v1~").expect("test");
        let instance_candidate =
            GtsId::try_new("gts.x.core.events.topic.v1~vendor.app.orders.thing.v1.0")
                .expect("test");

        assert!(type_candidate.matches_pattern(&pattern));
        assert!(instance_candidate.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_type_suffix() {
        // Wildcard after ~ should match type IDs
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_version_flexibility_in_matching() {
        // Pattern without minor version should match any minor version
        let pattern = GtsIdPattern::try_new("gts.x.core.events.event.v1~").expect("test");
        let id_no_minor = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        let id_with_minor = GtsId::try_new("gts.x.core.events.event.v1.0~").expect("test");

        assert!(id_no_minor.matches_pattern(&pattern));
        assert!(id_with_minor.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_exact_match() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.event.v1~").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_version_mismatch() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.event.v2~").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert!(!id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_with_minor_version() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.event.v1.0~").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1.0~").expect("test");
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_invalid_pattern() {
        let result = GtsIdPattern::try_new("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_wildcard_multiple_wildcards_error() {
        let result = GtsIdPattern::try_new("gts.*.*.*.*");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_wildcard_instance_match() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let id = GtsId::try_new("gts.x.core.events.event.v1~a.b.c.d.v1.0").expect("test");
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_whitespace_trimming() {
        let pattern = GtsIdPattern::try_new("  gts.x.core.events.*  ").expect("test");
        assert_eq!(pattern.pattern(), "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_only_at_end() {
        // Wildcard in middle should fail
        let result1 = GtsIdPattern::try_new("gts.*.core.events.event.v1~");
        assert!(result1.is_err());

        // Wildcard at end should work
        let pattern2 = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let id2 = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert!(id2.matches_pattern(&pattern2));
    }

    #[test]
    fn test_gts_wildcard_no_wildcard_different_vendor() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.event.v1~").expect("test");
        let id = GtsId::try_new("gts.y.core.events.event.v1~").expect("test");
        assert!(!id.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_display_trait() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        assert_eq!(format!("{pattern}"), "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_from_str_trait() {
        let pattern: GtsIdPattern = "gts.x.core.events.*".parse().expect("test");
        assert_eq!(pattern.pattern(), "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_as_ref_trait() {
        let pattern = GtsIdPattern::try_new("gts.x.core.events.*").expect("test");
        let s: &str = pattern.as_ref();
        assert_eq!(s, "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_type_suffix_match() {
        // Wildcard after type suffix
        let pattern = GtsIdPattern::try_new("gts.x.pkg.ns.type.v1~*").expect("test");
        let id1 = GtsId::try_new("gts.x.pkg.ns.type.v1~a.b.c.child.v1~").expect("test");
        let id2 = GtsId::try_new("gts.x.pkg.ns.type.v2~a.b.c.child.v1~").expect("test");
        assert!(id1.matches_pattern(&pattern));
        assert!(!id2.matches_pattern(&pattern));
    }

    #[test]
    fn test_gts_wildcard_at_various_positions() {
        // Wildcard at vendor position
        let result = GtsIdPattern::try_new("gts.*");
        assert!(result.is_ok());

        // Wildcard at package position
        let result = GtsIdPattern::try_new("gts.x.*");
        assert!(result.is_ok());

        // Wildcard at namespace position
        let result = GtsIdPattern::try_new("gts.x.pkg.*");
        assert!(result.is_ok());

        // Wildcard at type position
        let result = GtsIdPattern::try_new("gts.x.pkg.ns.*");
        assert!(result.is_ok());

        // Wildcard at version position
        let result = GtsIdPattern::try_new("gts.x.pkg.ns.type.*");
        assert!(result.is_ok());
    }

    // ---- covers ----

    #[test]
    fn test_covers_broad_covers_narrow() {
        let broad = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        let narrow = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~acme.*").expect("test");
        // Coverage is directional: the broad pattern covers the narrow one,
        // never the reverse.
        assert!(broad.covers(&narrow));
        assert!(!narrow.covers(&broad));
    }

    #[test]
    fn test_covers_disjoint_types() {
        let a = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsIdPattern::try_new("gts.x.core.other.resource.v1~*").expect("test");
        assert!(!a.covers(&b));
        assert!(!b.covers(&a));
    }

    #[test]
    fn test_covers_identical_patterns() {
        let a = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        // A pattern covers an identical one (both directions).
        assert!(a.covers(&b));
        assert!(b.covers(&a));
    }

    #[test]
    fn test_covers_wildcard_covers_exact() {
        let exact = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~acme.crm._.contact.v1~")
            .expect("test");
        let broad = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        assert!(broad.covers(&exact));
        assert!(!exact.covers(&broad));
    }

    #[test]
    fn test_covers_three_levels() {
        let l1 = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~*").expect("test");
        let l2 = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~acme.*").expect("test");
        let l3 = GtsIdPattern::try_new("gts.x.core.srr.resource.v1~acme.crm.*").expect("test");
        // Broader patterns cover narrower ones, transitively.
        assert!(l1.covers(&l2));
        assert!(l1.covers(&l3));
        assert!(l2.covers(&l3));
        assert!(!l2.covers(&l1));
        assert!(!l3.covers(&l1));
        assert!(!l3.covers(&l2));
    }

    // ---- is_valid ----

    #[test]
    fn test_is_valid() {
        // Exact ids and trailing-`*` patterns are valid.
        assert!(GtsIdPattern::is_valid("gts.x.core.events.event.v1~"));
        assert!(GtsIdPattern::is_valid("gts.x.core.events.*"));
        assert!(GtsIdPattern::is_valid("gts.x.core.events.topic.v1~*"));

        // Malformed strings and misplaced wildcards are not.
        assert!(!GtsIdPattern::is_valid("not-a-gts-id"));
        assert!(!GtsIdPattern::is_valid("gts.x.*.events.event.v1~"));
        assert!(!GtsIdPattern::is_valid("gts.x.core.events.topic.v1.*~"));
    }
}
