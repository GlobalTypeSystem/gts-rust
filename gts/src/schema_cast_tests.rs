#[cfg(test)]
mod tests {
    use crate::schema_cast::*;
    use serde_json::json;

    #[test]
    fn test_schema_cast_error_display() {
        let error = SchemaCastError::IncompatibleSchemas("test error".to_string());
        assert!(error.to_string().contains("test error"));

        let error = SchemaCastError::SchemaNotFound("schema_id".to_string());
        assert!(error.to_string().contains("schema_id"));

        let error = SchemaCastError::ValidationFailed("validation error".to_string());
        assert!(error.to_string().contains("validation error"));
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_up() {
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v2.0"
        );
        assert_eq!(direction, "up");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_down() {
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v2.0",
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert_eq!(direction, "down");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_lateral() {
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.other.v1.0"
        );
        assert_eq!(direction, "lateral");
    }

    #[test]
    fn test_json_entity_cast_result_to_dict() {
        let result = GtsEntityCastResult {
            from_id: "gts.vendor.package.namespace.type.v1.0".to_string(),
            to_id: "gts.vendor.package.namespace.type.v2.0".to_string(),
            direction: "up".to_string(),
            ok: true,
            error: String::new(),
            is_backward_compatible: true,
            is_forward_compatible: false,
            is_fully_compatible: false,
        };

        let dict = result.to_dict();
        assert_eq!(dict.get("from_id").unwrap().as_str().unwrap(), "gts.vendor.package.namespace.type.v1.0");
        assert_eq!(dict.get("to_id").unwrap().as_str().unwrap(), "gts.vendor.package.namespace.type.v2.0");
        assert_eq!(dict.get("direction").unwrap().as_str().unwrap(), "up");
        assert_eq!(dict.get("ok").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn test_check_schema_compatibility_identical() {
        let schema1 = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&schema1, &schema1);
        assert!(result.is_backward_compatible);
        assert!(result.is_forward_compatible);
        assert!(result.is_fully_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_added_optional_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding optional property is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_added_required_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["name", "email"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding required property is not backward compatible
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_removed_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Removing property is not forward compatible
        assert!(!result.is_forward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_enum_expansion() {
        let old_schema = json!({
            "type": "string",
            "enum": ["active", "inactive"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["active", "inactive", "pending"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Enum expansion: forward compatible but not backward
        assert!(result.is_forward_compatible);
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_enum_reduction() {
        let old_schema = json!({
            "type": "string",
            "enum": ["active", "inactive", "pending"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["active", "inactive"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Enum reduction: backward compatible but not forward
        assert!(result.is_backward_compatible);
        assert!(!result.is_forward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_type_change() {
        let old_schema = json!({
            "type": "string"
        });

        let new_schema = json!({
            "type": "number"
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Type change is incompatible
        assert!(!result.is_backward_compatible);
        assert!(!result.is_forward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_constraint_tightening() {
        let old_schema = json!({
            "type": "number",
            "minimum": 0
        });

        let new_schema = json!({
            "type": "number",
            "minimum": 10
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Tightening minimum is not backward compatible
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_constraint_relaxing() {
        let old_schema = json!({
            "type": "number",
            "maximum": 100
        });

        let new_schema = json!({
            "type": "number",
            "maximum": 200
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Relaxing maximum is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_nested_objects() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string"}
                    }
                }
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding optional nested property is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_array_items() {
        let old_schema = json!({
            "type": "array",
            "items": {"type": "string"}
        });

        let new_schema = json!({
            "type": "array",
            "items": {"type": "number"}
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Changing array item type is incompatible
        assert!(!result.is_backward_compatible);
        assert!(!result.is_forward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_string_length_constraints() {
        let old_schema = json!({
            "type": "string",
            "minLength": 1,
            "maxLength": 100
        });

        let new_schema = json!({
            "type": "string",
            "minLength": 5,
            "maxLength": 50
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Tightening string constraints is not backward compatible
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_array_length_constraints() {
        let old_schema = json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 10
        });

        let new_schema = json!({
            "type": "array",
            "minItems": 2,
            "maxItems": 5
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Tightening array constraints is not backward compatible
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_compatibility_result_default() {
        let result = CompatibilityResult::default();
        assert!(!result.is_backward_compatible);
        assert!(!result.is_forward_compatible);
        assert!(!result.is_fully_compatible);
    }

    #[test]
    fn test_compatibility_result_fully_compatible() {
        let result = CompatibilityResult {
            is_backward_compatible: true,
            is_forward_compatible: true,
            is_fully_compatible: true,
        };
        assert!(result.is_fully_compatible);
    }
}
