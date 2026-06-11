//! GTS wildcard patterns.
//!
//! [`GtsWildcard`] is a validated GTS identifier that may end in a single `*`
//! token. It supports prefix-based [`overlaps`](GtsWildcard::overlaps) and
//! [`is_subset_of`](GtsWildcard::is_subset_of) reasoning; actual ID-vs-pattern
//! matching lives on [`GtsID::wildcard_match`].

use std::fmt;
use std::str::FromStr;

use crate::{GtsIdError, GtsIdSegment};

/// GTS Wildcard pattern
#[derive(Debug, Clone, PartialEq)]
pub struct GtsWildcard {
    id: String,
    segments: Vec<GtsIdSegment>,
}

impl GtsWildcard {
    /// The validated wildcard pattern string.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The parsed segments of this pattern.
    #[must_use]
    pub fn segments(&self) -> &[GtsIdSegment] {
        &self.segments
    }

    /// Consumes the pattern, returning its parsed segments.
    #[must_use]
    pub fn into_segments(self) -> Vec<GtsIdSegment> {
        self.segments
    }

    /// Returns `true` if this pattern matches the given parsed candidate segments.
    ///
    /// This is the core matching primitive; [`GtsID::wildcard_match`] delegates
    /// to it. A non-wildcard pattern segment must match exactly (with minor-version
    /// flexibility); a wildcard segment accepts anything from that point on.
    ///
    /// [`GtsID::wildcard_match`]: crate::GtsID::wildcard_match
    #[must_use]
    pub fn matches_segments(&self, candidate: &[GtsIdSegment]) -> bool {
        let pattern_segs = &self.segments;
        // If pattern is longer than candidate, no match
        if pattern_segs.len() > candidate.len() {
            return false;
        }

        for (i, p_seg) in pattern_segs.iter().enumerate() {
            let c_seg = &candidate[i];

            // If pattern segment is a wildcard, check non-wildcard fields first
            if p_seg.is_wildcard {
                if !p_seg.vendor.is_empty() && p_seg.vendor != c_seg.vendor {
                    return false;
                }
                if !p_seg.package.is_empty() && p_seg.package != c_seg.package {
                    return false;
                }
                if !p_seg.namespace.is_empty() && p_seg.namespace != c_seg.namespace {
                    return false;
                }
                if !p_seg.type_name.is_empty() && p_seg.type_name != c_seg.type_name {
                    return false;
                }
                if p_seg.ver_major != 0 && p_seg.ver_major != c_seg.ver_major {
                    return false;
                }
                if let Some(p_minor) = p_seg.ver_minor
                    && Some(p_minor) != c_seg.ver_minor
                {
                    return false;
                }
                if p_seg.is_type && p_seg.is_type != c_seg.is_type {
                    return false;
                }
                // Wildcard matches - accept anything after this point
                return true;
            }

            // Non-wildcard UUID tail - compare raw segment string (the actual UUID)
            if p_seg.is_uuid_tail && p_seg.segment != c_seg.segment {
                return false;
            }

            // Non-wildcard segment - all fields must match exactly
            if p_seg.vendor != c_seg.vendor {
                return false;
            }
            if p_seg.package != c_seg.package {
                return false;
            }
            if p_seg.namespace != c_seg.namespace {
                return false;
            }
            if p_seg.type_name != c_seg.type_name {
                return false;
            }

            // Check version matching
            if p_seg.ver_major != c_seg.ver_major {
                return false;
            }

            // Minor version: if pattern has no minor version, accept any minor in candidate
            if let Some(p_minor) = p_seg.ver_minor
                && Some(p_minor) != c_seg.ver_minor
            {
                return false;
            }

            // Check is_type flag matches
            if p_seg.is_type != c_seg.is_type {
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

    /// Returns `true` if there is at least one GTS ID that matches **both** patterns.
    ///
    /// Two patterns overlap when one pattern's fixed prefix is a prefix of the
    /// other's fixed prefix (or they share the same prefix), meaning there exists
    /// at least one concrete GTS ID that satisfies both constraints.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let broad  = GtsWildcard::new("gts.x.core.srr.resource.v1~*")?;
    /// let narrow = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.*")?;
    /// assert!(broad.overlaps(&narrow));  // "acme.*" is a subset of "*"
    ///
    /// let other  = GtsWildcard::new("gts.x.core.other.resource.v1~*")?;
    /// assert!(!broad.overlaps(&other));  // different base type — no overlap
    /// ```
    #[must_use]
    pub fn overlaps(&self, other: &GtsWildcard) -> bool {
        let a = Self::prefix_str(&self.id);
        let b = Self::prefix_str(&other.id);
        a.starts_with(b) || b.starts_with(a)
    }

    /// Returns `true` if every GTS ID matching `self` also matches `other`.
    ///
    /// In other words, `self` is a **narrower** (more specific) pattern than `other`:
    /// the effective type set of `self` is a subset of the effective type set of `other`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let broad  = GtsWildcard::new("gts.x.core.srr.resource.v1~*")?;
    /// let narrow = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.*")?;
    /// assert!(narrow.is_subset_of(&broad));
    /// assert!(!broad.is_subset_of(&narrow));
    /// ```
    #[must_use]
    pub fn is_subset_of(&self, other: &GtsWildcard) -> bool {
        let a = Self::prefix_str(&self.id);
        let b = Self::prefix_str(&other.id);
        a.starts_with(b)
    }

    /// Creates a new GTS wildcard pattern.
    ///
    /// All validation is delegated to the parser
    /// ([`crate::parse::parse_gts_string`] with wildcards enabled), so a wildcard
    /// is parsed identically to the same string passed through any other entry
    /// point.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the pattern is not a valid GTS wildcard.
    pub fn new(pattern: &str) -> Result<Self, GtsIdError> {
        let p = pattern.trim();

        // Go straight to the parser (not GtsID::new, which rejects wildcards)
        // since a wildcard is an identifier-shaped pattern that may legitimately
        // contain a trailing '*'.
        let segments = crate::parse::parse_gts_string(p, true)?;

        Ok(GtsWildcard {
            id: p.to_owned(),
            segments,
        })
    }
}

impl fmt::Display for GtsWildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl FromStr for GtsWildcard {
    type Err = GtsIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for GtsWildcard {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::GtsID;

    #[test]
    fn test_gts_wildcard_simple() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_no_match() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id = GtsID::new("gts.y.core.events.event.v1~").expect("test");
        assert!(!id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_type_suffix() {
        // Wildcard after ~ should match type IDs
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_version_flexibility_in_matching() {
        // Pattern without minor version should match any minor version
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1~").expect("test");
        let id_no_minor = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let id_with_minor = GtsID::new("gts.x.core.events.event.v1.0~").expect("test");

        assert!(id_no_minor.wildcard_match(&pattern));
        assert!(id_with_minor.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_exact_match() {
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1~").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_version_mismatch() {
        let pattern = GtsWildcard::new("gts.x.core.events.event.v2~").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert!(!id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_with_minor_version() {
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1.0~").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1.0~").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_invalid_pattern() {
        let result = GtsWildcard::new("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_wildcard_multiple_wildcards_error() {
        let result = GtsWildcard::new("gts.*.*.*.*");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_wildcard_instance_match() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~a.b.c.d.v1.0").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_whitespace_trimming() {
        let pattern = GtsWildcard::new("  gts.x.core.events.*  ").expect("test");
        assert_eq!(pattern.id, "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_only_at_end() {
        // Wildcard in middle should fail
        let result1 = GtsWildcard::new("gts.*.core.events.event.v1~");
        assert!(result1.is_err());

        // Wildcard at end should work
        let pattern2 = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id2 = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert!(id2.wildcard_match(&pattern2));
    }

    #[test]
    fn test_gts_wildcard_no_wildcard_different_vendor() {
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1~").expect("test");
        let id = GtsID::new("gts.y.core.events.event.v1~").expect("test");
        assert!(!id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_display_trait() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        assert_eq!(format!("{pattern}"), "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_from_str_trait() {
        let pattern: GtsWildcard = "gts.x.core.events.*".parse().expect("test");
        assert_eq!(pattern.id, "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_as_ref_trait() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let s: &str = pattern.as_ref();
        assert_eq!(s, "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_wildcard_type_suffix_match() {
        // Wildcard after type suffix
        let pattern = GtsWildcard::new("gts.x.pkg.ns.type.v1~*").expect("test");
        let id1 = GtsID::new("gts.x.pkg.ns.type.v1~a.b.c.child.v1~").expect("test");
        let id2 = GtsID::new("gts.x.pkg.ns.type.v2~a.b.c.child.v1~").expect("test");
        assert!(id1.wildcard_match(&pattern));
        assert!(!id2.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_wildcard_at_various_positions() {
        // Wildcard at vendor position
        let result = GtsWildcard::new("gts.*");
        assert!(result.is_ok());

        // Wildcard at package position
        let result = GtsWildcard::new("gts.x.*");
        assert!(result.is_ok());

        // Wildcard at namespace position
        let result = GtsWildcard::new("gts.x.pkg.*");
        assert!(result.is_ok());

        // Wildcard at type position
        let result = GtsWildcard::new("gts.x.pkg.ns.*");
        assert!(result.is_ok());

        // Wildcard at version position
        let result = GtsWildcard::new("gts.x.pkg.ns.type.*");
        assert!(result.is_ok());
    }

    // ---- overlaps ----

    #[test]
    fn test_overlaps_broad_and_narrow() {
        let broad = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let narrow = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.*").expect("test");
        assert!(broad.overlaps(&narrow));
        assert!(narrow.overlaps(&broad)); // symmetric
    }

    #[test]
    fn test_overlaps_disjoint_types() {
        let a = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsWildcard::new("gts.x.core.other.resource.v1~*").expect("test");
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn test_overlaps_same_pattern() {
        let a = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        assert!(a.overlaps(&b));
    }

    #[test]
    fn test_overlaps_exact_vs_wildcard() {
        let exact =
            GtsWildcard::new("gts.x.core.srr.resource.v1~acme.crm._.contact.v1~").expect("test");
        let broad = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        assert!(exact.overlaps(&broad));
        assert!(broad.overlaps(&exact));
    }

    #[test]
    fn test_overlaps_tilde_star_chain() {
        // "~*" pattern: any chained type under the base
        let base = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let sub = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.crm.*").expect("test");
        assert!(base.overlaps(&sub));
    }

    // ---- is_subset_of ----

    #[test]
    fn test_subset_narrow_is_subset_of_broad() {
        let broad = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let narrow = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.*").expect("test");
        assert!(narrow.is_subset_of(&broad));
        assert!(!broad.is_subset_of(&narrow));
    }

    #[test]
    fn test_subset_identical_patterns() {
        let a = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        assert!(a.is_subset_of(&b)); // identical ⊆ identical
        assert!(b.is_subset_of(&a));
    }

    #[test]
    fn test_subset_disjoint_not_subset() {
        let a = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let b = GtsWildcard::new("gts.x.core.other.resource.v1~*").expect("test");
        assert!(!a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));
    }

    #[test]
    fn test_subset_exact_is_subset_of_wildcard() {
        let exact =
            GtsWildcard::new("gts.x.core.srr.resource.v1~acme.crm._.contact.v1~").expect("test");
        let broad = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        assert!(exact.is_subset_of(&broad));
        assert!(!broad.is_subset_of(&exact));
    }

    #[test]
    fn test_subset_three_levels() {
        let l1 = GtsWildcard::new("gts.x.core.srr.resource.v1~*").expect("test");
        let l2 = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.*").expect("test");
        let l3 = GtsWildcard::new("gts.x.core.srr.resource.v1~acme.crm.*").expect("test");
        assert!(l3.is_subset_of(&l2));
        assert!(l3.is_subset_of(&l1));
        assert!(l2.is_subset_of(&l1));
        assert!(!l1.is_subset_of(&l2));
        assert!(!l1.is_subset_of(&l3));
        assert!(!l2.is_subset_of(&l3));
    }
}
