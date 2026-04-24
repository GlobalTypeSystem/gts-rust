//! Test: Base structs with invalid ID field types but valid GTS Type fields should compile

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::nonminimal_bool,
    clippy::uninlined_format_args,
    clippy::bool_assert_comparison
)]

use gts::gts::GtsSchemaId;
use gts_macros::struct_to_gts_schema;
use uuid::Uuid;

/* ============================================================
Mixed validation tests - invalid ID fields but valid GTS Type fields
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type with invalid ID field but valid GTS Type field",
    properties = "id,r#type,name,description"
)]
#[derive(Debug)]
pub struct TopicV1MixedValidationV1<P> {
    pub id: String,          // Invalid ID field type (String instead of GtsInstanceId)
    pub r#type: GtsSchemaId, // Valid GTS Type field - this should allow the struct to pass
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type with invalid ID field but valid GTS Type field",
    properties = "id,gts_type,name,description"
)]
#[derive(Debug)]
pub struct BaseEventV1MixedV1<P> {
    pub id: Uuid,              // Invalid ID field type (Uuid instead of GtsInstanceId)
    pub gts_type: GtsSchemaId, // Valid GTS Type field - this should allow the struct to pass
    pub name: String,
    pub description: Option<String>,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.schema.v1~",
    description = "Base schema type with invalid ID fields but valid GTS Type field",
    properties = "gts_id,schema,name,description"
)]
#[derive(Debug)]
pub struct BaseSchemaV1MixedV1<P> {
    pub gts_id: String,      // Invalid ID field type (String instead of GtsInstanceId)
    pub schema: GtsSchemaId, // Valid GTS Type field - this should allow the struct to pass
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

/* ============================================================
Tests
============================================================ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixed_validation_struct_compiles() {
        // This should compile because even though the ID field has wrong type,
        // the GTS Type field has the correct type
        let topic = TopicV1MixedValidationV1::<()> {
            id: "invalid-id".to_string(),
            r#type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "Test Topic".to_string(),
            description: Some("Test description".to_string()),
            config: (),
        };

        assert_eq!(topic.name, "Test Topic");
    }

    #[test]
    fn test_mixed_validation_base_event_compiles() {
        // This should compile because even though the ID field has wrong type,
        // the GTS Type field has the correct type
        let event = BaseEventV1MixedV1::<()> {
            id: Uuid::new_v4(),
            gts_type: GtsSchemaId::new("gts.x.core.events.type.v1~"),
            name: "Test Event".to_string(),
            description: Some("Test description".to_string()),
            payload: (),
        };

        assert_eq!(event.name, "Test Event");
    }

    #[test]
    fn test_mixed_validation_base_schema_compiles() {
        // This should compile because even though the ID field has wrong type,
        // the GTS Type field has the correct type
        let schema = BaseSchemaV1MixedV1::<()> {
            gts_id: "invalid-id".to_string(),
            schema: GtsSchemaId::new("gts.x.core.events.schema.v1~"),
            name: "Test Schema".to_string(),
            description: Some("Test description".to_string()),
            config: (),
        };

        assert_eq!(schema.name, "Test Schema");
    }

    #[test]
    fn test_mixed_validation_schema_constants() {
        // Verify that schema constants are generated correctly
        assert_eq!(
            TopicV1MixedValidationV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert_eq!(
            BaseEventV1MixedV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.type.v1~"
        );
        assert_eq!(
            BaseSchemaV1MixedV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.schema.v1~"
        );
    }

    #[test]
    fn test_mixed_validation_serialization() {
        // Test that serialization works correctly
        let topic = TopicV1MixedValidationV1::<()> {
            id: "test-id".to_string(),
            r#type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "Test Topic".to_string(),
            description: Some("Test description".to_string()),
            config: (),
        };

        let json = serde_json::to_string(&topic).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["id"], "test-id");
        assert_eq!(parsed["type"], "gts.x.core.events.topic.v1~");
        assert_eq!(parsed["name"], "Test Topic");
    }
}
