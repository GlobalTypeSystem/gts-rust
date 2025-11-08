#[cfg(test)]
mod tests {
    use crate::entities::*;
    use crate::gts::GtsID;
    use serde_json::json;

    #[test]
    fn test_json_file_with_description() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "description": "Test description"
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.description, "Test description");
    }

    #[test]
    fn test_json_entity_with_file_and_sequence() {
        let file_content = json!([
            {"id": "gts.vendor.package.namespace.type.v1.0"},
            {"id": "gts.vendor.package.namespace.type.v1.1"}
        ]);

        let file = GtsFile::new(
            "/path/to/file.json".to_string(),
            "file.json".to_string(),
            file_content,
        );

        let entity_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});
        let cfg = GtsConfig::default();

        let entity = GtsEntity::new(
            Some(file),
            Some(0),
            entity_content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.label, "file.json#0");
    }

    #[test]
    fn test_json_entity_with_file_no_sequence() {
        let file_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let file = GtsFile::new(
            "/path/to/file.json".to_string(),
            "file.json".to_string(),
            file_content,
        );

        let entity_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});
        let cfg = GtsConfig::default();

        let entity = GtsEntity::new(
            Some(file),
            None,
            entity_content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.label, "file.json");
    }

    #[test]
    fn test_json_entity_extract_gts_ids() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "nested": {
                "ref": "gts.other.package.namespace.type.v2.0"
            }
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        let ids = entity.extract_gts_ids();
        assert!(!ids.is_empty());
    }

    #[test]
    fn test_json_entity_extract_ref_strings() {
        let content = json!({
            "$ref": "gts.vendor.package.namespace.type.v1.0~",
            "properties": {
                "user": {
                    "$ref": "gts.other.package.namespace.type.v2.0~"
                }
            }
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        let refs = entity.extract_ref_strings();
        assert!(!refs.is_empty());
    }

    #[test]
    fn test_json_entity_is_json_schema_entity() {
        let schema_content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let entity = GtsEntity::new(
            None,
            None,
            schema_content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert!(entity.is_schema);
    }

    #[test]
    fn test_json_entity_fallback_to_schema_id() {
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        // Should fallback to schema_id when entity_id is not found
        assert!(entity.gts_id.is_some());
    }

    #[test]
    fn test_json_entity_with_custom_label() {
        let content = json!({"name": "test"});

        let entity = GtsEntity::new(
            None,
            None,
            content,
            None,
            None,
            false,
            "custom_label".to_string(),
            None,
            None,
        );

        assert_eq!(entity.label, "custom_label");
    }

    #[test]
    fn test_json_entity_empty_label_fallback() {
        let content = json!({"name": "test"});

        let entity = GtsEntity::new(
            None,
            None,
            content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.label, "");
    }

    #[test]
    fn test_validation_result_default() {
        let result = ValidationResult::default();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validation_error_creation() {
        let mut params = std::collections::HashMap::new();
        params.insert("key".to_string(), json!("value"));

        let error = ValidationError {
            instance_path: "/path".to_string(),
            schema_path: "/schema".to_string(),
            keyword: "required".to_string(),
            message: "test error".to_string(),
            params,
            data: Some(json!({"test": "data"})),
        };

        assert_eq!(error.instance_path, "/path");
        assert_eq!(error.message, "test error");
        assert!(error.data.is_some());
    }

    #[test]
    fn test_gts_config_entity_id_fields() {
        let cfg = GtsConfig::default();
        assert!(cfg.entity_id_fields.contains(&"id".to_string()));
        assert!(cfg.entity_id_fields.contains(&"$id".to_string()));
        assert!(cfg.entity_id_fields.contains(&"gtsId".to_string()));
    }

    #[test]
    fn test_gts_config_schema_id_fields() {
        let cfg = GtsConfig::default();
        assert!(cfg.schema_id_fields.contains(&"type".to_string()));
        assert!(cfg.schema_id_fields.contains(&"$schema".to_string()));
        assert!(cfg.schema_id_fields.contains(&"gtsTid".to_string()));
    }

    #[test]
    fn test_json_entity_with_validation_result() {
        let content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let mut validation = ValidationResult::default();
        validation.errors.push(ValidationError {
            instance_path: "/test".to_string(),
            schema_path: "/schema/test".to_string(),
            keyword: "type".to_string(),
            message: "validation error".to_string(),
            params: std::collections::HashMap::new(),
            data: None,
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            None,
            None,
            false,
            String::new(),
            Some(validation.clone()),
            None,
        );

        assert_eq!(entity.validation.errors.len(), 1);
    }

    #[test]
    fn test_json_entity_schema_id_field_selection() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~"
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert!(entity.selected_schema_id_field.is_some());
    }

    #[test]
    fn test_json_entity_when_id_is_schema() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#"
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        // When entity ID itself is a schema, selected_schema_id_field should be None
        assert_eq!(entity.selected_schema_id_field, None);
    }
}
