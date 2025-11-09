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

    #[test]
    fn test_check_schema_compatibility_enum_reordered() {
        let old_schema = json!({
            "type": "string",
            "enum": ["a", "b", "c"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["c", "a", "b"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.is_backward_compatible);
        assert!(result.is_forward_compatible);
        assert!(result.is_fully_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_nested_required_added() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    },
                    "required": ["name"]
                }
            },
            "required": ["user"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string"}
                    },
                    "required": ["name", "email"]
                }
            },
            "required": ["user"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding nested required is not backward compatible
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_allof_flatten_equivalence() {
        let direct = json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "value": {"type": "number"}
            },
            "required": ["id"]
        });

        let via_allof = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
                },
                {
                    "type": "object",
                    "properties": {"value": {"type": "number"}}
                }
            ]
        });

        // Either direction should be fully compatible
        let r1 = check_schema_compatibility(&direct, &via_allof);
        assert!(r1.is_backward_compatible);
        assert!(r1.is_forward_compatible);
        assert!(r1.is_fully_compatible);

        let r2 = check_schema_compatibility(&via_allof, &direct);
        assert!(r2.is_backward_compatible);
        assert!(r2.is_forward_compatible);
        assert!(r2.is_fully_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_removed_required_is_forward_incompatible() {
        let old_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}}
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Removing required is forward-incompatible per current logic
        assert!(!result.is_forward_compatible);
    }

    #[test]
    fn test_cast_adds_defaults_and_updates_gtsid_const() {
        // Instance is missing optional 'region' and has an outdated GTS id const in 'typeRef'
        let from_instance_id = "gts.vendor.pkg.ns.type.v1.0";
        let from_instance = json!({
            "name": "alice",
            "typeRef": "gts.vendor.pkg.ns.subtype.v1.0~"
        });

        // From schema (minimal)
        let from_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "typeRef": {"type": "string"}
            }
        });

        // To schema has default for optional 'region' and const for 'typeRef' to a newer ID
        let to_schema_id = "gts.vendor.pkg.ns.type.v1.1";
        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "region": {"type": "string", "default": "us-east"},
                "typeRef": {"type": "string", "const": "gts.vendor.pkg.ns.subtype.v1.1~"}
            }
        });

        let cast = GtsEntityCastResult::cast(
            from_instance_id,
            to_schema_id,
            &from_instance,
            &from_schema,
            &to_schema,
            None,
        )
        .expect("cast ok");

        // Defaults should be added
        assert!(cast.added_properties.iter().any(|p| p == "region"));

        let casted = cast.casted_entity.expect("casted entity");
        assert_eq!(casted.get("region").and_then(|v| v.as_str()), Some("us-east"));
        // typeRef should be updated to the const GTS ID
        assert_eq!(
            casted.get("typeRef").and_then(|v| v.as_str()),
            Some("gts.vendor.pkg.ns.subtype.v1.1~")
        );
    }

    #[test]
    fn test_cast_removes_additional_properties_when_disallowed() {
        let from_instance_id = "gts.vendor.pkg.ns.type.v1.0";
        let from_instance = json!({
            "name": "alice",
            "extra": 123
        });

        let from_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}}
        });

        let to_schema_id = "gts.vendor.pkg.ns.type.v1.1";
        let to_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}}
        });

        let cast = GtsEntityCastResult::cast(
            from_instance_id,
            to_schema_id,
            &from_instance,
            &from_schema,
            &to_schema,
            None,
        )
        .expect("cast ok");

        // 'extra' should be removed
        let casted = cast.casted_entity.expect("casted entity");
        assert!(casted.get("extra").is_none());
        assert!(cast.removed_properties.iter().any(|p| p == "extra"));
    }
}
