use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;
use thiserror::Error;
use uuid::Uuid;

pub const GTS_PREFIX: &str = "gts.";
/// URI-compatible prefix for GTS identifiers in JSON Schema `$id` field (e.g., `gts://gts.x.y.z...`).
/// This is ONLY used for JSON Schema serialization/deserialization, not for GTS ID parsing.
pub const GTS_URI_PREFIX: &str = "gts://";
static GTS_NS: LazyLock<Uuid> = LazyLock::new(|| Uuid::new_v5(&Uuid::NAMESPACE_URL, b"gts"));

/// Validates a GTS segment token without regex for better performance.
/// Valid tokens: start with [a-z_], followed by [a-z0-9_]*
#[inline]
fn is_valid_segment_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let mut chars = token.chars();
    // First character must be [a-z_]
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    // Remaining characters must be [a-z0-9_]
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[derive(Debug, Error)]
pub enum GtsIdError {
    #[error("Invalid GTS segment #{num} @ offset {offset}: '{segment}': {cause}")]
    Segment {
        num: usize,
        offset: usize,
        segment: String,
        cause: String,
    },

    #[error("Invalid GTS identifier: {id}: {cause}")]
    Id { id: String, cause: String },

    #[error("Invalid GTS wildcard pattern: {pattern}: {cause}")]
    Wildcard { pattern: String, cause: String },
}

/// Parsed GTS segment containing vendor, package, namespace, type, and version info.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsIdSegment {
    pub num: usize,
    pub offset: usize,
    pub segment: String,
    pub vendor: String,
    pub package: String,
    pub namespace: String,
    pub type_name: String,
    pub ver_major: u32,
    pub ver_minor: Option<u32>,
    pub is_type: bool,
    pub is_wildcard: bool,
}

impl GtsIdSegment {
    /// Creates a new GTS ID segment from a string.
    ///
    /// # Errors
    /// Returns `GtsIdError::Segment` if the segment string is invalid.
    pub fn new(num: usize, offset: usize, segment: &str) -> Result<Self, GtsIdError> {
        let segment = segment.trim().to_owned();
        let mut seg = GtsIdSegment {
            num,
            offset,
            segment: segment.clone(),
            vendor: String::new(),
            package: String::new(),
            namespace: String::new(),
            type_name: String::new(),
            ver_major: 0,
            ver_minor: None,
            is_type: false,
            is_wildcard: false,
        };

        seg.parse_segment_id(&segment)?;
        Ok(seg)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_segment_id(&mut self, segment: &str) -> Result<(), GtsIdError> {
        let mut segment = segment.to_owned();

        // Check for type marker
        if segment.contains('~') {
            let tilde_count = segment.matches('~').count();
            if tilde_count > 1 {
                return Err(GtsIdError::Segment {
                    num: self.num,
                    offset: self.offset,
                    segment: self.segment.clone(),
                    cause: "Too many '~' characters".to_owned(),
                });
            }
            if segment.ends_with('~') {
                self.is_type = true;
                segment.pop();
            } else {
                return Err(GtsIdError::Segment {
                    num: self.num,
                    offset: self.offset,
                    segment: self.segment.clone(),
                    cause: " '~' must be at the end".to_owned(),
                });
            }
        }

        let tokens: Vec<&str> = segment.split('.').collect();

        if tokens.len() > 6 {
            return Err(GtsIdError::Segment {
                num: self.num,
                offset: self.offset,
                segment: self.segment.clone(),
                cause: "Too many tokens".to_owned(),
            });
        }

        if !segment.ends_with('*') && tokens.len() < 5 {
            return Err(GtsIdError::Segment {
                num: self.num,
                offset: self.offset,
                segment: self.segment.clone(),
                cause: "Too few tokens".to_owned(),
            });
        }

        // Validate tokens (except version tokens)
        if !segment.ends_with('*') {
            for (i, token) in tokens.iter().take(4).enumerate() {
                if !is_valid_segment_token(token) {
                    return Err(GtsIdError::Segment {
                        num: self.num,
                        offset: self.offset,
                        segment: self.segment.clone(),
                        cause: format!("Invalid segment token: {}", tokens[i]),
                    });
                }
            }
        }

        // Parse tokens
        if !tokens.is_empty() {
            if tokens[0] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }
            tokens[0].clone_into(&mut self.vendor);
        }

        if tokens.len() > 1 {
            if tokens[1] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }
            tokens[1].clone_into(&mut self.package);
        }

        if tokens.len() > 2 {
            if tokens[2] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }
            tokens[2].clone_into(&mut self.namespace);
        }

        if tokens.len() > 3 {
            if tokens[3] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }
            tokens[3].clone_into(&mut self.type_name);
        }

        if tokens.len() > 4 {
            if tokens[4] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }

            if !tokens[4].starts_with('v') {
                return Err(GtsIdError::Segment {
                    num: self.num,
                    offset: self.offset,
                    segment: self.segment.clone(),
                    cause: "Major version must start with 'v'".to_owned(),
                });
            }

            let major_str = &tokens[4][1..];
            self.ver_major = major_str.parse().map_err(|_| GtsIdError::Segment {
                num: self.num,
                offset: self.offset,
                segment: self.segment.clone(),
                cause: "Major version must be an integer".to_owned(),
            })?;

            if major_str != self.ver_major.to_string() {
                return Err(GtsIdError::Segment {
                    num: self.num,
                    offset: self.offset,
                    segment: self.segment.clone(),
                    cause: "Major version must be an integer".to_owned(),
                });
            }
        }

        if tokens.len() > 5 {
            if tokens[5] == "*" {
                self.is_wildcard = true;
                return Ok(());
            }

            let minor: u32 = tokens[5].parse().map_err(|_| GtsIdError::Segment {
                num: self.num,
                offset: self.offset,
                segment: self.segment.clone(),
                cause: "Minor version must be an integer".to_owned(),
            })?;

            if tokens[5] != minor.to_string() {
                return Err(GtsIdError::Segment {
                    num: self.num,
                    offset: self.offset,
                    segment: self.segment.clone(),
                    cause: "Minor version must be an integer".to_owned(),
                });
            }

            self.ver_minor = Some(minor);
        }

        Ok(())
    }
}

/// GTS ID - a validated Global Type System identifier.
///
/// GTS IDs follow the format: `gts.<vendor>.<package>.<namespace>.<type>.<version>[~]`
/// where `~` suffix indicates a type/schema definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsID {
    pub id: String,
    pub gts_id_segments: Vec<GtsIdSegment>,
}

impl GtsID {
    /// Parse and validate a GTS identifier string.
    ///
    /// # Errors
    /// Returns `GtsIdError::Id` if the string is not a valid GTS identifier.
    pub fn new(id: &str) -> Result<Self, GtsIdError> {
        let raw = id.trim();

        // Validate lowercase
        if raw != raw.to_lowercase() {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: "Must be lower case".to_owned(),
            });
        }

        if raw.contains('-') {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: "Must not contain '-'".to_owned(),
            });
        }

        if !raw.starts_with(GTS_PREFIX) {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: format!("Does not start with '{GTS_PREFIX}'"),
            });
        }

        if raw.len() > 1024 {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: "Too long".to_owned(),
            });
        }

        let mut gts_id_segments = Vec::new();
        let remainder = &raw[GTS_PREFIX.len()..];

        // Split by ~ preserving empties to detect trailing ~
        let tilde_parts: Vec<&str> = remainder.split('~').collect();
        let mut parts = Vec::new();

        for i in 0..tilde_parts.len() {
            if i < tilde_parts.len() - 1 {
                parts.push(format!("{}~", tilde_parts[i]));
                if i == tilde_parts.len() - 2 && tilde_parts[i + 1].is_empty() {
                    break;
                }
            } else {
                parts.push(tilde_parts[i].to_owned());
            }
        }

        let mut offset = GTS_PREFIX.len();
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() || part == "~" {
                return Err(GtsIdError::Id {
                    id: id.to_owned(),
                    cause: format!("GTS segment #{} @ offset {offset} is empty", i + 1),
                });
            }

            gts_id_segments.push(GtsIdSegment::new(i + 1, offset, part)?);
            offset += part.len();
        }

        // Issue #37: Single-segment instance IDs are prohibited
        // Instance IDs must be chained with at least one type segment (e.g., 'type~instance')
        // This check should only apply to non-wildcard, non-type single-segment IDs
        if gts_id_segments.len() == 1
            && !gts_id_segments[0].is_type
            && !gts_id_segments[0].is_wildcard
        {
            return Err(GtsIdError::Id {
                id: id.to_owned(),
                cause: "Single-segment instance IDs are prohibited. Instance IDs must be chained with at least one type segment (e.g., 'type~instance')".to_owned(),
            });
        }

        Ok(GtsID {
            id: raw.to_owned(),
            gts_id_segments,
        })
    }

    #[must_use]
    pub fn is_type(&self) -> bool {
        self.id.ends_with('~')
    }

    #[must_use]
    pub fn get_type_id(&self) -> Option<String> {
        if self.gts_id_segments.len() < 2 {
            return None;
        }
        let segments: String = self.gts_id_segments[..self.gts_id_segments.len() - 1]
            .iter()
            .map(|s| s.segment.as_str())
            .collect::<Vec<_>>()
            .join("");
        Some(format!("{GTS_PREFIX}{segments}"))
    }

    /// Generate a deterministic UUID v5 from this GTS ID.
    #[must_use]
    pub fn to_uuid(&self) -> Uuid {
        Uuid::new_v5(&GTS_NS, self.id.as_bytes())
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
        let p = &pattern.id;

        // No wildcard case - need exact match with version flexibility
        if !p.contains('*') {
            return Self::match_segments(&pattern.gts_id_segments, &self.gts_id_segments);
        }

        // Wildcard case
        if p.matches('*').count() > 1 || !p.ends_with('*') {
            return false;
        }

        Self::match_segments(&pattern.gts_id_segments, &self.gts_id_segments)
    }

    fn match_segments(pattern_segs: &[GtsIdSegment], candidate_segs: &[GtsIdSegment]) -> bool {
        // If pattern is longer than candidate, no match
        if pattern_segs.len() > candidate_segs.len() {
            return false;
        }

        for (i, p_seg) in pattern_segs.iter().enumerate() {
            let c_seg = &candidate_segs[i];

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

    /// Splits a GTS ID with an optional attribute path.
    ///
    /// # Errors
    /// Returns `GtsIdError::Id` if the path is empty after the `@` separator.
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
            return Err(GtsIdError::Id {
                id: gts_with_path.to_owned(),
                cause: "Attribute path cannot be empty".to_owned(),
            });
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

/// GTS Wildcard pattern
#[derive(Debug, Clone, PartialEq)]
pub struct GtsWildcard {
    pub id: String,
    pub gts_id_segments: Vec<GtsIdSegment>,
}

impl GtsWildcard {
    /// Creates a new GTS wildcard pattern.
    ///
    /// # Errors
    /// Returns `GtsIdError::Wildcard` if the pattern is invalid.
    pub fn new(pattern: &str) -> Result<Self, GtsIdError> {
        let p = pattern.trim();

        if !p.starts_with(GTS_PREFIX) {
            return Err(GtsIdError::Wildcard {
                pattern: pattern.to_owned(),
                cause: format!("Does not start with '{GTS_PREFIX}'"),
            });
        }

        if p.matches('*').count() > 1 {
            return Err(GtsIdError::Wildcard {
                pattern: pattern.to_owned(),
                cause: "The wildcard '*' token is allowed only once".to_owned(),
            });
        }

        if p.contains('*') && !p.ends_with(".*") && !p.ends_with("~*") {
            return Err(GtsIdError::Wildcard {
                pattern: pattern.to_owned(),
                cause: "The wildcard '*' token is allowed only at the end of the pattern"
                    .to_owned(),
            });
        }

        // Try to parse as GtsID
        let gts_id = GtsID::new(p).map_err(|e| GtsIdError::Wildcard {
            pattern: pattern.to_owned(),
            cause: e.to_string(),
        })?;

        Ok(GtsWildcard {
            id: gts_id.id,
            gts_id_segments: gts_id.gts_id_segments,
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

/// A type-safe wrapper for GTS entity identifiers.
///
/// `GtsEntityId` wraps a fully-formed GTS entity ID string (e.g.,
/// `gts.x.core.events.topic.v1~vendor.app.orders.v1.0`). It can be used as a map key,
/// compared for equality, hashed, and serialized/deserialized.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsEntityId(String);

impl GtsEntityId {
    /// Creates a new GTS entity ID from a string.
    /// Must be private as it's used by `GtsInstanceId::new()` or `GtsEntityId::new()`.
    #[must_use]
    fn new(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// Returns the underlying string representation of the entity ID.
    #[must_use]
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

impl From<GtsEntityId> for String {
    fn from(id: GtsEntityId) -> Self {
        id.0
    }
}

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
        Ok(GtsInstanceId(GtsEntityId(s)))
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
    /// use gts::gts::GtsInstanceId;
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
        id.0.into()
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

/// A type-safe wrapper for GTS schema (type) identifiers.
///
/// `GtsSchemaId` wraps a fully-formed GTS schema ID string (e.g.,
/// `gts.x.core.events.topic.v1~`). It can be used as a map key,
/// compared for equality, hashed, and serialized/deserialized.
///
/// # Example
///
/// ```
/// use gts::gts::GtsSchemaId;
///
/// let id = GtsSchemaId::new("gts.x.core.events.topic.v1~vendor.app.orders.v1.0~");
/// assert_eq!(id.as_ref(), "gts.x.core.events.topic.v1~vendor.app.orders.v1.0~");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GtsSchemaId(GtsEntityId);

impl serde::Serialize for GtsSchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de> serde::Deserialize<'de> for GtsSchemaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(GtsSchemaId(GtsEntityId(s)))
    }
}

impl schemars::JsonSchema for GtsSchemaId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("GtsSchemaId")
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

impl GtsSchemaId {
    /// Returns the JSON Schema representation of `GtsSchemaId` as a `serde_json::Value`.
    ///
    /// This is the canonical schema definition used by both the schemars `JsonSchema` impl
    /// and the CLI schema generator, ensuring consistency.
    ///
    /// # Example
    /// ```
    /// use gts::gts::GtsSchemaId;
    ///
    /// let schema = GtsSchemaId::json_schema_value();
    /// assert_eq!(schema["type"], "string");
    /// assert_eq!(schema["format"], "gts-schema-id");
    /// assert_eq!(schema["x-gts-ref"], "gts.*");
    /// ```
    #[must_use]
    pub fn json_schema_value() -> serde_json::Value {
        serde_json::json!({
            "type": "string",
            "format": "gts-schema-id",
            "title": "GTS Schema ID",
            "description": "GTS schema identifier",
            "x-gts-ref": "gts.*"
        })
    }

    /// Creates a new GTS schema ID from string.
    ///
    /// # Arguments
    ///
    /// * `schema_id` - The GTS schema ID (e.g., `gts.x.core.events.topic.v1~`)
    ///
    /// # Returns
    ///
    /// A new `GtsSchemaId` containing the concatenated ID.
    #[must_use]
    pub fn new(schema_id: &str) -> Self {
        Self(GtsEntityId::new(schema_id))
    }

    /// Returns the underlying string representation of the schema ID.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0.into_string()
    }
}

impl fmt::Display for GtsSchemaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for GtsSchemaId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<GtsSchemaId> for String {
    fn from(id: GtsSchemaId) -> Self {
        id.0.into()
    }
}

impl std::ops::Deref for GtsSchemaId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl PartialEq<str> for GtsSchemaId {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<&str> for GtsSchemaId {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl PartialEq<String> for GtsSchemaId {
    fn eq(&self, other: &String) -> bool {
        self.0.as_ref() == other
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
        assert_eq!(id.gts_id_segments.len(), 1);
    }

    #[test]
    fn test_gts_id_with_minor_version() {
        let id = GtsID::new("gts.x.core.events.event.v1.2~").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1.2~");
        assert!(id.is_type());
        let seg = &id.gts_id_segments[0];
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
    fn test_uuid_generation() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let uuid1 = id.to_uuid();
        let uuid2 = id.to_uuid();
        // UUIDs should be deterministic
        assert_eq!(uuid1, uuid2);
        assert!(!uuid1.to_string().is_empty());
    }

    #[test]
    fn test_uuid_different_ids() {
        let id1 = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let id2 = GtsID::new("gts.x.core.events.event.v2~").expect("test");
        assert_ne!(id1.to_uuid(), id2.to_uuid());
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
    fn test_version_flexibility_in_matching() {
        // Pattern without minor version should match any minor version
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1~").expect("test");
        let id_no_minor = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        let id_with_minor = GtsID::new("gts.x.core.events.event.v1.0~").expect("test");

        assert!(id_no_minor.wildcard_match(&pattern));
        assert!(id_with_minor.wildcard_match(&pattern));
    }

    #[test]
    fn test_chained_identifiers() {
        let id =
            GtsID::new("gts.x.core.events.type.v1~vendor.app._.custom_event.v1~").expect("test");
        assert_eq!(id.gts_id_segments.len(), 2);
        assert_eq!(id.gts_id_segments[0].vendor, "x");
        assert_eq!(id.gts_id_segments[1].vendor, "vendor");
    }

    #[test]
    fn test_gts_id_segment_validation() {
        // Test invalid segment with special characters
        let result = GtsIdSegment::new(0, 0, "invalid-segment");
        assert!(result.is_err());

        // Test valid segment
        let result = GtsIdSegment::new(0, 0, "x.core.events.event.v1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_id_with_underscore() {
        // Underscores are allowed in namespace
        let id = GtsID::new("gts.x.core._.event.v1~").expect("test");
        assert_eq!(id.gts_id_segments[0].namespace, "_");
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
    fn test_gts_wildcard_multiple_wildcards_error() {
        let result = GtsWildcard::new("gts.*.*.*.*");
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
    fn test_gts_wildcard_instance_match() {
        let pattern = GtsWildcard::new("gts.x.core.events.*").expect("test");
        let id = GtsID::new("gts.x.core.events.event.v1~a.b.c.d.v1.0").expect("test");
        assert!(id.wildcard_match(&pattern));
    }

    #[test]
    fn test_gts_id_whitespace_trimming() {
        let id = GtsID::new("  gts.x.core.events.event.v1~  ").expect("test");
        assert_eq!(id.id, "gts.x.core.events.event.v1~");
    }

    #[test]
    fn test_gts_wildcard_whitespace_trimming() {
        let pattern = GtsWildcard::new("  gts.x.core.events.*  ").expect("test");
        assert_eq!(pattern.id, "gts.x.core.events.*");
    }

    #[test]
    fn test_gts_id_long_chain() {
        let id = GtsID::new("gts.a.b.c.d.v1~e.f.g.h.v2~i.j.k.l.v3~").expect("test");
        assert_eq!(id.gts_id_segments.len(), 3);
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
    fn test_gts_id_version_without_minor() {
        let id = GtsID::new("gts.x.core.events.event.v1~").expect("test");
        assert_eq!(id.gts_id_segments[0].ver_major, 1);
        assert_eq!(id.gts_id_segments[0].ver_minor, None);
    }

    #[test]
    fn test_gts_id_version_with_large_numbers() {
        let id = GtsID::new("gts.x.core.events.event.v99.999~").expect("test");
        assert_eq!(id.gts_id_segments[0].ver_major, 99);
        assert_eq!(id.gts_id_segments[0].ver_minor, Some(999));
    }

    #[test]
    fn test_gts_wildcard_no_wildcard_different_vendor() {
        let pattern = GtsWildcard::new("gts.x.core.events.event.v1~").expect("test");
        let id = GtsID::new("gts.y.core.events.event.v1~").expect("test");
        assert!(!id.wildcard_match(&pattern));
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
    fn test_gts_wildcard_type_suffix_match() {
        // Wildcard after type suffix
        let pattern = GtsWildcard::new("gts.x.pkg.ns.type.v1~*").expect("test");
        let id1 = GtsID::new("gts.x.pkg.ns.type.v1~a.b.c.child.v1~").expect("test");
        let id2 = GtsID::new("gts.x.pkg.ns.type.v2~a.b.c.child.v1~").expect("test");
        assert!(id1.wildcard_match(&pattern));
        assert!(!id2.wildcard_match(&pattern));
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

    #[test]
    fn test_gts_segment_too_many_tildes() {
        // Multiple tildes together (invalid segment)
        let seg = GtsIdSegment::new(1, 0, "x.pkg.ns.type.v1~~");
        assert!(seg.is_err());
        if let Err(e) = seg {
            assert!(e.to_string().contains("Too many '~' characters"));
        }
    }

    #[test]
    fn test_gts_segment_tilde_not_at_end() {
        // Tilde in middle of segment (not at end)
        let seg = GtsIdSegment::new(1, 0, "x.pkg~mid.ns.type.v1");
        assert!(seg.is_err());
        if let Err(e) = seg {
            assert!(e.to_string().contains("'~' must be at the end"));
        }
    }

    #[test]
    fn test_gts_segment_too_many_tokens() {
        // More than 6 tokens in a segment
        let seg = GtsIdSegment::new(1, 0, "x.pkg.ns.type.v1.2.extra~");
        assert!(seg.is_err());
        if let Err(e) = seg {
            assert!(e.to_string().contains("Too many tokens"));
        }
    }

    #[test]
    fn test_gts_segment_version_without_v_prefix() {
        // Version without 'v' prefix
        let seg = GtsIdSegment::new(1, 0, "x.pkg.ns.type.1~");
        assert!(seg.is_err());
        if let Err(e) = seg {
            assert!(e.to_string().contains("Major version must start with 'v'"));
        }
    }

    #[test]
    fn test_gts_segment_version_leading_zeros() {
        // Version with leading zeros (should fail because "01" != "1")
        let seg = GtsIdSegment::new(1, 0, "x.pkg.ns.type.v01~");
        assert!(seg.is_err());
        if let Err(e) = seg {
            assert!(e.to_string().contains("Major version must be an integer"));
        }
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
}
