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

use crate::{
    GTS_ID_MAX_LENGTH, GTS_ID_PREFIX, GtsId, GtsIdError, GtsIdPatternSegment, GtsIdSegment,
};

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
    /// point. Input is validated as spelled, without trimming whitespace.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the string is not a valid GTS identifier pattern.
    pub fn try_new(pattern: &str) -> Result<Self, GtsIdError> {
        // Parse with the pattern parser (not GtsId::try_new, which rejects
        // wildcards) since a pattern is an identifier-shaped string that may
        // legitimately contain a trailing '*'.
        let segments = crate::parse::parse_pattern(pattern)?;

        Ok(GtsIdPattern {
            pattern: pattern.to_owned(),
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
    /// Every identifier selected by this candidate must be selected by the
    /// filter. This is exactly `pattern.covers(self)`, with the arguments in
    /// matching order rather than coverage order.
    #[must_use]
    pub fn matches_pattern(&self, pattern: &GtsIdPattern) -> bool {
        pattern.covers(self)
    }

    /// Returns `true` if this pattern matches the given candidate segments.
    ///
    /// This is the concrete-ID matching primitive used by
    /// [`GtsId::matches_pattern`]. A non-wildcard pattern segment must match
    /// exactly (with minor-version flexibility); a wildcard segment accepts
    /// anything from that point on. Pattern candidates use [`Self::covers`].
    ///
    /// [`GtsId::matches_pattern`]: crate::GtsId::matches_pattern
    pub(crate) fn matches_views(&self, candidate: &[GtsIdSegment]) -> bool {
        let pattern_segs = &self.segments;
        // A final bare `~*` may match an empty chain suffix. A wildcard that
        // already specifies part of the next segment (for example `~abc.*`)
        // still requires that segment to exist.
        let matches_empty_suffix = pattern_segs
            .last()
            .is_some_and(|seg| seg.is_wildcard() && seg.raw() == "*");
        let required_candidate_len = pattern_segs.len() - usize::from(matches_empty_suffix);
        if required_candidate_len > candidate.len() {
            return false;
        }

        for (i, p_seg) in pattern_segs.iter().enumerate() {
            if i == candidate.len() {
                return matches_empty_suffix && i == pattern_segs.len() - 1;
            }
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
                if let Some(p_major) = p_seg.ver_major_opt()
                    && Some(p_major) != c_seg.ver_major_opt()
                {
                    return false;
                }
                // `v1.*` requires an explicit minor in the concrete ID.
                if p_seg.ver_major_opt().is_some() && c_seg.ver_minor().is_none() {
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
            if p_seg.ver_major_opt() != c_seg.ver_major_opt() {
                return false;
            }

            // Minor version: if pattern has no minor version, accept any minor in candidate
            if let Some(p_minor) = p_seg.ver_minor()
                && Some(p_minor) != c_seg.ver_minor()
            {
                return false;
            }

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
    /// is contained within it. Coverage uses the already-parsed segments and
    /// respects minor-version flexibility, type/instance boundaries, and the
    /// identifier length limit. No pattern text is parsed again. Thus
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
        let value = Selection::of(other);
        let shortest = value.shortest_selected();
        shortest > GTS_ID_MAX_LENGTH
            || covers_nonempty(Selection::of(self), value, shortest == GTS_ID_MAX_LENGTH)
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

type Segment = GtsIdPatternSegment;

/// Borrowed views of the existing parse tree; no strings are parsed or copied.
#[derive(Clone, Copy)]
enum Selection<'a> {
    Instance {
        types: &'a [Segment],
        tail: &'a Segment,
        length: usize,
    },
    Prefixed {
        types: &'a [Segment],
        next: Option<&'a Segment>,
    },
}

impl<'a> Selection<'a> {
    fn of(pattern: &'a GtsIdPattern) -> Self {
        let segments = pattern.segments();
        // Validated patterns always have at least one segment.
        let end = segments.len() - 1;
        let last = &segments[end];
        if last.is_wildcard() {
            Self::Prefixed {
                types: &segments[..end],
                next: (last.raw() != "*").then_some(last),
            }
        } else if last.is_type() {
            Self::Prefixed {
                types: segments,
                next: None,
            }
        } else {
            Self::Instance {
                types: &segments[..end],
                tail: last,
                length: pattern.pattern().len(),
            }
        }
    }

    fn shortest_selected(self) -> usize {
        match self {
            Self::Instance { length, .. } => length,
            Self::Prefixed { types, next } => {
                let chain = GTS_ID_PREFIX.len()
                    + types
                        .iter()
                        .map(|segment| segment.raw().len())
                        .sum::<usize>();
                match next {
                    None if !types.is_empty() => chain,
                    next => {
                        // Four one-letter names, four dots, and `v0` need ten characters.
                        let segment = next.map_or(10, shortest_partial);
                        // A first segment must end in the type marker.
                        chain + segment + usize::from(types.is_empty())
                    }
                }
            }
        }
    }
}

fn shortest_partial(partial: &Segment) -> usize {
    if partial.ver_major_opt().is_some() {
        // In `v1.*`, replacing `*` with the shortest minor (`0`) keeps the length.
        partial.raw().len()
    } else {
        // Missing names are one character; `.v0` and the name separators add six.
        [
            partial.vendor(),
            partial.package(),
            partial.namespace(),
            partial.type_name(),
        ]
        .iter()
        .map(|name| name.len().max(1))
        .sum::<usize>()
            + 6
    }
}

fn covers_nonempty(operand: Selection<'_>, value: Selection<'_>, at_limit: bool) -> bool {
    match (operand, value) {
        (
            Selection::Instance {
                types: a,
                tail: a_tail,
                ..
            },
            Selection::Instance {
                types: b,
                tail: b_tail,
                ..
            },
        ) => {
            same_chain_covers(a, b)
                && match (a_tail.uuid_tail(), b_tail.uuid_tail()) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => segment_covers(a_tail, b_tail),
                    _ => false,
                }
        }
        (
            Selection::Instance { types: a, tail, .. },
            Selection::Prefixed {
                types: b,
                next: Some(partial),
            },
        ) if at_limit => {
            // Only one-digit instance minors fit; appending a type marker
            // or another chain segment would exceed the length limit.
            same_chain_covers(a, b)
                && tail.uuid_tail().is_none()
                && tail.ver_minor().is_none()
                && same_names(tail, partial)
                && partial.ver_major_opt() == tail.ver_major_opt()
        }
        (Selection::Instance { .. }, Selection::Prefixed { .. }) => false,
        (
            Selection::Prefixed { types, next },
            Selection::Instance {
                types: ids, tail, ..
            },
        ) => {
            ids.len() >= types.len()
                && types.iter().zip(ids).all(|(a, b)| segment_covers(a, b))
                && next.is_none_or(|partial| {
                    partial_admits(partial, ids.get(types.len()).unwrap_or(tail))
                })
        }
        (
            Selection::Prefixed {
                types: a,
                next: a_next,
            },
            Selection::Prefixed {
                types: b,
                next: b_next,
            },
        ) => prefixed_covers(a, a_next, b, b_next),
    }
}

fn same_names(a: &Segment, b: &Segment) -> bool {
    a.vendor() == b.vendor()
        && a.package() == b.package()
        && a.namespace() == b.namespace()
        && a.type_name() == b.type_name()
}

fn segment_covers(a: &Segment, b: &Segment) -> bool {
    same_names(a, b)
        && a.ver_major_opt() == b.ver_major_opt()
        && (a.ver_minor().is_none() || a.ver_minor() == b.ver_minor())
}

fn same_chain_covers(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| segment_covers(a, b))
}

fn partial_names_cover(a: &Segment, b: &Segment) -> bool {
    (a.vendor().is_empty() || a.vendor() == b.vendor())
        && (a.package().is_empty() || a.package() == b.package())
        && (a.namespace().is_empty() || a.namespace() == b.namespace())
        && (a.type_name().is_empty() || a.type_name() == b.type_name())
}

fn partial_admits(partial: &Segment, segment: &Segment) -> bool {
    partial_names_cover(partial, segment)
        && partial.ver_major_opt().is_none_or(|major| {
            segment.ver_major_opt() == Some(major) && segment.ver_minor().is_some()
        })
}

fn partial_covers(a: &Segment, b: &Segment) -> bool {
    partial_names_cover(a, b)
        && a.ver_major_opt()
            .is_none_or(|major| b.ver_major_opt() == Some(major))
}

fn prefixed_covers(
    a: &[Segment],
    a_next: Option<&Segment>,
    b: &[Segment],
    b_next: Option<&Segment>,
) -> bool {
    if a.len() > b.len() {
        // A first-segment minor wildcard selects types contained in a bare
        // type scope with the same names and major and an omitted minor.
        return match (a, a_next, b, b_next) {
            ([only], None, [], Some(partial)) => {
                only.ver_minor().is_none()
                    && same_names(only, partial)
                    && only.ver_major_opt() == partial.ver_major_opt()
            }
            _ => false,
        };
    }
    if !a.iter().zip(b).all(|(a, b)| segment_covers(a, b)) {
        return false;
    }
    let Some(partial) = a_next else {
        return true;
    };
    match b.get(a.len()) {
        Some(segment) => partial_admits(partial, segment),
        None => b_next.is_some_and(|other| partial_covers(partial, other)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "gts_id_pattern_test.rs"]
mod tests;
