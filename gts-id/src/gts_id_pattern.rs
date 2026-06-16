//! GTS identifier match patterns.
//!
//! [`GtsIdPattern`] is a validated GTS identifier that may end in a single
//! trailing `*` wildcard token, but may also be fully concrete. A zero-wildcard
//! pattern is not exact-match: a base type id used as a pattern also matches
//! everything derived from it down the chain — `gts.a.b.c.d.v1~` behaves as the
//! implicit envelope `gts.a.b.c.d.v1~*` (GTS spec §3.6, "implicit derived-type
//! coverage"), with minor-version flexibility. It supports segment-wise coverage
//! reasoning via [`covers`](GtsIdPattern::covers); actual ID-vs-pattern matching
//! lives on [`GtsId::matches_pattern`].

use std::fmt;
use std::str::FromStr;

use crate::gts_id_segment::SegmentView;
use crate::{GtsId, GtsIdError, GtsIdPatternSegment};

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

    /// Returns `true` if `self` covers `other`: every GTS ID matched by the
    /// `other` pattern is also matched by `self`.
    ///
    /// In other words `self` is the broader (less specific) pattern and `other`
    /// is contained within it. Coverage is decided segment-by-segment with the
    /// same field logic as matching — minor-version flexibility, wildcard
    /// widening, and the implicit derived-type coverage of a bare type id — so
    /// `…event.v1~*` covers `…event.v1.0~*` (any-minor covers a specific minor)
    /// and `…event.v1~` covers `…event.v1.0~`, which a naive string-prefix test
    /// would miss.
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
        // `self` covers `other` iff `self`, used as a pattern, matches `other`'s
        // segments taken as the candidate: where `self` is concrete the fields
        // must agree, and where `self` widens (wildcard / omitted minor) it
        // accepts whatever `other` fixes at that position.
        self.matches_views(other.segments())
    }

    /// Checks if a string is a valid GTS identifier pattern.
    #[must_use]
    pub fn is_valid(s: &str) -> bool {
        Self::try_new(s).is_ok()
    }
}

impl From<&GtsId> for GtsIdPattern {
    /// A concrete [`GtsId`] is always a valid zero-wildcard pattern. The
    /// conversion reuses the id's already-validated segments (wrapping each in
    /// [`GtsIdPatternSegment::Segment`]) instead of re-parsing, so it is cheap
    /// and cannot fail.
    ///
    /// Note this is not an "exact-match" pattern: a base type id used as a
    /// pattern also matches everything derived from it down the chain — a type
    /// id `gts.a.b.c.d.v1~` behaves as the implicit envelope `gts.a.b.c.d.v1~*`
    /// (GTS spec §3.6, "implicit derived-type coverage").
    fn from(id: &GtsId) -> Self {
        GtsIdPattern {
            pattern: id.id().to_owned(),
            segments: id
                .segments()
                .iter()
                .cloned()
                .map(GtsIdPatternSegment::Segment)
                .collect(),
        }
    }
}

impl From<GtsId> for GtsIdPattern {
    /// Consuming variant of [`From<&GtsId>`](GtsIdPattern). Reuses the id's owned
    /// segments via [`GtsId::into_segments`], avoiding the per-segment clone.
    fn from(id: GtsId) -> Self {
        let pattern = id.id().to_owned();
        GtsIdPattern {
            pattern,
            segments: id
                .into_segments()
                .into_iter()
                .map(GtsIdPatternSegment::Segment)
                .collect(),
        }
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
    fn test_pattern_longer_than_candidate_does_not_match() {
        let pattern =
            GtsIdPattern::try_new("gts.x.core.events.topic.v1~vendor.app.orders.order.v1~")
                .expect("test");
        let id = GtsId::try_new("gts.x.core.events.topic.v1~").expect("test");
        assert!(!id.matches_pattern(&pattern));
    }

    #[test]
    fn test_uuid_tail_mismatch_does_not_match() {
        let pattern = GtsIdPattern::try_new(
            "gts.x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123456",
        )
        .expect("test");
        let id = GtsId::try_new("gts.x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123457")
            .expect("test");
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

    // ---- glued version wildcard `v*` ----

    #[test]
    fn test_version_wildcard_valid_and_is_wildcard() {
        let pattern = GtsIdPattern::try_new("gts.x.llm.chat.message.v*").expect("test");
        assert_eq!(pattern.segments().len(), 1);
        assert!(pattern.segments()[0].is_wildcard());
        assert!(GtsIdPattern::is_valid("gts.x.llm.chat.message.v*"));
    }

    #[test]
    fn test_version_wildcard_matches_any_version_and_chain() {
        let pattern = GtsIdPattern::try_new("gts.x.llm.chat.message.v*").expect("test");
        for id in [
            "gts.x.llm.chat.message.v1.0~",
            "gts.x.llm.chat.message.v1.1~",
            "gts.x.llm.chat.message.v2~",
            "gts.x.llm.chat.message.v1.0~acme.app.ns.derived.v1~",
        ] {
            let candidate = GtsId::try_new(id).expect("test");
            assert!(candidate.matches_pattern(&pattern), "should match: {id}");
        }
        // Different type is not matched.
        let other = GtsId::try_new("gts.x.llm.chat.other.v1~").expect("test");
        assert!(!other.matches_pattern(&pattern));
    }

    #[test]
    fn test_version_wildcard_equivalent_to_version_position_star() {
        // `message.v*` and `message.*` match the same set: the `v` marker adds no
        // constraint because every version token starts with `v`.
        let v_star = GtsIdPattern::try_new("gts.x.llm.chat.message.v*").expect("test");
        let dot_star = GtsIdPattern::try_new("gts.x.llm.chat.message.*").expect("test");
        for id in [
            "gts.x.llm.chat.message.v1~",
            "gts.x.llm.chat.message.v9.9~",
            "gts.x.llm.chat.message.v1.0~acme.app.ns.derived.v1~",
        ] {
            let candidate = GtsId::try_new(id).expect("test");
            assert_eq!(
                candidate.matches_pattern(&v_star),
                candidate.matches_pattern(&dot_star),
                "match divergence for {id}"
            );
        }
    }

    #[test]
    fn test_version_wildcard_rejections() {
        // `*` after `v*` — two wildcards.
        assert!(!GtsIdPattern::is_valid("gts.x.llm.chat.message.v*~*"));
        // A stray `~` after the wildcard — `*` is not the final character.
        assert!(!GtsIdPattern::is_valid("gts.x.llm.chat.message.v1.*~"));
        // Partial (non-version) token wildcard.
        assert!(!GtsIdPattern::is_valid("gts.x.llm.chat.msg*"));
    }

    // ---- covers: minor-version flexibility (segment-based) ----

    #[test]
    fn test_covers_any_minor_covers_specific_minor() {
        // A pattern pinned to a major (no minor) is broader than one pinned to a
        // specific minor — segment-based coverage captures this; string prefixes
        // would not (`…v1~` is not a string prefix of `…v1.0~`).
        let any_minor = GtsIdPattern::try_new("gts.x.core.events.event.v1~*").expect("test");
        let specific = GtsIdPattern::try_new("gts.x.core.events.event.v1.0~*").expect("test");
        assert!(any_minor.covers(&specific));
        assert!(!specific.covers(&any_minor));
    }

    #[test]
    fn test_covers_bare_type_minor_flexibility() {
        let any_minor = GtsIdPattern::try_new("gts.x.core.events.event.v1~").expect("test");
        let specific = GtsIdPattern::try_new("gts.x.core.events.event.v1.0~").expect("test");
        assert!(any_minor.covers(&specific));
        assert!(!specific.covers(&any_minor));
    }

    #[test]
    fn test_covers_major_version_mismatch() {
        let v1 = GtsIdPattern::try_new("gts.x.core.events.event.v1~*").expect("test");
        let v2 = GtsIdPattern::try_new("gts.x.core.events.event.v2~*").expect("test");
        assert!(!v1.covers(&v2));
        assert!(!v2.covers(&v1));
    }

    // ---- From<GtsId> conversion ----

    #[test]
    fn test_from_gts_id_ref() {
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        let pattern = GtsIdPattern::from(&id);
        // The pattern string is the id verbatim.
        assert_eq!(pattern.pattern(), id.id());
        // A concrete id maps to a concrete pattern — no wildcard segments.
        assert!(pattern.segments().iter().all(|s| !s.is_wildcard()));
        // The id at minimum matches the pattern derived from itself.
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_concrete_pattern_covers_derived_chain() {
        // Per GTS spec §3.6 "implicit derived-type coverage": a base type id used
        // as a pattern is treated as the implicit envelope `…~*`, so it matches
        // not only itself but every type/instance derived from it down the chain.
        let pattern = GtsId::try_new("gts.a.b.c.d.v1~")
            .expect("test")
            .to_pattern();

        // Exact and derived candidates both match.
        let exact = GtsId::try_new("gts.a.b.c.d.v1~").expect("test");
        let derived = GtsId::try_new("gts.a.b.c.d.v1~w.x.y.z.v1").expect("test");
        assert!(exact.matches_pattern(&pattern));
        assert!(derived.matches_pattern(&pattern));

        // A different base type is not covered.
        let other_base = GtsId::try_new("gts.a.b.c.other.v1~w.x.y.z.v1").expect("test");
        assert!(!other_base.matches_pattern(&pattern));
    }

    #[test]
    fn test_from_gts_id_owned() {
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        let expected = id.id().to_owned();
        let pattern: GtsIdPattern = id.into();
        assert_eq!(pattern.pattern(), expected);
        assert!(pattern.segments().iter().all(|s| !s.is_wildcard()));
    }

    #[test]
    fn test_from_gts_id_chained_preserves_segments() {
        let id = GtsId::try_new("gts.x.core.events.topic.v1~vendor.app.orders.thing.v1.0")
            .expect("test");
        let pattern = GtsIdPattern::from(&id);
        assert_eq!(pattern.segments().len(), id.segments().len());
        assert_eq!(pattern.pattern(), id.id());
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_from_ref_and_owned_agree() {
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        let from_ref = GtsIdPattern::from(&id);
        let from_owned = GtsIdPattern::from(id);
        // Borrowing and consuming conversions produce the same pattern.
        assert_eq!(from_ref, from_owned);
    }

    #[test]
    fn test_from_gts_id_matches_to_pattern() {
        let id = GtsId::try_new("gts.x.core.events.event.v1~").expect("test");
        // The inherent `to_pattern` is just the ergonomic form of `From<&GtsId>`.
        assert_eq!(GtsIdPattern::from(&id), id.to_pattern());
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
