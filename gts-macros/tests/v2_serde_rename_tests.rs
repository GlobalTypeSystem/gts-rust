#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

//! Phase 6: Tests for the serde rename fix in nested struct deserializers.
//!
//! The old macro used `#[serde(field_identifier, rename_all = "snake_case")]` on the
//! field identifier enum, which broke deserialization when fields had `#[serde(rename)]`
//! attributes (e.g., camelCase names). The new macro uses per-field `#[serde(rename)]`
//! on each variant, correctly respecting user renames.

use gts::{GtsSchema, GtsSchemaId};
use gts_macros::GtsSchema;
use schemars::JsonSchema;
use uuid::Uuid;

// =============================================================================
// Test structs
// =============================================================================

/// Base event with a renamed field.
#[derive(Debug, Clone, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type"
)]
pub struct BaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub payload: P,
}

/// Nested struct with camelCase renamed fields.
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.camel.event.v1~",
    description = "Event with camelCase fields",
    extends = BaseEventV1
)]
pub struct CamelPayloadV1 {
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(rename = "ipAddress")]
    pub ip_address: String,
    pub severity: u8,
}

/// Nested struct with mixed renamed and non-renamed fields.
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.mixed.event.v1~",
    description = "Event with mixed renames",
    extends = BaseEventV1
)]
pub struct MixedPayloadV1 {
    #[serde(rename = "userId")]
    pub user_id: Uuid,
    pub message: String,
    #[serde(rename = "errorCode")]
    pub error_code: Option<i32>,
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn nested_camel_case_deserialize() {
    let json = serde_json::json!({
        "type": "gts.x.core.events.type.v1~x.core.camel.event.v1~",
        "id": Uuid::nil().to_string(),
        "payload": {
            "userName": "alice",
            "ipAddress": "10.0.0.1",
            "severity": 3
        }
    });

    let event: BaseEventV1<CamelPayloadV1> = serde_json::from_value(json).unwrap();
    assert_eq!(event.payload.user_name, "alice");
    assert_eq!(event.payload.ip_address, "10.0.0.1");
    assert_eq!(event.payload.severity, 3);
}

#[test]
fn nested_camel_case_serialize() {
    let event = BaseEventV1 {
        event_type: CamelPayloadV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        payload: CamelPayloadV1 {
            user_name: "bob".to_owned(),
            ip_address: "192.168.1.1".to_owned(),
            severity: 5,
        },
    };

    let json = serde_json::to_value(&event).unwrap();
    // camelCase keys should appear in output
    assert_eq!(json["payload"]["userName"], "bob");
    assert_eq!(json["payload"]["ipAddress"], "192.168.1.1");
    // Non-renamed field keeps snake_case
    assert_eq!(json["payload"]["severity"], 5);
    // Should NOT have snake_case keys
    assert!(json["payload"].get("user_name").is_none());
    assert!(json["payload"].get("ip_address").is_none());
}

#[test]
fn nested_mixed_renames() {
    let json = serde_json::json!({
        "type": "gts.x.core.events.type.v1~x.core.mixed.event.v1~",
        "id": Uuid::nil().to_string(),
        "payload": {
            "userId": Uuid::nil().to_string(),
            "message": "hello",
            "errorCode": 42
        }
    });

    let event: BaseEventV1<MixedPayloadV1> = serde_json::from_value(json).unwrap();
    assert_eq!(event.payload.user_id, Uuid::nil());
    assert_eq!(event.payload.message, "hello");
    assert_eq!(event.payload.error_code, Some(42));
}

#[test]
fn nested_rename_roundtrip() {
    let event = BaseEventV1 {
        event_type: CamelPayloadV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        payload: CamelPayloadV1 {
            user_name: "roundtrip".to_owned(),
            ip_address: "127.0.0.1".to_owned(),
            severity: 1,
        },
    };

    let json_str = serde_json::to_string(&event).unwrap();
    let deserialized: BaseEventV1<CamelPayloadV1> = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.payload.user_name, event.payload.user_name);
    assert_eq!(deserialized.payload.ip_address, event.payload.ip_address);
    assert_eq!(deserialized.payload.severity, event.payload.severity);
}

#[test]
fn nested_mixed_rename_roundtrip() {
    let event = BaseEventV1 {
        event_type: MixedPayloadV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        payload: MixedPayloadV1 {
            user_id: Uuid::nil(),
            message: "test".to_owned(),
            error_code: None,
        },
    };

    let json_str = serde_json::to_string(&event).unwrap();
    let deserialized: BaseEventV1<MixedPayloadV1> = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.payload.user_id, event.payload.user_id);
    assert_eq!(deserialized.payload.message, event.payload.message);
    assert_eq!(deserialized.payload.error_code, event.payload.error_code);
}
