//! GTS identifier types.
//!
//! The low-level identifier primitives ([`GtsId`], [`GtsIdSegment`],
//! [`GtsIdWildcard`]) and all validation live in the [`gts_id`] crate and are
//! re-exported here. The typed, schema-aware wrappers [`GtsTypeId`] and
//! [`GtsInstanceId`] live in this crate because they carry the JSON Schema
//! integration (`serde`, `schemars`, [`GtsTypeId::json_schema_value`]), which is
//! a `gts` (schema) concern rather than a pure identifier concern. They are
//! built on the crate-private [`GtsEntityId`] string newtype, which is plumbing
//! for those wrappers and intentionally not part of the public API.
//!
//! [`GtsIdError`] — the single error type shared across all GTS identifier and
//! wildcard parsing — is re-exported here from the [`gts_id`] crate.

use std::fmt;

pub use gts_id::{GTS_PREFIX, GtsId, GtsIdError, GtsIdSegment, GtsIdSegmentParts, GtsIdWildcard};

/// A type-safe wrapper for GTS entity identifiers.
///
/// `GtsEntityId` wraps a fully-formed GTS entity ID string (e.g.,
/// `gts.x.core.events.topic.v1~vendor.app.orders.v1.0`). It is crate-private
/// plumbing for the schema-aware wrappers ([`GtsTypeId`], [`GtsInstanceId`]) and
/// performs no validation itself — validation lives in their `try_new`
/// constructors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct GtsEntityId(String);

impl GtsEntityId {
    /// Creates a new GTS entity ID from a string, without validation.
    fn new(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// Returns the underlying string representation of the entity ID.
    fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for GtsEntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for GtsEntityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// URI-compatible prefix for GTS identifiers in JSON Schema `$id` field (e.g., `gts://gts.x.y.z...`).
/// This is ONLY used for JSON Schema serialization/deserialization, not for GTS ID parsing.
pub const GTS_URI_PREFIX: &str = "gts://";

/// A type-safe wrapper for GTS instance identifiers.
///
/// `GtsInstanceId` wraps a fully-formed GTS instance ID string (e.g.,
/// `gts.x.core.events.topic.v1~vendor.app.orders.v1.0`). It can be used as a map key,
/// compared for equality, hashed, and serialized/deserialized.
///
/// # Example
///
/// ```
/// use gts::GtsInstanceId;
///
/// let id = GtsInstanceId::new("gts.x.core.events.topic.v1~", "vendor.app.orders.v1.0");
/// assert_eq!(id.as_ref(), "gts.x.core.events.topic.v1~vendor.app.orders.v1.0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsInstanceId(GtsEntityId);

impl serde::Serialize for GtsInstanceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de> serde::Deserialize<'de> for GtsInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(GtsInstanceId(GtsEntityId::new(&s)))
    }
}

impl schemars::JsonSchema for GtsInstanceId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("GtsInstanceId")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Build inline schema to prevent $defs reference generation
        // This matches the old schemars 0.8 behavior where is_referenceable() returned false
        // We create the schema as JSON and convert it to avoid using private schema module
        let json = Self::json_schema_value();
        let mut schema_json = serde_json::json!({
            "type": "string"
        });

        if let Some(format) = json.get("format") {
            schema_json["format"] = format.clone();
        }
        if let Some(title) = json.get("title") {
            schema_json["title"] = title.clone();
        }
        if let Some(description) = json.get("description") {
            schema_json["description"] = description.clone();
        }
        if let Some(gts_ref) = json.get("x-gts-ref") {
            schema_json["x-gts-ref"] = gts_ref.clone();
        }

        // Convert JSON to Schema using TryFrom
        schema_json.try_into().unwrap_or_default()
    }
}

impl GtsInstanceId {
    /// Returns the JSON Schema representation of `GtsInstanceId` as a `serde_json::Value`.
    ///
    /// This is the canonical schema definition used by both the schemars `JsonSchema` impl
    /// and the CLI schema generator, ensuring consistency.
    ///
    /// # Example
    /// ```
    /// use gts::GtsInstanceId;
    ///
    /// let schema = GtsInstanceId::json_schema_value();
    /// assert_eq!(schema["type"], "string");
    /// assert_eq!(schema["format"], "gts-instance-id");
    /// assert_eq!(schema["x-gts-ref"], "gts.*");
    /// ```
    #[must_use]
    pub fn json_schema_value() -> serde_json::Value {
        serde_json::json!({
            "type": "string",
            "format": "gts-instance-id",
            "title": "GTS Instance ID",
            "description": "GTS instance identifier",
            "x-gts-ref": "gts.*"
        })
    }

    /// Creates a new GTS instance ID by combining a schema ID with a segment.
    ///
    /// # Arguments
    ///
    /// * `schema_id` - The GTS schema ID (e.g., `gts.x.core.events.topic.v1~`)
    /// * `segment` - The instance segment to append (e.g., `vendor.app.orders.v1.0`)
    ///
    /// # Returns
    ///
    /// A new `GtsInstanceId` containing the concatenated ID.
    #[must_use]
    pub fn new(schema_id: &str, segment: &str) -> Self {
        Self(GtsEntityId::new(&format!("{schema_id}{segment}")))
    }

    /// Creates a new GTS instance ID from a fully-formed string, validating
    /// that it is a well-formed *instance* identifier.
    ///
    /// Unlike the infallible [`GtsInstanceId::new`], this constructor enforces
    /// the instance/type discrimination via the type system rather than relying
    /// on downstream `ends_with('~')` string checks. The string is first parsed
    /// and structurally validated via [`GtsId::new`], then classified:
    ///
    /// * it must parse as a valid GTS identifier, and
    /// * it must **not** be a type id (a trailing `~` denotes a type id).
    ///
    /// A successfully parsed instance id is always chained with at least one
    /// type segment (single-segment instance ids are rejected by [`GtsId::new`]),
    /// so it necessarily contains `~`.
    ///
    /// # Errors
    /// Returns the underlying [`GtsIdError`] if the string fails GTS ID
    /// validation, or [`GtsIdError`] if it is a (trailing-`~`) type id.
    ///
    /// # Example
    /// ```
    /// use gts::GtsInstanceId;
    ///
    /// assert!(GtsInstanceId::try_new("gts.x.core.events.event.v1~a.b.c.d.v1.0").is_ok());
    /// // Trailing '~' is a type id, not an instance id:
    /// assert!(GtsInstanceId::try_new("gts.x.core.events.event.v1~").is_err());
    /// // A bare single-segment id is not a chained instance id:
    /// assert!(GtsInstanceId::try_new("gts.x.core.events.event.v1").is_err());
    /// ```
    pub fn try_new(instance_id: &str) -> Result<Self, GtsIdError> {
        let parsed = GtsId::new(instance_id)?;
        if parsed.is_type() {
            return Err(GtsIdError::new(
                instance_id,
                "GTS instance IDs must not end with '~' (a trailing '~' denotes a type id)",
            ));
        }
        Ok(Self(GtsEntityId::new(parsed.as_ref())))
    }

    /// Returns the underlying string representation of the instance ID.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0.into_string()
    }
}

impl fmt::Display for GtsInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for GtsInstanceId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<GtsInstanceId> for String {
    fn from(id: GtsInstanceId) -> Self {
        id.0.into_string()
    }
}

impl std::ops::Deref for GtsInstanceId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl PartialEq<str> for GtsInstanceId {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<&str> for GtsInstanceId {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl PartialEq<String> for GtsInstanceId {
    fn eq(&self, other: &String) -> bool {
        self.0.as_ref() == other
    }
}

/// A type-safe wrapper for GTS type identifiers (formerly schema IDs).
///
/// `GtsTypeId` wraps a fully-formed GTS type ID string (e.g.,
/// `gts.x.core.events.topic.v1~`). It can be used as a map key,
/// compared for equality, hashed, and serialized/deserialized.
///
/// # Example
///
/// ```
/// use gts::GtsTypeId;
///
/// let id = GtsTypeId::new("gts.x.core.events.topic.v1~vendor.app.orders.v1.0~");
/// assert_eq!(id.as_ref(), "gts.x.core.events.topic.v1~vendor.app.orders.v1.0~");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsTypeId(GtsEntityId);

/// Deprecated alias retained for v0.10 callers. New code should use [`GtsTypeId`].
#[deprecated(since = "0.10.0", note = "renamed to `GtsTypeId`")]
pub type GtsSchemaId = GtsTypeId;

impl serde::Serialize for GtsTypeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de> serde::Deserialize<'de> for GtsTypeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(GtsTypeId(GtsEntityId::new(&s)))
    }
}

impl schemars::JsonSchema for GtsTypeId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("GtsTypeId")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Build inline schema to prevent $defs reference generation
        // This matches the old schemars 0.8 behavior where is_referenceable() returned false
        // We create the schema as JSON and convert it to avoid using private schema module
        let json = Self::json_schema_value();
        let mut schema_json = serde_json::json!({
            "type": "string"
        });

        if let Some(format) = json.get("format") {
            schema_json["format"] = format.clone();
        }
        if let Some(title) = json.get("title") {
            schema_json["title"] = title.clone();
        }
        if let Some(description) = json.get("description") {
            schema_json["description"] = description.clone();
        }
        if let Some(gts_ref) = json.get("x-gts-ref") {
            schema_json["x-gts-ref"] = gts_ref.clone();
        }

        // Convert JSON to Schema using TryFrom
        schema_json.try_into().unwrap_or_default()
    }
}

impl GtsTypeId {
    /// Returns the JSON Schema representation of `GtsTypeId` as a `serde_json::Value`.
    ///
    /// This is the canonical schema definition used by both the schemars `JsonSchema` impl
    /// and the CLI schema generator, ensuring consistency.
    ///
    /// # Example
    /// ```
    /// use gts::GtsTypeId;
    ///
    /// let schema = GtsTypeId::json_schema_value();
    /// assert_eq!(schema["type"], "string");
    /// assert_eq!(schema["format"], "gts-type-id");
    /// assert_eq!(schema["x-gts-ref"], "gts.*");
    /// ```
    #[must_use]
    pub fn json_schema_value() -> serde_json::Value {
        serde_json::json!({
            "type": "string",
            "format": "gts-type-id",
            "title": "GTS Type ID",
            "description": "GTS type identifier",
            "x-gts-ref": "gts.*"
        })
    }

    /// Creates a new GTS type ID from string.
    ///
    /// # Arguments
    ///
    /// * `type_id` - The GTS type ID (e.g., `gts.x.core.events.topic.v1~`)
    ///
    /// # Returns
    ///
    /// A new `GtsTypeId` containing the concatenated ID.
    #[must_use]
    pub fn new(type_id: &str) -> Self {
        Self(GtsEntityId::new(type_id))
    }

    /// Creates a new GTS type ID from a string, validating that it is a
    /// well-formed *type* identifier.
    ///
    /// Unlike the infallible [`GtsTypeId::new`], this constructor enforces the
    /// type/instance discrimination via the type system rather than relying on
    /// downstream `ends_with('~')` string checks. The string is first parsed and
    /// structurally validated via [`GtsId::new`], then classified:
    ///
    /// * it must parse as a valid GTS identifier, and
    /// * it must be a type id (i.e. end with `~`).
    ///
    /// # Errors
    /// Returns the underlying [`GtsIdError`] if the string fails GTS ID
    /// validation, or [`GtsIdError`] if it is an instance id (no trailing `~`).
    ///
    /// # Example
    /// ```
    /// use gts::GtsTypeId;
    ///
    /// assert!(GtsTypeId::try_new("gts.x.core.events.event.v1~").is_ok());
    /// // An instance id (no trailing '~') is not a type id:
    /// assert!(GtsTypeId::try_new("gts.x.core.events.event.v1~a.b.c.d.v1.0").is_err());
    /// ```
    pub fn try_new(type_id: &str) -> Result<Self, GtsIdError> {
        let parsed = GtsId::new(type_id)?;
        if !parsed.is_type() {
            return Err(GtsIdError::new(type_id, "GTS type IDs must end with '~'"));
        }
        Ok(Self(GtsEntityId::new(parsed.as_ref())))
    }

    /// Returns the underlying string representation of the type ID.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0.into_string()
    }
}

impl fmt::Display for GtsTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for GtsTypeId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<GtsTypeId> for String {
    fn from(id: GtsTypeId) -> Self {
        id.0.into_string()
    }
}

impl std::ops::Deref for GtsTypeId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl PartialEq<str> for GtsTypeId {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<&str> for GtsTypeId {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl PartialEq<String> for GtsTypeId {
    fn eq(&self, other: &String) -> bool {
        self.0.as_ref() == other
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_type_id_try_new_accepts_type_rejects_instance() {
        // A trailing-'~' type id is accepted.
        let id = GtsTypeId::try_new("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(id.as_ref(), "gts.x.core.events.event.v1~");

        // A valid instance id (no trailing '~') is classified out.
        let err = GtsTypeId::try_new("gts.x.core.events.event.v1~a.b.c.d.v1.0")
            .expect_err("must reject instance id");
        assert!(err.to_string().contains("must end with '~'"));

        // A wholly invalid CTI is rejected by GtsId::new before classification.
        assert!(GtsTypeId::try_new("not a valid cti~").is_err());
    }

    #[test]
    fn test_instance_id_try_new_accepts_instance_rejects_type() {
        // A chained, non-type id is accepted.
        let id = GtsInstanceId::try_new("gts.x.core.events.event.v1~a.b.c.d.v1.0").expect("test");
        assert_eq!(id.as_ref(), "gts.x.core.events.event.v1~a.b.c.d.v1.0");

        // A valid type id (trailing '~') is classified out.
        let err =
            GtsInstanceId::try_new("gts.x.core.events.event.v1~").expect_err("must reject type id");
        assert!(err.to_string().contains("must not end with '~'"));

        // A wholly invalid CTI is rejected by GtsId::new before classification.
        assert!(GtsInstanceId::try_new("not a valid cti").is_err());
    }
}
