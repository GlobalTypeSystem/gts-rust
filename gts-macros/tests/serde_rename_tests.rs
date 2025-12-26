//! Test: Serde rename attribute handling for GTS Type fields

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
Serde rename tests - event_type field with serde(rename = "type")
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type with serde(rename = \"type\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1SerdeRenameV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.gts_type.v1~",
    description = "Base event type with serde(rename = \"gts_type\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1GtsTypeRenameV1<P> {
    #[serde(rename = "gts_type")]
    pub event_type: GtsSchemaId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.schema.v1~",
    description = "Base event type with serde(rename = \"schema\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1SchemaRenameV1<P> {
    #[serde(rename = "schema")]
    pub event_type: GtsSchemaId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

/* ============================================================
Tests
============================================================ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_rename_type_compiles() {
        // This should compile because event_type is renamed to "type" and has correct type
        let event = BaseEventV1SerdeRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.type.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(event.event_type.to_string(), "gts.x.core.events.type.v1~");
    }

    #[test]
    fn test_serde_rename_gts_type_compiles() {
        // This should compile because event_type is renamed to "gts_type" and has correct type
        let event = BaseEventV1GtsTypeRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.gts_type.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(
            event.event_type.to_string(),
            "gts.x.core.events.gts_type.v1~"
        );
    }

    #[test]
    fn test_serde_rename_schema_compiles() {
        // This should compile because event_type is renamed to "schema" and has correct type
        let event = BaseEventV1SchemaRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.schema.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(event.event_type.to_string(), "gts.x.core.events.schema.v1~");
    }

    #[test]
    fn test_serde_rename_schema_constants() {
        // Verify that schema constants are generated correctly
        assert_eq!(
            BaseEventV1SerdeRenameV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.type.v1~"
        );
        assert_eq!(
            BaseEventV1GtsTypeRenameV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.gts_type.v1~"
        );
        assert_eq!(
            BaseEventV1SchemaRenameV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.schema.v1~"
        );
    }

    #[test]
    fn test_serde_rename_serialization() {
        // Test that serialization works correctly with serde rename
        let event = BaseEventV1SerdeRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.type.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "type" (not "event_type")
        assert!(parsed.get("type").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["type"], "gts.x.core.events.type.v1~");
    }

    #[test]
    fn test_serde_rename_gts_type_serialization() {
        // Test that serialization works correctly with serde rename to gts_type
        let event = BaseEventV1GtsTypeRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.gts_type.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "gts_type" (not "event_type")
        assert!(parsed.get("gts_type").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["gts_type"], "gts.x.core.events.gts_type.v1~");
    }

    #[test]
    fn test_serde_rename_schema_serialization() {
        // Test that serialization works correctly with serde rename to schema
        let event = BaseEventV1SchemaRenameV1::<()> {
            event_type: GtsSchemaId::new("gts.x.core.events.schema.v1~"),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "schema" (not "event_type")
        assert!(parsed.get("schema").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["schema"], "gts.x.core.events.schema.v1~");
    }
}
