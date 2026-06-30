//! The validated GTS identifier type.
//!
//! [`GtsId`] parses and validates a full GTS identifier string into its
//! constituent [`GtsIdSegment`]s. It also exposes the matching logic used to
//! test an ID against a [`GtsIdPattern`] pattern.

use std::fmt;
use std::str::FromStr;

use crate::parse::parse_id;
use crate::{GTS_ID_PREFIX, GtsIdError, GtsIdPattern, GtsIdSegment};

/// GTS ID - a validated Global Type System identifier.
///
/// GTS IDs follow the format: `gts.<vendor>.<package>.<namespace>.<type>.<version>[~]`
/// where `~` suffix indicates a type definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsId {
    id: String,
    segments: Vec<GtsIdSegment>,
}

impl GtsId {
    /// Parse and validate a concrete GTS identifier string.
    ///
    /// Wildcards are **not** accepted here — a `GtsId` is always a concrete
    /// identifier. Parse wildcard patterns with [`GtsIdPattern::try_new`] instead.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the string is not a valid concrete GTS identifier.
    pub fn try_new(id: &str) -> Result<Self, GtsIdError> {
        let raw = id.trim();

        // Delegate to the concrete parser (single source of truth). A `GtsId`
        // is always concrete, so its segments are `GtsIdSegment` (never wildcard).
        let segments = parse_id(raw)?;

        Ok(GtsId {
            id: raw.to_owned(),
            segments,
        })
    }

    /// The validated identifier string.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The parsed segments of this identifier.
    #[must_use]
    pub fn segments(&self) -> &[GtsIdSegment] {
        &self.segments
    }

    /// Consumes the identifier, returning its parsed segments.
    #[must_use]
    pub fn into_segments(self) -> Vec<GtsIdSegment> {
        self.segments
    }

    #[must_use]
    pub fn is_type(&self) -> bool {
        self.id.ends_with('~')
    }

    #[must_use]
    pub fn get_type_id(&self) -> Option<String> {
        if self.segments.len() < 2 {
            return None;
        }
        let segments: String = self.segments[..self.segments.len() - 1]
            .iter()
            .map(GtsIdSegment::raw)
            .collect::<Vec<_>>()
            .join("");
        Some(format!("{GTS_ID_PREFIX}{segments}"))
    }

    /// Returns all prefix IDs of this identifier, from the first segment up to
    /// and including the full ID.
    ///
    /// For a single-segment ID `gts.x.core.events.event.v1~` the result is
    /// `["gts.x.core.events.event.v1~"]`.
    /// For a three-segment chain `gts.A~B~C~` the result is
    /// `["gts.A~", "gts.A~B~", "gts.A~B~C~"]`.
    #[must_use]
    pub fn chain_ids(&self) -> Vec<String> {
        (1..=self.segments.len())
            .map(|n| {
                let joined: String = self.segments[..n]
                    .iter()
                    .map(GtsIdSegment::raw)
                    .collect::<Vec<_>>()
                    .join("");
                format!("{GTS_ID_PREFIX}{joined}")
            })
            .collect()
    }

    /// Generate a deterministic UUID v5 from this GTS ID.
    ///
    /// The UUID is derived from the validated identifier string under a fixed
    /// GTS namespace, so it is stable across processes and runs: the same ID
    /// always maps to the same UUID.
    ///
    /// Requires the `uuid` feature.
    #[cfg(feature = "uuid")]
    #[must_use]
    pub fn to_uuid(&self) -> uuid::Uuid {
        use std::sync::LazyLock;

        /// UUID v5 namespace for deterministic GTS identifier UUIDs.
        static GTS_NS: LazyLock<uuid::Uuid> =
            LazyLock::new(|| uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, b"gts"));

        uuid::Uuid::new_v5(&GTS_NS, self.id.as_bytes())
    }

    /// Check if a string is a valid GTS identifier.
    #[must_use]
    pub fn is_valid(s: &str) -> bool {
        Self::try_new(s).is_ok()
    }

    /// Check if this GTS ID matches a wildcard pattern.
    #[must_use]
    pub fn matches_pattern(&self, pattern: &GtsIdPattern) -> bool {
        pattern.matches_views(&self.segments)
    }

    /// Converts this concrete identifier into an equivalent zero-wildcard
    /// [`GtsIdPattern`].
    ///
    /// The conversion reuses the already validated segments, so it never
    /// re-parses and cannot fail. This is the ergonomic borrowing form of
    /// [`From<&GtsId>`](GtsIdPattern); the consuming form is
    /// `GtsIdPattern::from(id)`.
    ///
    /// The resulting pattern matches this id *and* everything derived from it
    /// down the chain: a base type id `gts.a.b.c.d.v1~` behaves as the implicit
    /// envelope `gts.a.b.c.d.v1~*` (GTS spec §3.6, "implicit derived-type
    /// coverage"), with the usual minor-version flexibility.
    #[must_use]
    pub fn to_pattern(&self) -> GtsIdPattern {
        self.into()
    }

    /// Splits a GTS ID with an optional attribute path.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the path is empty after the `@` separator.
    pub fn split_at_path(gts_with_path: &str) -> Result<(String, Option<String>), GtsIdError> {
        if !gts_with_path.contains('@') {
            return Ok((gts_with_path.to_owned(), None));
        }

        let parts: Vec<&str> = gts_with_path.splitn(2, '@').collect();
        let gts = parts[0].to_owned();
        let path = parts.get(1).map(|s| (*s).to_owned());

        if let Some(ref p) = path
            && p.is_empty()
        {
            return Err(GtsIdError::new(
                gts_with_path,
                "Attribute path cannot be empty",
            ));
        }

        Ok((gts, path))
    }
}

impl fmt::Display for GtsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl FromStr for GtsId {
    type Err = GtsIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl AsRef<str> for GtsId {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Prepend the configured prefix to a suffix, yielding a full GTS id string
    /// for use in tests. This keeps tests prefix-aware without hardcoding "gts.".
    fn gts_id(suffix: &str) -> String {
        format!("{GTS_ID_PREFIX}{suffix}")
    }

    #[test]
    fn test_gts_id_valid() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        assert_eq!(id.id, gts_id("x.core.events.event.v1~"));
        assert!(id.is_type());
        assert_eq!(id.segments.len(), 1);
    }

    #[test]
    fn test_gts_id_with_minor_version() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1.2~")).expect("test");
        assert_eq!(id.id, gts_id("x.core.events.event.v1.2~"));
        assert!(id.is_type());
        let seg = &id.segments[0];
        assert_eq!(seg.vendor(), "x");
        assert_eq!(seg.package(), "core");
        assert_eq!(seg.namespace(), "events");
        assert_eq!(seg.type_name(), "event");
        assert_eq!(seg.ver_major(), 1);
        assert_eq!(seg.ver_minor(), Some(2));
    }

    #[test]
    fn test_gts_id_instance() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~a.b.c.d.v1.0")).expect("test");
        assert_eq!(id.id, gts_id("x.core.events.event.v1~a.b.c.d.v1.0"));
        assert!(!id.is_type());
    }

    #[test]
    fn test_gts_id_invalid_uppercase() {
        let result = GtsId::try_new(&gts_id("X.core.events.event.v1~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_invalid_no_prefix() {
        let result = GtsId::try_new("x.core.events.event.v1~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_invalid_hyphen() {
        let result = GtsId::try_new(&gts_id("x-vendor.core.events.event.v1~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_type_id() {
        // get_type_id is for chained IDs - returns None for single segment
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let type_id = id.get_type_id();
        assert!(type_id.is_none());

        // For chained IDs, it returns the base type
        let chained =
            GtsId::try_new(&gts_id("x.core.events.type.v1~vendor.app._.custom.v1~")).expect("test");
        let base_type = chained.get_type_id();
        assert!(base_type.is_some());
        assert_eq!(base_type.expect("test"), gts_id("x.core.events.type.v1~"));
    }

    #[test]
    fn test_split_at_path() {
        let (gts, path) =
            GtsId::split_at_path(&gts_id("x.core.events.event.v1~@field.subfield")).expect("test");
        assert_eq!(gts, gts_id("x.core.events.event.v1~"));
        assert_eq!(path, Some("field.subfield".to_owned()));
    }

    #[test]
    fn test_split_at_path_no_path() {
        let (gts, path) = GtsId::split_at_path(&gts_id("x.core.events.event.v1~")).expect("test");
        assert_eq!(gts, gts_id("x.core.events.event.v1~"));
        assert_eq!(path, None);
    }

    #[test]
    fn test_split_at_path_empty_path_error() {
        let result = GtsId::split_at_path(&gts_id("x.core.events.event.v1~@"));
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid() {
        assert!(GtsId::is_valid(&gts_id("x.core.events.event.v1~")));
        assert!(!GtsId::is_valid("invalid"));
        assert!(!GtsId::is_valid(&gts_id("X.core.events.event.v1~")));
    }

    #[test]
    fn test_chained_identifiers() {
        let id = GtsId::try_new(&gts_id(
            "x.core.events.type.v1~vendor.app._.custom_event.v1~",
        ))
        .expect("test");
        assert_eq!(id.segments.len(), 2);
        assert_eq!(id.segments[0].vendor(), "x");
        assert_eq!(id.segments[1].vendor(), "vendor");
    }

    #[test]
    fn test_gts_id_with_underscore() {
        // Underscores are allowed in namespace
        let id = GtsId::try_new(&gts_id("x.core._.event.v1~")).expect("test");
        assert_eq!(id.segments[0].namespace(), "_");
    }

    #[test]
    fn test_gts_id_invalid_version_format() {
        let result = GtsId::try_new(&gts_id("x.core.events.event.vX~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_missing_segments() {
        let result = GtsId::try_new(&gts_id("x.core~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_empty_segment() {
        let result = GtsId::try_new(&gts_id("x..events.event.v1~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_split_at_path_multiple_at_signs() {
        // Should only split at first @
        let (gts, path) =
            GtsId::split_at_path(&gts_id("x.core.events.event.v1~@field@subfield")).expect("test");
        assert_eq!(gts, gts_id("x.core.events.event.v1~"));
        assert_eq!(path, Some("field@subfield".to_owned()));
    }

    #[test]
    fn test_gts_id_whitespace_trimming() {
        let id =
            GtsId::try_new(&format!("  {}  ", gts_id("x.core.events.event.v1~"))).expect("test");
        assert_eq!(id.id, gts_id("x.core.events.event.v1~"));
    }

    #[test]
    fn test_gts_id_long_chain() {
        let id = GtsId::try_new(&gts_id("a.b.c.d.v1~e.f.g.h.v2~i.j.k.l.v3~")).expect("test");
        assert_eq!(id.segments.len(), 3);
    }

    #[test]
    fn test_gts_id_version_without_minor() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        assert_eq!(id.segments[0].ver_major(), 1);
        assert_eq!(id.segments[0].ver_minor(), None);
    }

    #[test]
    fn test_gts_id_version_with_large_numbers() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v99.999~")).expect("test");
        assert_eq!(id.segments[0].ver_major(), 99);
        assert_eq!(id.segments[0].ver_minor(), Some(999));
    }

    #[test]
    fn test_gts_id_invalid_double_tilde() {
        let result = GtsId::try_new(&gts_id("x.core.events.event.v1~~"));
        assert!(result.is_err());
    }

    #[test]
    fn test_split_at_path_with_hash() {
        // Hash is not a separator, should be part of the ID
        let (gts, path) =
            GtsId::split_at_path(&gts_id("x.core.events.event.v1~#field")).expect("test");
        assert_eq!(gts, gts_id("x.core.events.event.v1~#field"));
        assert_eq!(path, None);
    }

    #[test]
    fn test_gts_id_display_trait() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        assert_eq!(format!("{id}"), gts_id("x.core.events.event.v1~"));
    }

    #[test]
    fn test_gts_id_from_str_trait() {
        let id: GtsId = gts_id("x.core.events.event.v1~").parse().expect("test");
        assert_eq!(id.id, gts_id("x.core.events.event.v1~"));
    }

    #[test]
    fn test_gts_id_as_ref_trait() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let s: &str = id.as_ref();
        assert_eq!(s, gts_id("x.core.events.event.v1~"));
    }

    #[test]
    fn test_gts_id_new_with_uri_prefix() {
        // Should reject gts:// prefix
        assert!(GtsId::try_new("gts://x.core.v1~").is_err());
        // The `gts:` form without slashes is equally invalid.
        assert!(GtsId::try_new("gts:x.core.v1~").is_err());
        // is_valid must agree with try_new on the gts:// form.
        assert!(!GtsId::is_valid("gts://x.core.v1~"));
    }

    #[test]
    fn test_gts_id_minimum_segments() {
        // Too few segments
        assert!(GtsId::try_new(&gts_id("~")).is_err());
        assert!(GtsId::try_new(&gts_id("x~")).is_err());
        assert!(GtsId::try_new(&gts_id("x.pkg~")).is_err());
        assert!(GtsId::try_new(&gts_id("x.pkg.ns~")).is_err());

        // Minimum valid (vendor.package.namespace.type.version)
        assert!(GtsId::try_new(&gts_id("x.pkg.ns.type.v1~")).is_ok());
    }

    #[test]
    fn test_gts_id_invalid_characters() {
        // Full IDs with only the target token malformed, so the assertions
        // exercise per-token character validation rather than failing earlier
        // on the segment-count check.
        assert!(GtsId::try_new(&gts_id("x.core.events.event!.v1~")).is_err());
        assert!(GtsId::try_new(&gts_id("x.core.events.ev$ent.v1~")).is_err());
        assert!(GtsId::try_new(&gts_id("x.core.events.ev ent.v1~")).is_err());
    }

    #[test]
    fn test_gts_id_uppercase_rejected() {
        assert!(GtsId::try_new(&gts_id("x.core.events.Test.v1~")).is_err());
        assert!(GtsId::try_new(&gts_id("X.core.events.test.v1~")).is_err());
    }

    #[test]
    fn test_gts_id_hyphen_rejected() {
        assert!(GtsId::try_new(&gts_id("x.core.events.test-name.v1~")).is_err());
    }

    #[test]
    fn test_gts_id_digit_start_segment() {
        // A token starting with a digit is invalid; use a full ID so the
        // start-character rule is reached rather than the segment-count check.
        assert!(GtsId::try_new(&gts_id("x.core.events.9test.v1~")).is_err());
    }

    #[test]
    fn test_gts_id_with_numbers_midword() {
        // Numbers in middle of segment are OK
        assert!(GtsId::try_new(&gts_id("x.test2name.ns.type.v1~")).is_ok());
        assert!(GtsId::try_new(&gts_id("x.pkg.item3.type.v1~")).is_ok());
    }

    #[test]
    fn test_split_at_path_valid_json_pointer() {
        let (gts, path) =
            GtsId::split_at_path(&gts_id("x.test.v1~@/properties/field")).expect("test");
        assert_eq!(gts, gts_id("x.test.v1~"));
        assert_eq!(path, Some("/properties/field".to_owned()));
    }

    #[test]
    fn test_gts_id_segment_start_underscore() {
        // A leading underscore is a *valid* token start ([a-z_]), so a full ID
        // whose type token begins with '_' parses successfully. (The previous
        // input "gts.x._private.event.v1~" only "passed" by failing the
        // segment-count check, masking this allowed-by-design behavior.)
        assert!(GtsId::try_new(&gts_id("x.core.events._private.v1~")).is_ok());
    }

    #[test]
    fn test_gts_id_multi_digit_versions() {
        // Multi-digit version numbers
        assert!(GtsId::try_new(&gts_id("x.pkg.ns.event.v10~")).is_ok());
        assert!(GtsId::try_new(&gts_id("x.pkg.ns.event.v1.20~")).is_ok());
    }

    #[test]
    fn test_gts_id_rejects_wildcards() {
        // A concrete `GtsId` never accepts wildcard patterns — those parse only
        // through `GtsIdPattern`. This is a deliberate tightening over the old
        // `gts::GtsID`, which delegated to `validate_gts_id(.., true)` and so
        // treated wildcard patterns as valid.
        assert!(GtsId::try_new(&gts_id("x.core.*")).is_err());
        assert!(GtsId::try_new(&gts_id("x.core.events.topic.v1~*")).is_err());
        assert!(!GtsId::is_valid(&gts_id("x.core.*")));
        assert!(!GtsId::is_valid(&gts_id("x.core.events.topic.v1~*")));

        // The same strings are valid as wildcard patterns.
        assert!(GtsIdPattern::try_new(&gts_id("x.core.*")).is_ok());
        assert!(GtsIdPattern::try_new(&gts_id("x.core.events.topic.v1~*")).is_ok());
    }

    #[test]
    fn test_wildcard_must_be_terminal_and_not_a_type() {
        // `*` is only ever the last token of a pattern, and a wildcard is never
        // a type segment: `*~` is rejected (it neither ends in `.*` nor `~*`).
        for pattern in [
            gts_id("x.core.*~"),
            gts_id("x.core.events.topic.v1.*~"),
            gts_id("x.*.events.topic.v1~"), // `*` not terminal
        ] {
            assert!(
                GtsIdPattern::try_new(&pattern).is_err(),
                "pattern must be rejected: {pattern}"
            );
        }
    }

    #[test]
    fn test_get_type_id_multi_segment_chain() {
        // Regression: reconstructing the parent type id must join the raw
        // segments (which already carry their trailing `~`) directly, never
        // re-inserting `~` between them. A three-segment chain has two parent
        // segments, which is exactly where a `join("~")` would produce `~~`.
        let id = GtsId::try_new(&gts_id(
            "x.core.events.topic.v1~vendor.app.orders.thing.v1~acme.shop.checkout.item.v1.0",
        ))
        .expect("valid three-segment chain");

        let parent = id.get_type_id().expect("chain has a parent type id");
        assert_eq!(
            parent,
            gts_id("x.core.events.topic.v1~vendor.app.orders.thing.v1~")
        );
        assert!(!parent.contains("~~"), "parent id must not contain '~~'");

        // A two-segment chain has a single parent segment.
        let id = GtsId::try_new(&gts_id(
            "x.core.events.topic.v1~vendor.app.orders.thing.v1.0",
        ))
        .expect("valid two-segment chain");
        assert_eq!(
            id.get_type_id().expect("parent"),
            gts_id("x.core.events.topic.v1~")
        );

        // A single segment has no parent.
        let id = GtsId::try_new(&gts_id("x.core.events.topic.v1~")).expect("single type segment");
        assert_eq!(id.get_type_id(), None);
    }

    #[test]
    fn test_chain_ids_single_segment() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let chain = id.chain_ids();
        assert_eq!(chain, vec![gts_id("x.core.events.event.v1~")]);
    }

    #[test]
    fn test_chain_ids_multi_segment() {
        let id = GtsId::try_new(&gts_id(
            "x.core.events.topic.v1~vendor.app.orders.thing.v1~acme.shop.checkout.item.v1.0",
        ))
        .expect("valid three-segment chain");
        let chain = id.chain_ids();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0], gts_id("x.core.events.topic.v1~"));
        assert_eq!(
            chain[1],
            gts_id("x.core.events.topic.v1~vendor.app.orders.thing.v1~")
        );
        assert_eq!(
            chain[2],
            gts_id(
                "x.core.events.topic.v1~vendor.app.orders.thing.v1~acme.shop.checkout.item.v1.0"
            )
        );
        // No double tildes
        assert!(chain.iter().all(|c| !c.contains("~~")));
    }

    #[test]
    fn test_to_pattern_roundtrip() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let pattern = id.to_pattern();
        assert_eq!(pattern.pattern(), id.id());
        // The id at minimum matches the pattern derived from itself (it also
        // covers derived chains — see gts_id_pattern's coverage test).
        assert!(id.matches_pattern(&pattern));
    }

    #[test]
    fn test_to_pattern_instance_id() {
        // Works for a chained instance id too — every segment is carried over.
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~a.b.c.d.v1.0")).expect("test");
        let pattern = id.to_pattern();
        assert_eq!(pattern.pattern(), id.id());
        assert_eq!(pattern.segments().len(), id.segments().len());
        assert!(id.matches_pattern(&pattern));
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn test_uuid_generation() {
        let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let uuid1 = id.to_uuid();
        let uuid2 = id.to_uuid();
        // UUIDs should be deterministic
        assert_eq!(uuid1, uuid2);
        assert_eq!(uuid1.to_string(), "154302ad-df5c-56e6-97d4-f87c5faca44b");
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn test_uuid_different_ids() {
        let id1 = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
        let id2 = GtsId::try_new(&gts_id("x.core.events.event.v2~")).expect("test");
        assert_ne!(id1.to_uuid(), id2.to_uuid());
    }
}
