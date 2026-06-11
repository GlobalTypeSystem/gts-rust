//! The validated GTS identifier type.
//!
//! [`GtsID`] parses and validates a full GTS identifier string into its
//! constituent [`GtsIdSegment`]s. It also exposes the matching logic used to
//! test an ID against a [`GtsWildcard`] pattern.

use std::fmt;
use std::str::FromStr;

use crate::parse::parse_gts_string;
use crate::{GTS_PREFIX, GtsIdError, GtsIdSegment, GtsWildcard};

/// GTS ID - a validated Global Type System identifier.
///
/// GTS IDs follow the format: `gts.<vendor>.<package>.<namespace>.<type>.<version>[~]`
/// where `~` suffix indicates a type/schema definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsID {
    id: String,
    segments: Vec<GtsIdSegment>,
}

impl GtsID {
    /// Parse and validate a concrete GTS identifier string.
    ///
    /// Wildcards are **not** accepted here — a `GtsID` is always a concrete
    /// identifier. Parse wildcard patterns with [`GtsWildcard::new`] instead.
    ///
    /// # Errors
    /// Returns `GtsIdError` if the string is not a valid concrete GTS identifier.
    pub fn new(id: &str) -> Result<Self, GtsIdError> {
        let raw = id.trim();

        // Delegate all parsing to the shared parser (single source of truth).
        // `allow_wildcards = false`: a concrete identifier never contains '*'.
        let segments = parse_gts_string(raw, false)?;

        Ok(GtsID {
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
            .map(|s| s.segment.as_str())
            .collect::<Vec<_>>()
            .join("");
        Some(format!("{GTS_PREFIX}{segments}"))
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
        if !s.starts_with(GTS_PREFIX) {
            return false;
        }
        Self::new(s).is_ok()
    }

    /// Check if this GTS ID matches a wildcard pattern.
    #[must_use]
    pub fn wildcard_match(&self, pattern: &GtsWildcard) -> bool {
        pattern.matches_segments(&self.segments)
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

impl fmt::Display for GtsID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl FromStr for GtsID {
    type Err = GtsIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for GtsID {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_gts_id_valid() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1~");
        assert!(id.is_type());
        assert_eq!(id.segments.len(), 1);
    }

    #[test]
    fn test_gts_id_with_minor_version() {
        let id = GtsID::new("gts.x.core.events.event.v1.2~").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1.2~");
        assert!(id.is_type());
        let seg = &id.segments[0];
        assert_eq!(seg.vendor, "x");
        assert_eq!(seg.package, "core");
        assert_eq!(seg.namespace, "events");
        assert_eq!(seg.type_name, "event");
        assert_eq!(seg.ver_major, 1);
        assert_eq!(seg.ver_minor, Some(2));
    }

    #[test]
    fn test_gts_id_instance() {
        let id = GtsID::new("gts.x.core.events.event.v1~a.b.c.d.v1.0").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1~a.b.c.d.v1.0");
        assert!(!id.is_type());
    }

    #[test]
    fn test_gts_id_invalid_uppercase() {
        let result = GtsID::new("gts.X.core.events.event.v1~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_invalid_no_prefix() {
        let result = GtsID::new("x.core.events.event.v1~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_invalid_hyphen() {
        let result = GtsID::new("gts.x-vendor.core.events.event.v1~");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_type_id() {
        // get_type_id is for chained IDs - returns None for single segment
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let type_id = id.get_type_id();
        assert!(type_id.is_none());

        // For chained IDs, it returns the base type
        let chained =
            GtsID::new("gts.x.core.events.type.v1~vendor.app._.custom.v1~").expect("test");
        let base_type = chained.get_type_id();
        assert!(base_type.is_some());
        assert_eq!(base_type.expect("test"), "gts.x.core.events.type.v1~");
    }

    #[test]
    fn test_split_at_path() {
        let (gts, path) =
            GtsID::split_at_path("gts.x.core.events.event.v1~@field.subfield").expect("test");
        assert_eq!(gts, "gts.x.core.events.event.v1~");
        assert_eq!(path, Some("field.subfield".to_owned()));
    }

    #[test]
    fn test_split_at_path_no_path() {
        let (gts, path) = GtsID::split_at_path("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(gts, "gts.x.core.events.event.v1~");
        assert_eq!(path, None);
    }

    #[test]
    fn test_split_at_path_empty_path_error() {
        let result = GtsID::split_at_path("gts.x.core.events.event.v1~@");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid() {
        assert!(GtsID::is_valid("gts.x.core.events.event.v1~"));
        assert!(!GtsID::is_valid("invalid"));
        assert!(!GtsID::is_valid("gts.X.core.events.event.v1~"));
    }

    #[test]
    fn test_chained_identifiers() {
        let id =
            GtsID::new("gts.x.core.events.type.v1~vendor.app._.custom_event.v1~").expect("test");
        assert_eq!(id.segments.len(), 2);
        assert_eq!(id.segments[0].vendor, "x");
        assert_eq!(id.segments[1].vendor, "vendor");
    }

    #[test]
    fn test_gts_id_with_underscore() {
        // Underscores are allowed in namespace
        let id = GtsID::new("gts.x.core._.event.v1~").expect("test");
        assert_eq!(id.segments[0].namespace, "_");
    }

    #[test]
    fn test_gts_id_invalid_version_format() {
        let result = GtsID::new("gts.x.core.events.event.vX~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_missing_segments() {
        let result = GtsID::new("gts.x.core~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_id_empty_segment() {
        let result = GtsID::new("gts.x..events.event.v1~");
        assert!(result.is_err());
    }

    #[test]
    fn test_split_at_path_multiple_at_signs() {
        // Should only split at first @
        let (gts, path) =
            GtsID::split_at_path("gts.x.core.events.event.v1~@field@subfield").expect("test");
        assert_eq!(gts, "gts.x.core.events.event.v1~");
        assert_eq!(path, Some("field@subfield".to_owned()));
    }

    #[test]
    fn test_gts_id_whitespace_trimming() {
        let id = GtsID::new("  gts.x.core.events.event.v1~  ").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1~");
    }

    #[test]
    fn test_gts_id_long_chain() {
        let id = GtsID::new("gts.a.b.c.d.v1~e.f.g.h.v2~i.j.k.l.v3~").expect("test");
        assert_eq!(id.segments.len(), 3);
    }

    #[test]
    fn test_gts_id_version_without_minor() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(id.segments[0].ver_major, 1);
        assert_eq!(id.segments[0].ver_minor, None);
    }

    #[test]
    fn test_gts_id_version_with_large_numbers() {
        let id = GtsID::new("gts.x.core.events.event.v99.999~").expect("test");
        assert_eq!(id.segments[0].ver_major, 99);
        assert_eq!(id.segments[0].ver_minor, Some(999));
    }

    #[test]
    fn test_gts_id_invalid_double_tilde() {
        let result = GtsID::new("gts.x.core.events.event.v1~~");
        assert!(result.is_err());
    }

    #[test]
    fn test_split_at_path_with_hash() {
        // Hash is not a separator, should be part of the ID
        let (gts, path) = GtsID::split_at_path("gts.x.core.events.event.v1~#field").expect("test");
        assert_eq!(gts, "gts.x.core.events.event.v1~#field");
        assert_eq!(path, None);
    }

    #[test]
    fn test_gts_id_display_trait() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(format!("{id}"), "gts.x.core.events.event.v1~");
    }

    #[test]
    fn test_gts_id_from_str_trait() {
        let id: GtsID = "gts.x.core.events.event.v1~".parse().expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1~");
    }

    #[test]
    fn test_gts_id_as_ref_trait() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let s: &str = id.as_ref();
        assert_eq!(s, "gts.x.core.events.event.v1~");
    }

    #[test]
    fn test_gts_id_new_with_uri_prefix() {
        // Should reject gts:// prefix
        assert!(GtsID::new("gts://x.core.v1~").is_err());
    }

    #[test]
    fn test_gts_id_minimum_segments() {
        // Too few segments
        assert!(GtsID::new("gts~").is_err());
        assert!(GtsID::new("gts.x~").is_err());
        assert!(GtsID::new("gts.x.pkg~").is_err());
        assert!(GtsID::new("gts.x.pkg.ns~").is_err());

        // Minimum valid (vendor.package.namespace.type.version)
        assert!(GtsID::new("gts.x.pkg.ns.type.v1~").is_ok());
    }

    #[test]
    fn test_gts_id_invalid_characters() {
        assert!(GtsID::new("gts.x.test!.v1~").is_err());
        assert!(GtsID::new("gts.x.te$t.v1~").is_err());
        assert!(GtsID::new("gts.x.te st.v1~").is_err());
    }

    #[test]
    fn test_gts_id_uppercase_rejected() {
        assert!(GtsID::new("gts.x.Test.v1~").is_err());
        assert!(GtsID::new("gts.X.test.v1~").is_err());
    }

    #[test]
    fn test_gts_id_hyphen_rejected() {
        assert!(GtsID::new("gts.x.test-name.v1~").is_err());
    }

    #[test]
    fn test_gts_id_digit_start_segment() {
        // Digits at start of segment
        assert!(GtsID::new("gts.x.9test.v1~").is_err());
    }

    #[test]
    fn test_gts_id_with_numbers_midword() {
        // Numbers in middle of segment are OK
        assert!(GtsID::new("gts.x.test2name.ns.type.v1~").is_ok());
        assert!(GtsID::new("gts.x.pkg.item3.type.v1~").is_ok());
    }

    #[test]
    fn test_split_at_path_valid_json_pointer() {
        let (gts, path) = GtsID::split_at_path("gts.x.test.v1~@/properties/field").expect("test");
        assert_eq!(gts, "gts.x.test.v1~");
        assert_eq!(path, Some("/properties/field".to_owned()));
    }

    #[test]
    fn test_gts_id_segment_start_underscore() {
        // Underscore at start is invalid
        assert!(GtsID::new("gts.x._private.event.v1~").is_err());
    }

    #[test]
    fn test_gts_id_multi_digit_versions() {
        // Multi-digit version numbers
        assert!(GtsID::new("gts.x.pkg.ns.event.v10~").is_ok());
        assert!(GtsID::new("gts.x.pkg.ns.event.v1.20~").is_ok());
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn test_uuid_generation() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let uuid1 = id.to_uuid();
        let uuid2 = id.to_uuid();
        // UUIDs should be deterministic
        assert_eq!(uuid1, uuid2);
        assert!(!uuid1.to_string().is_empty());
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn test_uuid_different_ids() {
        let id1 = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let id2 = GtsID::new("gts.x.core.events.event.v2~").expect("test");
        assert_ne!(id1.to_uuid(), id2.to_uuid());
    }
}
