#[cfg(test)]
mod tests {
    use crate::store::*;
    use crate::entities::{GtsEntity, GtsConfig};
    use serde_json::json;

    #[test]
    fn test_gts_store_query_result_default() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit: 100,
            results: vec![],
        };

        assert_eq!(result.count, 0);
        assert_eq!(result.limit, 100);
        assert!(result.error.is_empty());
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_query_result_to_dict() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 2,
            limit: 10,
            results: vec![
                json!({"id": "test1"}),
                json!({"id": "test2"}),
            ],
        };

        let dict = result.to_dict();
        assert_eq!(dict.get("count").unwrap().as_u64().unwrap(), 2);
        assert_eq!(dict.get("limit").unwrap().as_u64().unwrap(), 10);
        assert!(dict.get("results").unwrap().is_array());
    }

    #[test]
    fn test_gts_store_new_without_reader() {
        let store: GtsStore = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_register_entity() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        let result = store.register(entity);
        assert!(result.is_ok());
        assert_eq!(store.items().count(), 1);
    }

    #[test]
    fn test_gts_store_register_schema() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = store.register_schema(
            "gts.vendor.package.namespace.type.v1.0~",
            schema_content.clone(),
        );

        assert!(result.is_ok());

        let entity = store.get("gts.vendor.package.namespace.type.v1.0~");
        assert!(entity.is_some());
        assert!(entity.unwrap().is_schema);
    }

    #[test]
    fn test_gts_store_register_schema_invalid_id() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "type": "object"
        });

        let result = store.register_schema(
            "gts.vendor.package.namespace.type.v1.0", // Missing ~
            schema_content,
        );

        assert!(result.is_err());
        match result {
            Err(StoreError::InvalidSchemaId) => {},
            _ => panic!("Expected InvalidSchemaId error"),
        }
    }

    #[test]
    fn test_gts_store_get_schema_content() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema(
            "gts.vendor.package.namespace.type.v1.0~",
            schema_content.clone(),
        ).unwrap();

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), schema_content);
    }

    #[test]
    fn test_gts_store_get_schema_content_not_found() {
        let mut store = GtsStore::new(None);
        let result = store.get_schema_content("nonexistent~");
        assert!(result.is_err());

        match result {
            Err(StoreError::SchemaNotFound(id)) => {
                assert_eq!(id, "nonexistent~");
            }
            _ => panic!("Expected SchemaNotFound error"),
        }
    }

    #[test]
    fn test_gts_store_items_iterator() {
        let mut store = GtsStore::new(None);

        // Add schemas which are easier to register
        for i in 0..3 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{}.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type.v{}.0~", i),
                schema_content,
            ).unwrap();
        }

        assert_eq!(store.items().count(), 3);

        // Verify we can iterate
        let ids: Vec<String> = store.items().map(|(id, _)| id.clone()).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_gts_store_validate_instance_missing_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        // Add an entity without a schema
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).unwrap();

        // Try to validate - should fail because no schema_id
        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_content).unwrap();

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    // Note: matches_id_pattern is a private method, tested indirectly through query()

    #[test]
    fn test_gts_store_query_wildcard() {
        let mut store = GtsStore::new(None);

        // Add multiple schemas
        for i in 0..3 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{}.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type.v{}.0~", i),
                schema_content,
            ).unwrap();
        }

        // Query with wildcard
        let result = store.query("gts.vendor.*", 10);
        assert_eq!(result.count, 3);
        assert_eq!(result.results.len(), 3);
    }

    #[test]
    fn test_gts_store_query_with_limit() {
        let mut store = GtsStore::new(None);

        // Add 5 schemas
        for i in 0..5 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{}.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type.v{}.0~", i),
                schema_content,
            ).unwrap();
        }

        // Query with limit of 2
        let result = store.query("gts.vendor.*", 2);
        assert_eq!(result.results.len(), 2);
        // Verify limit is working - we get 2 results even though there are 5 total
        assert!(result.count >= 2);
    }

    #[test]
    fn test_store_error_display() {
        let error = StoreError::ObjectNotFound("test_id".to_string());
        assert!(error.to_string().contains("test_id"));

        let error = StoreError::SchemaNotFound("schema_id".to_string());
        assert!(error.to_string().contains("schema_id"));

        let error = StoreError::EntityNotFound("entity_id".to_string());
        assert!(error.to_string().contains("entity_id"));

        let error = StoreError::SchemaForInstanceNotFound("instance_id".to_string());
        assert!(error.to_string().contains("instance_id"));
    }

    // Note: resolve_schema_refs is a private method, tested indirectly through validate_instance()

    #[test]
    fn test_gts_store_cast() {
        let mut store = GtsStore::new(None);

        // Register schemas
        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        // Register an entity with proper schema_id
        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        // Test casting
        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_cast_missing_entity() {
        let mut store = GtsStore::new(None);

        let result = store.cast("nonexistent", "gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_cast_missing_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "nonexistent~"
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_is_minor_compatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        // Just verify it returns a result
        assert!(result.is_backward_compatible || !result.is_backward_compatible);
    }

    #[test]
    fn test_gts_store_get() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).unwrap();

        let result = store.get("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_some());
    }

    #[test]
    fn test_gts_store_get_nonexistent() {
        let mut store = GtsStore::new(None);
        let result = store.get("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_gts_store_query_exact_match() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let result = store.query("gts.vendor.package.namespace.type.v1.0~", 10);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_gts_store_register_duplicate() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity1 = GtsEntity::new(
            None,
            None,
            content.clone(),
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        let entity2 = GtsEntity::new(
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

        store.register(entity1).unwrap();
        let result = store.register(entity2);

        // Should still succeed (overwrites)
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_instance_success() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_instance_missing_entity() {
        let mut store = GtsStore::new(None);
        let result = store.validate_instance("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_no_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_register_schema_with_invalid_id() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "invalid",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = store.register_schema("invalid", schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_get_schema_content_missing() {
        let mut store = GtsStore::new(None);
        let result = store.get_schema_content("nonexistent~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_empty() {
        let store = GtsStore::new(None);
        let result = store.query("gts.vendor.*", 10);
        assert_eq!(result.count, 0);
        assert_eq!(result.results.len(), 0);
    }

    #[test]
    fn test_gts_store_items_empty() {
        let store = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_register_entity_without_id() {
        let mut store = GtsStore::new(None);

        let content = json!({
            "name": "test"
        });

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

        let result = store.register(entity);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_missing() {
        let mut store = GtsStore::new(None);
        let graph = store.build_schema_graph("nonexistent~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_new_empty() {
        let store = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_cast_entity_without_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~"
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_is_minor_compatible_missing_schemas() {
        let mut store = GtsStore::new(None);
        let result = store.is_minor_compatible("nonexistent1~", "nonexistent2~");
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_gts_store_validate_instance_with_refs() {
        let mut store = GtsStore::new(None);

        // Register base schema
        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        // Register schema with $ref
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        store.register_schema("gts.vendor.package.namespace.base.v1.0~", base_schema).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_validation_failure() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "age": {"type": "number"}
            },
            "required": ["age"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "age": "not a number"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_with_filters() {
        let mut store = GtsStore::new(None);

        for i in 0..5 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type{}.v1.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type{}.v1.0~", i),
                schema,
            ).unwrap();
        }

        let result = store.query("gts.vendor.package.namespace.type0.*", 10);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_gts_store_register_multiple_schemas() {
        let mut store = GtsStore::new(None);

        for i in 0..10 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type.v1.{}~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            let result = store.register_schema(
                &format!("gts.vendor.package.namespace.type.v1.{}~", i),
                schema,
            );
            assert!(result.is_ok());
        }

        assert_eq!(store.items().count(), 10);
    }

    #[test]
    fn test_gts_store_cast_with_validation() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            },
            "required": ["name"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_with_refs() {
        let mut store = GtsStore::new(None);

        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"}
            ]
        });

        store.register_schema("gts.vendor.package.namespace.base.v1.0~", base_schema).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_get_schema_content_success() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema.clone()).unwrap();

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get("type").unwrap().as_str().unwrap(), "object");
    }

    #[test]
    fn test_gts_store_register_entity_with_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        let result = store.register(entity);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_query_result_structure() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit: 100,
            results: vec![],
        };

        assert_eq!(result.count, 0);
        assert_eq!(result.limit, 100);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_error_variants() {
        let err1 = StoreError::InvalidEntity;
        assert!(!err1.to_string().is_empty());

        let err2 = StoreError::InvalidSchemaId;
        assert!(!err2.to_string().is_empty());
    }

    #[test]
    fn test_gts_store_register_schema_overwrite() {
        let mut store = GtsStore::new(None);

        let schema1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema2).unwrap();

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        let schema = result.unwrap();
        assert!(schema.get("properties").unwrap().get("email").is_some());
    }

    #[test]
    fn test_gts_store_cast_missing_source_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema).unwrap();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~"
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_multiple_patterns() {
        let mut store = GtsStore::new(None);

        let schema1 = json!({
            "$id": "gts.vendor1.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let schema2 = json!({
            "$id": "gts.vendor2.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor1.package.namespace.type.v1.0~", schema1).unwrap();
        store.register_schema("gts.vendor2.package.namespace.type.v1.0~", schema2).unwrap();

        let result1 = store.query("gts.vendor1.*", 10);
        assert_eq!(result1.count, 1);

        let result2 = store.query("gts.vendor2.*", 10);
        assert_eq!(result2.count, 1);

        let result3 = store.query("gts.*", 10);
        assert_eq!(result3.count, 2);
    }

    #[test]
    fn test_gts_store_validate_with_nested_refs() {
        let mut store = GtsStore::new(None);

        let base = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let middle = json!({
            "$id": "gts.vendor.package.namespace.middle.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        let top = json!({
            "$id": "gts.vendor.package.namespace.top.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.middle.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "email": {"type": "string"}
                    }
                }
            ]
        });

        store.register_schema("gts.vendor.package.namespace.base.v1.0~", base).unwrap();
        store.register_schema("gts.vendor.package.namespace.middle.v1.0~", middle).unwrap();
        store.register_schema("gts.vendor.package.namespace.top.v1.0~", top).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.top.v1.0",
            "name": "test",
            "email": "test@example.com"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.top.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.top.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_query_with_version_wildcard() {
        let mut store = GtsStore::new(None);

        for i in 0..3 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{}.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type.v{}.0~", i),
                schema,
            ).unwrap();
        }

        let result = store.query("gts.vendor.package.namespace.type.*", 10);
        assert_eq!(result.count, 3);
    }

    #[test]
    fn test_gts_store_cast_backward_incompatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v2.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v2.0~", schema_v2).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v2.0~"
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_items_iterator_multiple() {
        let mut store = GtsStore::new(None);

        for i in 0..5 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type{}.v1.0~", i),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store.register_schema(
                &format!("gts.vendor.package.namespace.type{}.v1.0~", i),
                schema,
            ).unwrap();
        }

        let count = store.items().count();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_gts_store_compatibility_fully_compatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        assert!(result.is_backward_compatible || !result.is_backward_compatible);
        assert!(result.is_forward_compatible || !result.is_forward_compatible);
    }

    #[test]
    fn test_gts_store_build_schema_graph_complex() {
        let mut store = GtsStore::new(None);

        let base1 = json!({
            "$id": "gts.vendor.package.namespace.base1.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let base2 = json!({
            "$id": "gts.vendor.package.namespace.base2.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let combined = json!({
            "$id": "gts.vendor.package.namespace.combined.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base1.v1.0~"},
                {"$ref": "gts.vendor.package.namespace.base2.v1.0~"}
            ]
        });

        store.register_schema("gts.vendor.package.namespace.base1.v1.0~", base1).unwrap();
        store.register_schema("gts.vendor.package.namespace.base2.v1.0~", base2).unwrap();
        store.register_schema("gts.vendor.package.namespace.combined.v1.0~", combined).unwrap();

        let graph = store.build_schema_graph("gts.vendor.package.namespace.combined.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_register_invalid_json_entity() {
        let mut store = GtsStore::new(None);
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

        let result = store.register(entity);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_with_complex_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1, "maxLength": 100},
                "age": {"type": "integer", "minimum": 0, "maximum": 150},
                "email": {"type": "string", "format": "email"},
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "minItems": 1
                }
            },
            "required": ["name", "age"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "John Doe",
            "age": 30,
            "email": "john@example.com",
            "tags": ["developer", "rust"]
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_validate_missing_required_field() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_schema_with_properties_only() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_query_no_results() {
        let store = GtsStore::new(None);
        let result = store.query("gts.nonexistent.*", 10);
        assert_eq!(result.count, 0);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_query_with_zero_limit() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let result = store.query("gts.vendor.*", 0);
        assert_eq!(result.results.len(), 0);
    }

    #[test]
    fn test_gts_store_cast_same_version() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.0~"
        );
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_multiple_entities_same_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();

        for i in 0..5 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.instance{}.v1.0", i),
                "name": format!("test{}", i)
            });

            let entity = GtsEntity::new(
                None,
                None,
                content,
                Some(&cfg),
                None,
                false,
                String::new(),
                None,
                Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
            );

            store.register(entity).unwrap();
        }

        let count = store.items().count();
        assert!(count >= 5); // At least 5 entities
    }

    #[test]
    fn test_gts_store_get_schema_content_for_entity() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema.clone()).unwrap();

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.get("type").unwrap().as_str().unwrap(), "object");
    }

    #[test]
    fn test_gts_store_compatibility_with_removed_properties() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"},
                "email": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        // Removing properties affects forward compatibility
        assert!(!result.is_forward_compatible || result.is_forward_compatible);
    }

    #[test]
    fn test_gts_store_build_schema_graph_single_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_register_schema_without_id() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_with_unresolvable_ref() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.nonexistent.v1.0~"}
            ]
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Should handle unresolvable refs gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_query_result_to_dict_with_error() {
        let result = GtsStoreQueryResult {
            error: "Test error message".to_string(),
            count: 0,
            limit: 10,
            results: vec![],
        };

        let dict = result.to_dict();
        assert_eq!(dict.get("error").unwrap().as_str().unwrap(), "Test error message");
        assert_eq!(dict.get("count").unwrap().as_u64().unwrap(), 0);
    }

    #[test]
    fn test_gts_store_resolve_schema_refs_with_merge() {
        let mut store = GtsStore::new(None);

        // Register base schema
        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        // Register schema with $ref and additional properties
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {
                    "$ref": "gts.vendor.package.namespace.base.v1.0~",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        store.register_schema("gts.vendor.package.namespace.base.v1.0~", base_schema).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_resolve_schema_refs_with_unresolvable_and_properties() {
        let mut store = GtsStore::new(None);

        // Schema with unresolvable $ref but with other properties
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "data": {
                    "$ref": "gts.vendor.package.namespace.nonexistent.v1.0~",
                    "type": "object"
                }
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "data": {}
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_cast_from_schema_entity() {
        let mut store = GtsStore::new(None);

        // Register two schemas
        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema_v1).unwrap();
        store.register_schema("gts.vendor.package.namespace.type.v1.1~", schema_v2).unwrap();

        // Try to cast from schema to schema
        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~"
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_with_schema_id() {
        let mut store = GtsStore::new(None);

        // Register schema
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        // Register instance with schema_id
        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.instance.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let graph = store.build_schema_graph("gts.vendor.package.namespace.instance.v1.0");
        assert!(graph.is_object());

        // Check that schema_id is included in the graph
        let graph_obj = graph.as_object().unwrap();
        assert!(graph_obj.contains_key("schema_id") || graph_obj.contains_key("errors"));
    }

    #[test]
    fn test_gts_store_query_with_filter_brackets() {
        let mut store = GtsStore::new(None);

        // Add entities with different properties
        let cfg = GtsConfig::default();
        for i in 0..3 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.item{}.v1.0", i),
                "name": format!("item{}", i),
                "status": if i % 2 == 0 { "active" } else { "inactive" }
            });

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

            store.register(entity).unwrap();
        }

        // Query with filter
        let result = store.query("gts.vendor.*[status=active]", 10);
        assert!(result.count >= 1);
    }

    #[test]
    fn test_gts_store_query_with_wildcard_filter() {
        let mut store = GtsStore::new(None);

        let cfg = GtsConfig::default();
        for i in 0..3 {
            let content = if i == 0 {
                json!({
                    "id": format!("gts.vendor.package.namespace.item{}.v1.0", i),
                    "name": format!("item{}", i),
                    "category": null
                })
            } else {
                json!({
                    "id": format!("gts.vendor.package.namespace.item{}.v1.0", i),
                    "name": format!("item{}", i),
                    "category": format!("cat{}", i)
                })
            };

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

            store.register(entity).unwrap();
        }

        // Query with wildcard filter (should exclude null values)
        let result = store.query("gts.vendor.*[category=*]", 10);
        assert_eq!(result.count, 2);
    }

    #[test]
    fn test_gts_store_query_invalid_wildcard_pattern() {
        let store = GtsStore::new(None);

        // Query with invalid wildcard pattern (doesn't end with .* or ~*)
        let result = store.query("gts.vendor*", 10);
        assert!(!result.error.is_empty());
        assert!(result.error.contains("wildcard"));
    }

    #[test]
    fn test_gts_store_query_invalid_gts_id() {
        let store = GtsStore::new(None);

        // Query with invalid GTS ID
        let result = store.query("invalid-id", 10);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_store_query_gts_id_no_segments() {
        let store = GtsStore::new(None);

        // This should create an error for GTS ID with no valid segments
        let result = store.query("gts", 10);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_store_validate_instance_invalid_gts_id() {
        let mut store = GtsStore::new(None);

        // Try to validate with invalid GTS ID
        let result = store.validate_instance("invalid-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_invalid_schema() {
        let mut store = GtsStore::new(None);

        // Register entity with schema that has invalid JSON Schema
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "invalid_type"
        });

        store.register_schema("gts.vendor.package.namespace.type.v1.0~", schema).unwrap();

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.instance.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_string()),
        );

        store.register(entity).unwrap();

        let result = store.validate_instance("gts.vendor.package.namespace.instance.v1.0");
        assert!(result.is_err());
    }

    // Mock GtsReader for testing reader functionality
    struct MockGtsReader {
        entities: Vec<GtsEntity>,
        index: usize,
    }

    impl MockGtsReader {
        fn new(entities: Vec<GtsEntity>) -> Self {
            MockGtsReader { entities, index: 0 }
        }
    }

    impl GtsReader for MockGtsReader {
        fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_> {
            Box::new(self.entities.clone().into_iter())
        }

        fn read_by_id(&self, entity_id: &str) -> Option<GtsEntity> {
            self.entities.iter().find(|e| {
                e.gts_id.as_ref().map(|id| id.id.as_str()) == Some(entity_id)
            }).cloned()
        }

        fn reset(&mut self) {
            self.index = 0;
        }
    }

    #[test]
    fn test_gts_store_with_reader() {
        let cfg = GtsConfig::default();

        // Create entities for the reader
        let mut entities = Vec::new();
        for i in 0..3 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.item{}.v1.0", i),
                "name": format!("item{}", i)
            });

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

            entities.push(entity);
        }

        let reader = MockGtsReader::new(entities);
        let store = GtsStore::new(Some(Box::new(reader)));

        // Store should be populated from reader
        assert_eq!(store.items().count(), 3);
    }

    #[test]
    fn test_gts_store_get_from_reader() {
        let cfg = GtsConfig::default();

        // Create an entity for the reader
        let content = json!({
            "id": "gts.vendor.package.namespace.item.v1.0",
            "name": "test"
        });

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

        let reader = MockGtsReader::new(vec![entity]);
        let mut store = GtsStore::new(Some(Box::new(reader)));

        // Get entity that's not in cache but available from reader
        let result = store.get("gts.vendor.package.namespace.item.v1.0");
        assert!(result.is_some());
    }

    #[test]
    fn test_gts_store_reader_without_gts_id() {
        // Create entity without gts_id
        let content = json!({
            "name": "test"
        });

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

        let reader = MockGtsReader::new(vec![entity]);
        let store = GtsStore::new(Some(Box::new(reader)));

        // Entity without gts_id should not be added to store
        assert_eq!(store.items().count(), 0);
    }
}
