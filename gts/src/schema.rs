//! Runtime schema generation traits for GTS types.
//!
//! This module provides the `GtsSchema` trait which enables runtime schema
//! composition for nested generic types like `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`.

use serde_json::Value;

/// Trait for types that have a GTS schema.
///
/// This trait enables runtime schema composition for nested generic types.
/// When you have `BaseEventV1<P>` where `P: GtsSchema`, the composed schema
/// can be generated at runtime with proper nesting.
///
/// # Example
///
/// ```ignore
/// use gts::GtsSchema;
///
/// // Get the composed schema for a nested type
/// let schema = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>>::gts_schema();
/// // The schema will have payload field containing AuditPayloadV1's schema,
/// // which in turn has data field containing PlaceOrderDataV1's schema
/// ```
pub trait GtsSchema {
    /// The GTS schema ID for this type.
    const SCHEMA_ID: &'static str;

    /// The name of the field that contains the generic type parameter, if any.
    /// For example, `BaseEventV1<P>` has `payload` as the generic field.
    const GENERIC_FIELD: Option<&'static str> = None;

    /// Returns the JSON schema for this type with $ref references intact.
    fn gts_schema_with_refs() -> Value;

    /// Returns the composed JSON schema for this type.
    /// For types with generic parameters that implement `GtsSchema`,
    /// this returns the schema with the generic field's type replaced
    /// by the nested type's schema.
    #[must_use]
    fn gts_schema() -> Value {
        Self::gts_schema_with_refs()
    }

    /// Generate a GTS-style schema with allOf and $ref to base type.
    ///
    /// This produces a schema like:
    /// ```json
    /// {
    ///   "$id": "gts://innermost_type_id",
    ///   "allOf": [
    ///     { "$ref": "gts://base_type_id" },
    ///     { "properties": { "payload": { nested_schema } } }
    ///   ]
    /// }
    /// ```
    #[must_use]
    fn gts_schema_with_refs_allof() -> Value {
        Self::gts_schema_with_refs()
    }

    /// Get the innermost schema ID in a nested generic chain.
    /// For `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`, returns `PlaceOrderDataV1`'s ID.
    #[must_use]
    fn innermost_schema_id() -> &'static str {
        Self::SCHEMA_ID
    }

    /// Get the innermost (leaf) type's raw schema.
    /// For `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`, returns `PlaceOrderDataV1`'s schema.
    #[must_use]
    fn innermost_schema() -> Value {
        Self::gts_schema_with_refs()
    }

    /// Collect the nesting path (generic field names) from outer to inner types.
    /// For `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`, returns `["payload", "data"]`.
    #[must_use]
    fn collect_nesting_path() -> Vec<&'static str> {
        Vec::new()
    }

    /// Wrap properties in a nested structure following the nesting path.
    /// For path `["payload", "data"]` and properties `{order_id, product_id, last}`,
    /// returns `{ "payload": { "type": "object", "properties": { "data": { "type": "object", "additionalProperties": false, "properties": {...}, "required": [...] } } } }`
    ///
    /// The `additionalProperties: false` is placed on the object that contains the current type's
    /// own properties. Generic fields that will be extended by children are just `{"type": "object"}`.
    ///
    /// # Arguments
    /// * `path` - The nesting path from outer to inner (e.g., `["payload", "data"]`)
    /// * `properties` - The properties of the current type
    /// * `required` - The required fields of the current type
    /// * `generic_field` - The name of the generic field in the current type (if any), which should NOT have additionalProperties: false
    #[must_use]
    fn wrap_in_nesting_path(
        path: &[&str],
        properties: Value,
        required: Value,
        generic_field: Option<&str>,
    ) -> Value {
        if path.is_empty() {
            return properties;
        }

        // Build the innermost schema - this contains the current type's own properties
        // Set additionalProperties: false on this level (the object containing our properties)
        let mut current = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required
        });

        // If we have a generic field, ensure it's just {"type": "object"} without additionalProperties
        // This field will be extended by child schemas
        if let Some(gf) = generic_field
            && let Some(props) = current
                .get_mut("properties")
                .and_then(|v| v.as_object_mut())
            && props.contains_key(gf)
        {
            props.insert(gf.to_owned(), serde_json::json!({"type": "object"}));
        }

        // Wrap from inner to outer - parent levels don't need additionalProperties: false
        for field in path.iter().rev() {
            current = serde_json::json!({
                "type": "object",
                "properties": {
                    *field: current
                }
            });
        }

        // Extract just the properties object from the outermost wrapper
        // since the caller will put this in a "properties" field
        if let Some(props) = current.get("properties") {
            return props.clone();
        }

        current
    }
}

/// Marker implementation for () to allow `BaseEventV1<()>` etc.
impl GtsSchema for () {
    const SCHEMA_ID: &'static str = "";

    fn gts_schema_with_refs() -> Value {
        serde_json::json!({
            "type": "object"
        })
    }

    fn gts_schema() -> Value {
        Self::gts_schema_with_refs()
    }
}

/// Generate a GTS-style schema for a nested type with allOf and $ref to base.
///
/// This macro generates a schema where:
/// - `$id` is the innermost type's schema ID
/// - `allOf` contains a `$ref` to the base (outermost) type's schema ID
/// - The nested types' properties are placed in the payload fields
///
/// # Example
///
/// ```ignore
/// use gts::gts_schema_for;
///
/// let schema = gts_schema_for!(BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>);
/// // Produces:
/// // {
/// //   "$id": "gts://...PlaceOrderDataV1...",
/// //   "allOf": [
/// //     { "$ref": "gts://BaseEventV1..." },
/// //     { "properties": { "payload": { ... } } }
/// //   ]
/// // }
/// ```
#[macro_export]
macro_rules! gts_schema_for {
    ($base:ty) => {{
        use $crate::GtsSchema;
        <$base as GtsSchema>::gts_schema_with_refs_allof()
    }};
}

/// Strip schema metadata fields ($id, $schema, title, description) for cleaner nested schemas.
#[must_use]
pub fn strip_schema_metadata(schema: &Value) -> Value {
    let mut result = schema.clone();
    if let Some(obj) = result.as_object_mut() {
        obj.remove("$id");
        obj.remove("$schema");
        obj.remove("title");
        obj.remove("description");

        // Recursively strip from nested properties
        if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
            let keys: Vec<String> = props.keys().cloned().collect();
            for key in keys {
                if let Some(prop_value) = props.get(&key) {
                    let cleaned = strip_schema_metadata(prop_value);
                    props.insert(key, cleaned);
                }
            }
        }
    }
    result
}

/// Build a GTS schema with allOf structure referencing base type.
///
/// # Arguments
/// * `innermost_schema_id` - The $id for the generated schema (innermost type)
/// * `base_schema_id` - The $ref target (base/outermost type)
/// * `title` - Schema title
/// * `own_properties` - Properties specific to this composed type
/// * `required` - Required fields
#[must_use]
pub fn build_gts_allof_schema(
    innermost_schema_id: &str,
    base_schema_id: &str,
    title: &str,
    own_properties: &Value,
    required: &[&str],
) -> Value {
    serde_json::json!({
        "$id": format!("gts://{}", innermost_schema_id),
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": title,
        "type": "object",
        "allOf": [
            { "$ref": format!("gts://{}", base_schema_id) },
            {
                "type": "object",
                "properties": own_properties,
                "required": required
            }
        ]
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_unit_type_properties() {
        // Test all unit type properties in one test
        let schema = <()>::gts_schema();
        assert_eq!(schema, json!({"type": "object"}));
        assert_eq!(<()>::SCHEMA_ID, "");
        assert_eq!(<()>::GENERIC_FIELD, None);
    }

    #[test]
    fn test_wrap_in_nesting_path_empty_path() {
        let properties = json!({"field1": {"type": "string"}});
        let required = json!(["field1"]);

        let result = <()>::wrap_in_nesting_path(&[], properties.clone(), required, None);

        assert_eq!(result, properties);
    }

    #[test]
    fn test_wrap_in_nesting_path_single_level() {
        let properties = json!({"field1": {"type": "string"}});
        let required = json!(["field1"]);

        let result = <()>::wrap_in_nesting_path(&["payload"], properties, required.clone(), None);

        assert_eq!(
            result,
            json!({
                "payload": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"field1": {"type": "string"}},
                    "required": required
                }
            })
        );
    }

    #[test]
    fn test_wrap_in_nesting_path_multi_level() {
        let properties = json!({"field1": {"type": "string"}});
        let required = json!(["field1"]);

        let result =
            <()>::wrap_in_nesting_path(&["payload", "data"], properties, required.clone(), None);

        assert_eq!(
            result,
            json!({
                "payload": {
                    "type": "object",
                    "properties": {
                        "data": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {"field1": {"type": "string"}},
                            "required": required
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_wrap_in_nesting_path_with_generic_field() {
        let properties = json!({
            "field1": {"type": "string"},
            "generic_field": {"type": "number"}
        });
        let required = json!(["field1"]);

        let result =
            <()>::wrap_in_nesting_path(&["payload"], properties, required, Some("generic_field"));

        let result_obj = result.as_object().unwrap();
        let payload = result_obj.get("payload").unwrap();
        let props = payload.get("properties").unwrap();

        // Generic field should be just {"type": "object"}
        assert_eq!(
            props.get("generic_field").unwrap(),
            &json!({"type": "object"})
        );
        // Other fields should be preserved
        assert_eq!(props.get("field1").unwrap(), &json!({"type": "string"}));
    }

    #[test]
    fn test_strip_schema_metadata_removes_all_metadata() {
        // Test removal of all metadata fields including $id, $schema, title, description
        let schema = json!({
            "$id": "gts://test",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "Test Schema",
            "description": "A test",
            "type": "object",
            "properties": {"field": {"type": "string"}}
        });

        let result = strip_schema_metadata(&schema);

        // All metadata should be removed
        assert!(result.get("$id").is_none());
        assert!(result.get("$schema").is_none());
        assert!(result.get("title").is_none());
        assert!(result.get("description").is_none());
        // Non-metadata should be preserved
        assert_eq!(result.get("type").unwrap(), "object");
        assert!(result.get("properties").is_some());
    }

    #[test]
    fn test_strip_schema_metadata_recursive() {
        let schema = json!({
            "$id": "gts://test",
            "properties": {
                "nested": {
                    "$id": "gts://nested",
                    "type": "string",
                    "description": "Nested field"
                }
            }
        });

        let result = strip_schema_metadata(&schema);

        assert!(result.get("$id").is_none());
        let props = result.get("properties").unwrap();
        let nested = props.get("nested").unwrap();
        assert!(nested.get("$id").is_none());
        assert!(nested.get("description").is_none());
        assert_eq!(nested.get("type").unwrap(), "string");
    }

    #[test]
    fn test_strip_schema_metadata_preserves_non_metadata() {
        let schema = json!({
            "$id": "gts://test",
            "type": "object",
            "properties": {"field": {"type": "string"}},
            "required": ["field"],
            "additionalProperties": false
        });

        let result = strip_schema_metadata(&schema);

        assert_eq!(result.get("type").unwrap(), "object");
        assert!(result.get("properties").is_some());
        assert!(result.get("required").is_some());
        assert_eq!(result.get("additionalProperties").unwrap(), &json!(false));
    }

    #[test]
    fn test_build_gts_allof_schema_structure() {
        let properties = json!({"field1": {"type": "string"}});
        let required = vec!["field1"];

        let result = build_gts_allof_schema(
            "vendor.package.namespace.child.1",
            "vendor.package.namespace.base.1",
            "Child Schema",
            &properties,
            &required,
        );

        assert_eq!(
            result.get("$id").unwrap(),
            "gts://vendor.package.namespace.child.1"
        );
        assert_eq!(
            result.get("$schema").unwrap(),
            "http://json-schema.org/draft-07/schema#"
        );
        assert_eq!(result.get("title").unwrap(), "Child Schema");
        assert_eq!(result.get("type").unwrap(), "object");

        let allof = result.get("allOf").unwrap().as_array().unwrap();
        assert_eq!(allof.len(), 2);
    }

    #[test]
    fn test_build_gts_allof_schema_ref_format() {
        let properties = json!({"field1": {"type": "string"}});
        let required = vec!["field1"];

        let result = build_gts_allof_schema(
            "vendor.package.namespace.child.1",
            "vendor.package.namespace.base.1",
            "Child Schema",
            &properties,
            &required,
        );

        let allof = result.get("allOf").unwrap().as_array().unwrap();
        let ref_obj = &allof[0];

        assert_eq!(
            ref_obj.get("$ref").unwrap(),
            "gts://vendor.package.namespace.base.1"
        );
    }

    #[test]
    fn test_build_gts_allof_schema_properties_in_allof() {
        let properties = json!({"field1": {"type": "string"}, "field2": {"type": "number"}});
        let required = vec!["field1", "field2"];

        let result = build_gts_allof_schema(
            "vendor.package.namespace.child.1",
            "vendor.package.namespace.base.1",
            "Child Schema",
            &properties,
            &required,
        );

        let allof = result.get("allOf").unwrap().as_array().unwrap();
        let props_obj = &allof[1];

        assert_eq!(props_obj.get("type").unwrap(), "object");
        assert_eq!(props_obj.get("properties").unwrap(), &properties);
        assert_eq!(props_obj.get("required").unwrap(), &json!(required));
    }
}
