/// x-gts-ref validation support for GTS schemas.
///
/// This module implements validation for the `x-gts-ref` extension as specified
/// in the GTS specification v0.5, section 9.5.
///
/// # Overview
///
/// The `x-gts-ref` extension allows schemas to enforce that string values must be
/// valid GTS identifiers or match specific patterns. This is useful for ensuring
/// referential integrity in GTS-based systems.
///
/// # Features
///
/// 1. **Schema Validation**: Validates that `x-gts-ref` fields in schemas contain valid patterns
/// 2. **Instance Validation**: Validates that instance values match their `x-gts-ref` constraints
/// 3. **JSON Pointer Resolution**: Supports JSON Pointer references (e.g., `/$id`, `/properties/name`)
/// 4. **GTS ID Pattern Matching**: Validates GTS IDs and prefix patterns (e.g., `gts.x.y._.z.v1~`)
///
/// # Examples
///
/// ## Schema with x-gts-ref
///
/// ```json
/// {
///   "$id": "gts://gts.x.example._.user.v1~",
///   "$schema": "http://json-schema.org/draft-07/schema#",
///   "type": "object",
///   "properties": {
///     "id": {
///       "type": "string",
///       "x-gts-ref": "/$id"
///     },
///     "role": {
///       "type": "string",
///       "x-gts-ref": "gts.x.example._.role.v1~"
///     }
///   }
/// }
/// ```
///
/// ## Usage
///
/// ```rust
/// use gts::XGtsRefValidator;
/// use serde_json::json;
///
/// let validator = XGtsRefValidator::new();
///
/// // Validate a schema
/// let schema = json!({
///     "$id": "gts://gts.x.test._.schema.v1~",
///     "$schema": "http://json-schema.org/draft-07/schema#",
///     "type": "object",
///     "properties": {
///         "id": {"type": "string", "x-gts-ref": "/$id"}
///     }
/// });
/// let errors = validator.validate_schema(&schema, "", None);
/// assert!(errors.is_empty());
///
/// // Validate an instance - note: the value must match $id WITHOUT the gts:// prefix
/// let instance = json!({"id": "gts.x.test._.schema.v1~"});
/// let errors = validator.validate_instance(&instance, &schema, "");
/// assert!(errors.is_empty());
/// ```
///
/// # x-gts-ref Patterns
///
/// The `x-gts-ref` field can contain:
///
/// - **GTS ID Pattern**: A full or prefix GTS identifier (e.g., `gts.x.y._.z.v1~`)
/// - **JSON Pointer**: A reference to another field in the schema (e.g., `/$id`, `/properties/name`)
///
/// ## JSON Pointer Resolution
///
/// When an `x-gts-ref` starts with `/`, it's treated as a JSON Pointer that resolves
/// to a value in the schema. The resolved value must be a valid GTS ID pattern.
///
/// Example:
/// ```json
/// {
///   "$id": "gts://gts.x.example._.user.v1~",
///   "$schema": "http://json-schema.org/draft-07/schema#",
///   "type": "object",
///   "properties": {
///     "type": {"type": "string", "x-gts-ref": "/$id"}
///   }
/// }
/// ```
///
/// In this case, the `type` field must match the schema's `$id` value.
use serde_json::Value;
use std::fmt;

use crate::gts::GtsID;

/// Error type for x-gts-ref validation failures
#[derive(Debug, Clone)]
pub struct XGtsRefValidationError {
    pub field_path: String,
    pub value: String,
    pub ref_pattern: String,
    pub reason: String,
}

impl XGtsRefValidationError {
    #[must_use]
    pub fn new(field_path: String, value: String, ref_pattern: String, reason: String) -> Self {
        Self {
            field_path,
            value,
            ref_pattern,
            reason,
        }
    }
}

impl fmt::Display for XGtsRefValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "x-gts-ref validation failed for field '{}': {}",
            self.field_path, self.reason
        )
    }
}

impl std::error::Error for XGtsRefValidationError {}

/// Validator for x-gts-ref constraints in GTS schemas
#[derive(Debug, Clone, Copy, Default)]
pub struct XGtsRefValidator;

// These methods take &self for API consistency even though XGtsRefValidator is zero-sized.
// This allows future extension with state if needed.
#[allow(clippy::unused_self, clippy::trivially_copy_pass_by_ref)]
impl XGtsRefValidator {
    /// Create a new validator
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Validate an instance against x-gts-ref constraints in schema
    ///
    /// # Arguments
    /// * `instance` - The data instance to validate
    /// * `schema` - The JSON schema with x-gts-ref extensions
    /// * `instance_path` - Current path in instance (for error reporting)
    ///
    /// # Returns
    /// List of validation errors (empty if valid)
    #[must_use]
    pub fn validate_instance(
        &self,
        instance: &Value,
        schema: &Value,
        instance_path: &str,
    ) -> Vec<XGtsRefValidationError> {
        let mut errors = Vec::new();
        self.visit_instance(instance, schema, schema, instance_path, &mut errors);
        errors
    }

    fn visit_instance(
        &self,
        inst: &Value,
        sch: &Value,
        root_schema: &Value,
        path: &str,
        errors: &mut Vec<XGtsRefValidationError>,
    ) {
        let Some(sch_obj) = sch.as_object() else {
            return;
        };

        // Check for x-gts-ref constraint
        if let Some(x_gts_ref) = sch_obj.get("x-gts-ref")
            && let Some(inst_str) = inst.as_str()
            && let Some(ref_pattern) = x_gts_ref.as_str()
            && let Some(error) = self.validate_ref_value(inst_str, ref_pattern, path, root_schema)
        {
            errors.push(error);
        }

        // Recurse into object properties
        if let Some(Value::String(type_str)) = sch_obj.get("type") {
            if type_str == "object" {
                if let Some(properties) = sch_obj.get("properties")
                    && let Some(properties_obj) = properties.as_object()
                    && let Some(inst_obj) = inst.as_object()
                {
                    for (prop_name, prop_schema) in properties_obj {
                        if let Some(prop_value) = inst_obj.get(prop_name) {
                            let prop_path = if path.is_empty() {
                                prop_name.clone()
                            } else {
                                format!("{path}.{prop_name}")
                            };
                            self.visit_instance(
                                prop_value,
                                prop_schema,
                                root_schema,
                                &prop_path,
                                errors,
                            );
                        }
                    }
                }
            } else if type_str == "array"
                && let Some(items) = sch_obj.get("items")
                && let Some(inst_arr) = inst.as_array()
            {
                for (idx, item) in inst_arr.iter().enumerate() {
                    let item_path = format!("{path}[{idx}]");
                    self.visit_instance(item, items, root_schema, &item_path, errors);
                }
            }
        }
    }

    /// Validate x-gts-ref fields in a schema definition
    ///
    /// # Arguments
    /// * `schema` - The JSON schema to validate
    /// * `schema_path` - Current path in schema (for error reporting)
    /// * `root_schema` - The root schema (for resolving relative refs)
    ///
    /// # Returns
    /// List of validation errors (empty if valid)
    #[must_use]
    pub fn validate_schema(
        &self,
        schema: &Value,
        schema_path: &str,
        root_schema: Option<&Value>,
    ) -> Vec<XGtsRefValidationError> {
        let root = root_schema.unwrap_or(schema);
        let mut errors = Vec::new();
        self.visit_schema(schema, schema_path, root, &mut errors);
        errors
    }

    fn visit_schema(
        &self,
        sch: &Value,
        path: &str,
        root_schema: &Value,
        errors: &mut Vec<XGtsRefValidationError>,
    ) {
        let Some(sch_obj) = sch.as_object() else {
            return;
        };

        // Check for x-gts-ref field
        if let Some(x_gts_ref) = sch_obj.get("x-gts-ref") {
            let ref_path = if path.is_empty() {
                "x-gts-ref".to_owned()
            } else {
                format!("{path}/x-gts-ref")
            };

            if let Some(ref_value) = x_gts_ref.as_str() {
                if let Some(error) = self.validate_ref_pattern(ref_value, &ref_path, root_schema) {
                    errors.push(error);
                }
            } else {
                errors.push(XGtsRefValidationError::new(
                    ref_path,
                    format!("{x_gts_ref:?}"),
                    String::new(),
                    format!("x-gts-ref value must be a string, got {x_gts_ref}"),
                ));
            }
        }

        // Recurse into nested structures
        for (key, value) in sch_obj {
            if key == "x-gts-ref" {
                continue;
            }
            let nested_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}/{key}")
            };

            if value.is_object() {
                self.visit_schema(value, &nested_path, root_schema, errors);
            } else if let Some(arr) = value.as_array() {
                for (idx, item) in arr.iter().enumerate() {
                    if item.is_object() {
                        let item_path = format!("{nested_path}[{idx}]");
                        self.visit_schema(item, &item_path, root_schema, errors);
                    }
                }
            }
        }
    }

    /// Validate an instance value against its x-gts-ref constraint
    fn validate_ref_value(
        &self,
        value: &str,
        ref_pattern: &str,
        field_path: &str,
        schema: &Value,
    ) -> Option<XGtsRefValidationError> {
        // Resolve pattern if it's a relative reference
        let resolved_pattern = if ref_pattern.starts_with('/') {
            match Self::resolve_pointer(schema, ref_pattern) {
                Some(resolved) => {
                    if !resolved.starts_with("gts.") {
                        return Some(XGtsRefValidationError::new(
                            field_path.to_owned(),
                            value.to_owned(),
                            ref_pattern.to_owned(),
                            format!(
                                "Resolved reference '{ref_pattern}' -> '{resolved}' is not a GTS pattern"
                            ),
                        ));
                    }
                    resolved
                }
                None => {
                    return Some(XGtsRefValidationError::new(
                        field_path.to_owned(),
                        value.to_owned(),
                        ref_pattern.to_owned(),
                        format!("Cannot resolve reference path '{ref_pattern}'"),
                    ));
                }
            }
        } else {
            ref_pattern.to_owned()
        };

        // Validate against GTS pattern
        self.validate_gts_pattern(value, &resolved_pattern, field_path)
    }

    /// Validate an x-gts-ref pattern in a schema definition
    fn validate_ref_pattern(
        &self,
        ref_pattern: &str,
        field_path: &str,
        root_schema: &Value,
    ) -> Option<XGtsRefValidationError> {
        // Case 1: Absolute GTS pattern
        if ref_pattern.starts_with("gts.") {
            return self.validate_gts_id_or_pattern(ref_pattern, field_path);
        }

        // Case 2: Relative reference
        if ref_pattern.starts_with('/') {
            match Self::resolve_pointer(root_schema, ref_pattern) {
                Some(resolved) => {
                    if !GtsID::is_valid(&resolved) {
                        return Some(XGtsRefValidationError::new(
                            field_path.to_owned(),
                            ref_pattern.to_owned(),
                            ref_pattern.to_owned(),
                            format!(
                                "Resolved reference '{ref_pattern}' -> '{resolved}' is not a valid GTS identifier"
                            ),
                        ));
                    }
                    None
                }
                None => Some(XGtsRefValidationError::new(
                    field_path.to_owned(),
                    ref_pattern.to_owned(),
                    ref_pattern.to_owned(),
                    format!("Cannot resolve reference path '{ref_pattern}'"),
                )),
            }
        } else {
            Some(XGtsRefValidationError::new(
                field_path.to_owned(),
                ref_pattern.to_owned(),
                ref_pattern.to_owned(),
                format!("Invalid x-gts-ref value: '{ref_pattern}' must start with 'gts.' or '/'"),
            ))
        }
    }

    /// Validate a GTS ID or pattern in schema definition
    fn validate_gts_id_or_pattern(
        &self,
        pattern: &str,
        field_path: &str,
    ) -> Option<XGtsRefValidationError> {
        // Valid wildcard
        if pattern == "gts.*" {
            return None;
        }

        // Wildcard pattern - validate prefix
        if pattern.contains('*') {
            let prefix = pattern.trim_end_matches('*');
            if !prefix.starts_with("gts.") {
                return Some(XGtsRefValidationError::new(
                    field_path.to_owned(),
                    pattern.to_owned(),
                    pattern.to_owned(),
                    format!("Invalid GTS wildcard pattern: {pattern}"),
                ));
            }
            return None;
        }

        // Specific GTS ID
        if !GtsID::is_valid(pattern) {
            return Some(XGtsRefValidationError::new(
                field_path.to_owned(),
                pattern.to_owned(),
                pattern.to_owned(),
                format!("Invalid GTS identifier: {pattern}"),
            ));
        }

        None
    }

    /// Validate value matches a GTS pattern
    fn validate_gts_pattern(
        &self,
        value: &str,
        pattern: &str,
        field_path: &str,
    ) -> Option<XGtsRefValidationError> {
        // Validate it's a valid GTS ID
        if !GtsID::is_valid(value) {
            return Some(XGtsRefValidationError::new(
                field_path.to_owned(),
                value.to_owned(),
                pattern.to_owned(),
                format!("Value '{value}' is not a valid GTS identifier"),
            ));
        }

        // Check pattern match
        if pattern == "gts.*" {
            // Any valid GTS ID matches
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            if !value.starts_with(prefix) {
                return Some(XGtsRefValidationError::new(
                    field_path.to_owned(),
                    value.to_owned(),
                    pattern.to_owned(),
                    format!("Value '{value}' does not match pattern '{pattern}'"),
                ));
            }
        } else if !value.starts_with(pattern) {
            return Some(XGtsRefValidationError::new(
                field_path.to_owned(),
                value.to_owned(),
                pattern.to_owned(),
                format!("Value '{value}' does not match pattern '{pattern}'"),
            ));
        }

        // Note: We don't check if the entity exists in the store here
        // to avoid borrowing issues. The store check can be done separately if needed.

        None
    }

    /// Resolve a JSON Pointer in the schema
    ///
    /// # Arguments
    /// * `schema` - The schema to search
    /// * `pointer` - JSON Pointer (e.g., "/$id", "/properties/type")
    ///
    /// # Returns
    /// The resolved value as a string or None if not found.
    /// Note: For `/$id` references, the `gts://` prefix is stripped from the value
    /// as per GTS specification (relative self-reference should match the $id without the prefix).
    fn resolve_pointer(schema: &Value, pointer: &str) -> Option<String> {
        let path = pointer.trim_start_matches('/');
        if path.is_empty() {
            return None;
        }

        let parts: Vec<&str> = path.split('/').collect();
        let mut current = schema;

        for part in parts {
            if !current.is_object() {
                return None;
            }
            current = current.get(part)?;
        }

        // If current is a string, return it (stripping gts:// prefix if present)
        if let Some(s) = current.as_str() {
            return Some(Self::strip_gts_uri_prefix(s));
        }

        // If current is an object with x-gts-ref, resolve it
        if let Some(obj) = current.as_object()
            && let Some(ref_value) = obj.get("x-gts-ref")
            && let Some(ref_str) = ref_value.as_str()
        {
            if ref_str.starts_with('/') {
                return Self::resolve_pointer(schema, ref_str);
            }
            return Some(ref_str.to_owned());
        }

        None
    }

    /// Strip the `gts://` prefix from a value if present.
    ///
    /// This is used for `/$id` relative references where the schema's `$id` field
    /// contains a full GTS URI (e.g., `gts://gts.x.example._.user.v1~`) but the
    /// instance value should match without the prefix (e.g., `gts.x.example._.user.v1~`).
    fn strip_gts_uri_prefix(value: &str) -> String {
        value.strip_prefix("gts://").unwrap_or(value).to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_gts_pattern_matching() {
        let validator = XGtsRefValidator::new();

        // Test exact match
        let result = validator.validate_gts_pattern(
            "gts.x.core.events.topic.v1~",
            "gts.x.core.events.topic.v1~",
            "test_field",
        );
        assert!(result.is_none(), "Exact match should succeed");

        // Test wildcard match
        let result =
            validator.validate_gts_pattern("gts.x.core.events.topic.v1~", "gts.*", "test_field");
        assert!(result.is_none(), "Wildcard match should succeed");

        // Test prefix match
        let result = validator.validate_gts_pattern(
            "gts.x.core.events.topic.v1~",
            "gts.x.core.*",
            "test_field",
        );
        assert!(result.is_none(), "Prefix match should succeed");

        // Test mismatch
        let result = validator.validate_gts_pattern(
            "gts.x.core.events.topic.v1~",
            "gts.y.core.*",
            "test_field",
        );
        assert!(result.is_some(), "Mismatch should return error");
    }

    #[test]
    fn test_validate_schema_with_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_instance_with_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let instance = json!({
            "topic_id": "gts.x.core.events.topic.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_instance_with_x_gts_ref_mismatch() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let instance = json!({
            "topic_id": "gts.y.core.events.topic.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_instance_with_dollar_id_ref_strips_gts_prefix() {
        let validator = XGtsRefValidator::new();
        // Schema has $id with gts:// prefix
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "x-gts-ref": "/$id"
                }
            }
        });

        // Instance value should match WITHOUT the gts:// prefix
        let instance = json!({
            "id": "gts.x.test._.entity.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty(), "Expected no errors but got: {errors:?}");
    }

    #[test]
    fn test_validate_instance_with_dollar_id_ref_rejects_full_uri() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "x-gts-ref": "/$id"
                }
            }
        });

        // Instance value with gts:// prefix should be rejected (not a valid GTS ID)
        let instance = json!({
            "id": "gts://gts.x.test._.entity.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(
            !errors.is_empty(),
            "Expected validation error for value with gts:// prefix"
        );
    }

    #[test]
    fn test_strip_gts_uri_prefix() {
        // With prefix
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts://gts.x.test._.entity.v1~"),
            "gts.x.test._.entity.v1~"
        );
        // Without prefix (passthrough)
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts.x.test._.entity.v1~"),
            "gts.x.test._.entity.v1~"
        );
        // Empty string
        assert_eq!(XGtsRefValidator::strip_gts_uri_prefix(""), "");
        // Partial prefix
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts:/incomplete"),
            "gts:/incomplete"
        );
    }

    #[test]
    fn test_validation_error_creation_and_display() {
        let error = XGtsRefValidationError::new(
            "test_field".to_owned(),
            "invalid_value".to_owned(),
            "gts.x.*".to_owned(),
            "Test reason".to_owned(),
        );

        // Test field access
        assert_eq!(error.field_path, "test_field");
        assert_eq!(error.value, "invalid_value");
        assert_eq!(error.ref_pattern, "gts.x.*");
        assert_eq!(error.reason, "Test reason");

        // Test display formatting
        let display = format!("{error}");
        assert!(display.contains("test_field"));
        assert!(display.contains("Test reason"));
    }

    #[test]
    fn test_validate_gts_pattern_failures() {
        let validator = XGtsRefValidator::new();

        // Test invalid GTS ID
        let result = validator.validate_gts_pattern("not-a-valid-gts-id", "gts.*", "test_field");
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("not a valid GTS identifier")
        );

        // Test prefix no match
        let result = validator.validate_gts_pattern("gts.a.b.c.d.v1~", "gts.x.y.*", "test_field");
        assert!(result.is_some());
        assert!(result.unwrap().reason.contains("does not match pattern"));

        // Test exact no match
        let result =
            validator.validate_gts_pattern("gts.a.b.c.d.v1~", "gts.x.y.z.w.v1~", "test_field");
        assert!(result.is_some());
    }

    #[test]
    fn test_validate_gts_id_or_pattern() {
        let validator = XGtsRefValidator::new();

        // Valid exact GTS ID
        assert!(
            validator
                .validate_gts_id_or_pattern("gts.x.core.events.topic.v1~", "test_field",)
                .is_none()
        );

        // Valid wildcard
        assert!(
            validator
                .validate_gts_id_or_pattern("gts.*", "test_field")
                .is_none()
        );

        // Valid prefix wildcard
        assert!(
            validator
                .validate_gts_id_or_pattern("gts.x.core.*", "test_field")
                .is_none()
        );

        // Invalid wildcard
        let result = validator.validate_gts_id_or_pattern("invalid.*", "test_field");
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("Invalid GTS wildcard pattern")
        );

        // Invalid ID
        let result = validator.validate_gts_id_or_pattern("not-a-valid-id", "test_field");
        assert!(result.is_some());
        assert!(result.unwrap().reason.contains("Invalid GTS identifier"));
    }

    #[test]
    fn test_validate_ref_pattern() {
        let validator = XGtsRefValidator::new();
        let schema_with_id = json!({"$id": "gts://gts.x.test._.entity.v1~"});
        let schema_without_id = json!({});

        // Valid GTS prefix
        assert!(
            validator
                .validate_ref_pattern("gts.x.y.z.w.v1~", "test_field", &schema_without_id)
                .is_none()
        );

        // Invalid GTS prefix
        let result =
            validator.validate_ref_pattern("gts.INVALID", "test_field", &schema_without_id);
        assert!(result.is_some());

        // Valid JSON pointer
        assert!(
            validator
                .validate_ref_pattern("/$id", "test_field", &schema_with_id)
                .is_none()
        );

        // JSON pointer with invalid resolution
        let schema_bad = json!({"notAnId": "not-a-valid-gts-id"});
        let result = validator.validate_ref_pattern("/notAnId", "test_field", &schema_bad);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("not a valid GTS identifier")
        );

        // JSON pointer not found
        let result =
            validator.validate_ref_pattern("/nonexistent", "test_field", &schema_without_id);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("Cannot resolve reference path")
        );

        // Invalid format
        let result =
            validator.validate_ref_pattern("invalid-format", "test_field", &schema_without_id);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("must start with 'gts.' or '/'")
        );
    }

    #[test]
    fn test_validate_ref_value() {
        let validator = XGtsRefValidator::new();
        let schema_with_id = json!({"$id": "gts://gts.x.test._.entity.v1~"});
        let schema_without_pattern = json!({"someField": "not-a-gts-pattern"});

        // Valid GTS pattern
        assert!(
            validator
                .validate_ref_value(
                    "gts.x.test._.entity.v1~",
                    "gts.x.test.*",
                    "test_field",
                    &schema_with_id,
                )
                .is_none()
        );

        // Valid JSON pointer
        assert!(
            validator
                .validate_ref_value(
                    "gts.x.test._.entity.v1~",
                    "/$id",
                    "test_field",
                    &schema_with_id,
                )
                .is_none()
        );

        // JSON pointer not GTS pattern
        let result = validator.validate_ref_value(
            "some-value",
            "/someField",
            "test_field",
            &schema_without_pattern,
        );
        assert!(result.is_some());
        assert!(result.unwrap().reason.contains("is not a GTS pattern"));

        // JSON pointer not found
        let result =
            validator.validate_ref_value("some-value", "/missing", "test_field", &json!({}));
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .reason
                .contains("Cannot resolve reference path")
        );
    }

    #[test]
    fn test_resolve_pointer() {
        // Simple resolution
        let schema = json!({"$id": "gts://gts.x.test._.entity.v1~"});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/$id"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );

        // Nested resolution
        let schema = json!({
            "properties": {
                "name": {
                    "x-gts-ref": "gts.x.test.*"
                }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/name/x-gts-ref"),
            Some("gts.x.test.*".to_owned())
        );

        // Not found
        let schema = json!({"properties": {}});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/nonexistent"),
            None
        );

        // Empty path
        let schema = json!({"$id": "test"});
        assert_eq!(XGtsRefValidator::resolve_pointer(&schema, "/"), None);

        // Non-object path
        let schema = json!({"value": "string"});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/value/nested"),
            None
        );

        // With x-gts-ref recursion
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "properties": {
                "type": {
                    "x-gts-ref": "/$id"
                }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/type"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );

        // Strips gts:// URI prefix
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "type": "gts://gts.x.another._.type.v1~"
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/$id"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/type"),
            Some("gts.x.another._.type.v1~".to_owned())
        );
    }

    #[test]
    fn test_visit_schema_non_string_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "properties": {
                "field": {
                    "x-gts-ref": 123
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(!errors.is_empty());
        assert!(errors[0].reason.contains("must be a string"));
    }

    #[test]
    fn test_visit_schema_nested_in_properties() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field1": {
                    "type": "string",
                    "x-gts-ref": "gts.x.test.*"
                },
                "field2": {
                    "type": "string",
                    "x-gts-ref": "gts.y.test.*"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_schema_nested_in_array() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "items": [
                {
                    "x-gts-ref": "gts.x.test.*"
                },
                {
                    "x-gts-ref": "gts.y.test.*"
                }
            ]
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_nested_objects() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "outer": {
                    "type": "object",
                    "properties": {
                        "inner": {
                            "type": "string",
                            "x-gts-ref": "gts.x.test.*"
                        }
                    }
                }
            }
        });

        // Valid nested value
        let instance_valid = json!({
            "outer": {
                "inner": "gts.x.test._.entity.v1~"
            }
        });
        let errors = validator.validate_instance(&instance_valid, &schema, "");
        assert!(errors.is_empty(), "Valid nested value should pass");

        // Invalid nested value
        let instance_invalid = json!({
            "outer": {
                "inner": "gts.y.different._.entity.v1~"
            }
        });
        let errors = validator.validate_instance(&instance_invalid, &schema, "");
        assert!(!errors.is_empty(), "Invalid nested value should fail");
        assert!(errors[0].field_path.contains("outer.inner"));
    }

    #[test]
    fn test_visit_instance_array() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "array",
            "items": {
                "type": "string",
                "x-gts-ref": "gts.x.test.*"
            }
        });

        let instance = json!(["gts.x.test._.entity1.v1~", "gts.x.test._.entity2.v1~"]);

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_array_with_error() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "array",
            "items": {
                "type": "string",
                "x-gts-ref": "gts.x.test.*"
            }
        });

        let instance = json!(["gts.x.test._.entity1.v1~", "gts.y.other._.entity2.v1~"]);

        let errors = validator.validate_instance(&instance, &schema, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field_path.contains("[1]"));
    }

    #[test]
    fn test_visit_instance_schema_not_object() {
        let validator = XGtsRefValidator::new();
        let schema = json!("not an object");
        let instance = json!({"field": "value"});

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_no_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field": {
                    "type": "string"
                }
            }
        });

        let instance = json!({
            "field": "any value"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_value_not_string() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field": {
                    "type": "number",
                    "x-gts-ref": "gts.x.test.*"
                }
            }
        });

        let instance = json!({
            "field": 123
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        // No error because value is not a string, so x-gts-ref doesn't apply
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_instance_empty_path() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "string",
            "x-gts-ref": "gts.x.test.*"
        });

        let instance = json!("gts.x.test._.entity.v1~");

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_schema_with_root_schema() {
        let validator = XGtsRefValidator::new();
        let root = json!({
            "$id": "gts://gts.x.test._.root.v1~"
        });

        let schema = json!({
            "properties": {
                "field": {
                    "x-gts-ref": "/$id"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", Some(&root));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_strip_gts_uri_prefix_empty_string() {
        let result = XGtsRefValidator::strip_gts_uri_prefix("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_gts_uri_prefix_partial_prefix() {
        let result = XGtsRefValidator::strip_gts_uri_prefix("gts:/incomplete");
        assert_eq!(result, "gts:/incomplete");
    }
}
