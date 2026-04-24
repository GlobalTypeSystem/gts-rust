#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::bool_assert_comparison,
    dead_code
)]

use gts::{GtsInstanceId, GtsSchema, GtsSchemaId};
use gts_macros::GtsSchema;
use schemars::JsonSchema;
use uuid::Uuid;

// =============================================================================
// Test structs — 3-level hierarchy
// =============================================================================

/// Level 1: Base event (root, generic)
#[derive(Debug, Clone, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type"
)]
pub struct BaseEventV1<P: GtsSchema + JsonSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

/// Level 2: Audit payload (nested, generic)
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    extends = BaseEventV1
)]
pub struct AuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub user_id: Uuid,
    pub ip_address: String,
    pub data: D,
}

/// Level 3: Place order data (nested, non-generic, leaf)
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    description = "Order placement audit event",
    extends = AuditPayloadV1
)]
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}

/// Non-generic base struct (no serde injection needed)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Event topic definition"
)]
pub struct EventTopicV1 {
    #[gts(instance_id)]
    pub id: GtsInstanceId,
    pub name: String,
    pub description: Option<String>,
    pub retention: String,
}

/// Data entity (anonymous instance) with a `type_field` identity
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.commerce.orders.order.v1.0~",
    description = "Order data entity"
)]
pub struct OrderV1_0 {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub gts_type: GtsSchemaId,
    pub id: Uuid,
    pub product_id: Uuid,
    pub quantity: u32,
    pub total: f64,
}

// =============================================================================
// Tests: Base struct serialization (non-generic)
// =============================================================================

#[test]
fn base_struct_serialize() {
    let topic = EventTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.marketplace._.orders.v1"),
        name: "orders".to_string(),
        description: Some("Order events".to_string()),
        retention: "P30D".to_string(),
    };
    let json = serde_json::to_value(&topic).unwrap();
    assert_eq!(json["name"], "orders");
    assert_eq!(json["retention"], "P30D");
}

#[test]
fn base_struct_deserialize() {
    let json = serde_json::json!({
        "id": "gts.x.core.events.topic.v1~x.marketplace._.orders.v1",
        "name": "orders",
        "description": "Order events",
        "retention": "P30D"
    });
    let topic: EventTopicV1 = serde_json::from_value(json).unwrap();
    assert_eq!(topic.name, "orders");
    assert_eq!(topic.retention, "P30D");
}

#[test]
fn base_struct_roundtrip() {
    let topic = EventTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.marketplace._.orders.v1"),
        name: "orders".to_string(),
        description: None,
        retention: "P30D".to_string(),
    };
    let json = serde_json::to_string(&topic).unwrap();
    let deserialized: EventTopicV1 = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, topic.name);
    assert_eq!(deserialized.retention, topic.retention);
}

// =============================================================================
// Tests: Generic base struct serialization (custom Serialize/Deserialize)
// =============================================================================

#[test]
fn base_struct_with_nested_serialize() {
    let order_id = Uuid::nil();
    let product_id = Uuid::nil();
    let user_id = Uuid::nil();
    let tenant_id = Uuid::nil();
    let event_id = Uuid::nil();

    let event = BaseEventV1 {
        event_type: PlaceOrderDataV1::gts_schema_id().clone(),
        id: event_id,
        tenant_id,
        sequence_id: 1,
        payload: AuditPayloadV1 {
            user_agent: "Mozilla/5.0".to_string(),
            user_id,
            ip_address: "127.0.0.1".to_string(),
            data: PlaceOrderDataV1 {
                order_id,
                product_id,
            },
        },
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(
        json["type"],
        "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
    );
    assert_eq!(json["sequence_id"], 1);
    assert_eq!(json["payload"]["user_agent"], "Mozilla/5.0");
    assert_eq!(json["payload"]["ip_address"], "127.0.0.1");
    assert_eq!(json["payload"]["data"]["order_id"], order_id.to_string());
}

#[test]
fn base_struct_with_nested_deserialize() {
    let order_id = Uuid::nil();
    let json = serde_json::json!({
        "type": "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
        "id": Uuid::nil().to_string(),
        "tenant_id": Uuid::nil().to_string(),
        "sequence_id": 42,
        "payload": {
            "user_agent": "curl/7.0",
            "user_id": Uuid::nil().to_string(),
            "ip_address": "10.0.0.1",
            "data": {
                "order_id": order_id.to_string(),
                "product_id": Uuid::nil().to_string()
            }
        }
    });

    let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
        serde_json::from_value(json).unwrap();
    assert_eq!(event.sequence_id, 42);
    assert_eq!(event.payload.user_agent, "curl/7.0");
    assert_eq!(event.payload.ip_address, "10.0.0.1");
    assert_eq!(event.payload.data.order_id, order_id);
}

#[test]
fn base_struct_with_nested_roundtrip() {
    let event = BaseEventV1 {
        event_type: PlaceOrderDataV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        tenant_id: Uuid::nil(),
        sequence_id: 99,
        payload: AuditPayloadV1 {
            user_agent: "test-agent".to_string(),
            user_id: Uuid::nil(),
            ip_address: "192.168.1.1".to_string(),
            data: PlaceOrderDataV1 {
                order_id: Uuid::nil(),
                product_id: Uuid::nil(),
            },
        },
    };

    let json_str = serde_json::to_string(&event).unwrap();
    let deserialized: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
        serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.sequence_id, event.sequence_id);
    assert_eq!(deserialized.payload.user_agent, event.payload.user_agent);
    assert_eq!(
        deserialized.payload.data.order_id,
        event.payload.data.order_id
    );
}

// =============================================================================
// Tests: serde rename handling
// =============================================================================

#[test]
fn serde_rename_respected() {
    let event = BaseEventV1 {
        event_type: GtsSchemaId::new("gts.x.core.events.type.v1~"),
        id: Uuid::nil(),
        tenant_id: Uuid::nil(),
        sequence_id: 1,
        payload: (),
    };

    let json = serde_json::to_value(&event).unwrap();
    // The field is named "event_type" in Rust but "type" in JSON (via serde rename)
    assert!(json.get("type").is_some(), "should have 'type' key");
    assert!(
        json.get("event_type").is_none(),
        "should NOT have 'event_type' key"
    );
}

#[test]
fn serde_rename_in_deserialize() {
    let json = serde_json::json!({
        "type": "gts.x.core.events.type.v1~",
        "id": Uuid::nil().to_string(),
        "tenant_id": Uuid::nil().to_string(),
        "sequence_id": 1,
        "payload": null
    });

    let event: BaseEventV1<()> = serde_json::from_value(json).unwrap();
    assert_eq!(event.event_type.as_ref(), "gts.x.core.events.type.v1~");
}

// =============================================================================
// Tests: Instance methods
// =============================================================================

#[test]
fn instance_json_methods() {
    let topic = EventTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.test.v1"),
        name: "test".to_string(),
        description: None,
        retention: "P7D".to_string(),
    };

    let val = topic.gts_instance_json();
    assert_eq!(val["name"], "test");

    let s = topic.gts_instance_json_as_string();
    assert!(s.contains("test"));

    let pretty = topic.gts_instance_json_as_string_pretty();
    assert!(pretty.contains('\n'));
}

// =============================================================================
// Tests: Issue #72 — no GTS identity field roundtrip
// =============================================================================

#[test]
fn data_entity_with_type_field_roundtrip() {
    let order = OrderV1_0 {
        gts_type: OrderV1_0::gts_schema_id().clone(),
        id: Uuid::nil(),
        product_id: Uuid::nil(),
        quantity: 3,
        total: 29.99,
    };

    let json_str = serde_json::to_string(&order).unwrap();
    let deserialized: OrderV1_0 = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.quantity, 3);
    assert!((deserialized.total - 29.99).abs() < f64::EPSILON);
    assert_eq!(
        deserialized.gts_type.as_ref(),
        "gts.x.commerce.orders.order.v1.0~"
    );
}

#[test]
fn constructor_instance_roundtrips() {
    // An instance built via the generated `new(...)` serializes, deserializes,
    // and round-trips identically to a struct-literal-built equivalent.
    let via_ctor = OrderV1_0::new(Uuid::nil(), Uuid::nil(), 5, 12.50);
    let via_literal = OrderV1_0 {
        gts_type: OrderV1_0::gts_schema_id().clone(),
        id: Uuid::nil(),
        product_id: Uuid::nil(),
        quantity: 5,
        total: 12.50,
    };

    let ctor_json = serde_json::to_value(&via_ctor).unwrap();
    let literal_json = serde_json::to_value(&via_literal).unwrap();
    assert_eq!(
        ctor_json, literal_json,
        "constructor-built and literal-built instances must serialize identically"
    );

    // Round-trip the constructor-built instance.
    let json_str = serde_json::to_string(&via_ctor).unwrap();
    let back: OrderV1_0 = serde_json::from_str(&json_str).unwrap();
    assert_eq!(back.gts_type.as_ref(), via_ctor.gts_type.as_ref());
    assert_eq!(back.quantity, 5);
    assert!((back.total - 12.50).abs() < f64::EPSILON);
}
