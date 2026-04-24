#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::bool_assert_comparison,
    dead_code
)]

use gts::{GtsSchema, GtsSchemaId};
use gts_macros::GtsSchema;
use schemars::JsonSchema;
use uuid::Uuid;

// =============================================================================
// Test hierarchy: 3-level chain
//
// All structs derive JsonSchema (required for schemars::schema_for! inside GtsSchema).
// Generic params use just `GtsSchema` — schemars adds `JsonSchema` bound automatically.
// =============================================================================

/// Level 1: Base event (root, generic)
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

/// Unit struct child (no fields)
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.empty.event.v1~",
    description = "Empty event payload",
    extends = BaseEventV1
)]
pub struct EmptyPayloadV1;

// =============================================================================
// Tests: Schema structure for 2-level and 3-level inheritance
// =============================================================================

#[test]
fn two_level_schema() {
    let schema = AuditPayloadV1::<()>::gts_schema_with_refs();

    assert_eq!(
        schema["$id"],
        "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~"
    );

    let all_of = schema.get("allOf").expect("should have allOf");
    assert!(all_of.is_array());
    assert_eq!(all_of[0]["$ref"], "gts://gts.x.core.events.type.v1~");

    let props = &all_of[1]["properties"];
    assert!(props.is_object(), "allOf[1] should have properties");
}

#[test]
fn three_level_schema() {
    let schema = PlaceOrderDataV1::gts_schema_with_refs();

    assert_eq!(
        schema["$id"],
        "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
    );

    let all_of = schema.get("allOf").expect("should have allOf");
    assert!(all_of.is_array());
    assert_eq!(
        all_of[0]["$ref"],
        "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~"
    );
}

// =============================================================================
// Tests: 3-level serialization and deserialization
// =============================================================================

#[test]
fn three_level_serialize() {
    let event = BaseEventV1 {
        event_type: PlaceOrderDataV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        tenant_id: Uuid::nil(),
        sequence_id: 42,
        payload: AuditPayloadV1 {
            user_agent: "Mozilla/5.0".to_string(),
            user_id: Uuid::nil(),
            ip_address: "127.0.0.1".to_string(),
            data: PlaceOrderDataV1 {
                order_id: Uuid::nil(),
                product_id: Uuid::nil(),
            },
        },
    };

    let json = serde_json::to_value(&event).unwrap();

    assert_eq!(json["sequence_id"], 42);
    assert!(json.get("type").is_some());
    assert_eq!(json["payload"]["user_agent"], "Mozilla/5.0");
    assert_eq!(json["payload"]["ip_address"], "127.0.0.1");
    assert!(json["payload"]["data"].get("order_id").is_some());
}

#[test]
fn three_level_deserialize() {
    let json = serde_json::json!({
        "type": "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
        "id": Uuid::nil().to_string(),
        "tenant_id": Uuid::nil().to_string(),
        "sequence_id": 99,
        "payload": {
            "user_agent": "curl/8.0",
            "user_id": Uuid::nil().to_string(),
            "ip_address": "10.0.0.1",
            "data": {
                "order_id": Uuid::nil().to_string(),
                "product_id": Uuid::nil().to_string()
            }
        }
    });

    let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
        serde_json::from_value(json).unwrap();

    assert_eq!(event.sequence_id, 99);
    assert_eq!(event.payload.user_agent, "curl/8.0");
    assert_eq!(event.payload.data.order_id, Uuid::nil());
}

#[test]
fn three_level_roundtrip() {
    let event = BaseEventV1 {
        event_type: PlaceOrderDataV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        tenant_id: Uuid::nil(),
        sequence_id: 7,
        payload: AuditPayloadV1 {
            user_agent: "roundtrip-test".to_string(),
            user_id: Uuid::nil(),
            ip_address: "192.168.0.1".to_string(),
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
// Tests: Schema ID, base schema ID, instance ID
// =============================================================================

#[test]
fn child_schema_id() {
    assert_eq!(
        AuditPayloadV1::<()>::gts_schema_id().as_ref(),
        "gts.x.core.events.type.v1~x.core.audit.event.v1~"
    );
    assert_eq!(
        PlaceOrderDataV1::gts_schema_id().as_ref(),
        "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
    );
}

#[test]
fn child_base_schema_id() {
    assert_eq!(
        AuditPayloadV1::<()>::gts_base_schema_id().map(AsRef::as_ref),
        Some("gts.x.core.events.type.v1~")
    );
    assert_eq!(
        PlaceOrderDataV1::gts_base_schema_id().map(AsRef::as_ref),
        Some("gts.x.core.events.type.v1~x.core.audit.event.v1~")
    );
}

#[test]
fn child_instance_id() {
    let instance_id = PlaceOrderDataV1::gts_make_instance_id("vendor.test.v1");
    assert_eq!(
        instance_id.as_ref(),
        "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~vendor.test.v1"
    );
}

// =============================================================================
// Tests: innermost_schema_id and nesting path
// =============================================================================

#[test]
fn innermost_schema_id() {
    let innermost = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>>::innermost_schema_id();
    assert_eq!(
        innermost,
        "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
    );
}

#[test]
fn nesting_path() {
    let path = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>>::collect_nesting_path();
    assert_eq!(path, vec!["payload", "data"]);
}

#[test]
fn nesting_path_two_level() {
    let path = BaseEventV1::<EmptyPayloadV1>::collect_nesting_path();
    assert_eq!(path, vec!["payload"]);
}

// =============================================================================
// Tests: GENERIC_FIELD detection
// =============================================================================

#[test]
fn generic_field_detection() {
    assert_eq!(
        <BaseEventV1<()> as GtsSchema>::GENERIC_FIELD,
        Some("payload")
    );
    assert_eq!(
        <AuditPayloadV1<()> as GtsSchema>::GENERIC_FIELD,
        Some("data")
    );
    assert_eq!(<PlaceOrderDataV1 as GtsSchema>::GENERIC_FIELD, None);
}

// =============================================================================
// Tests: Unit struct child
// =============================================================================

#[test]
fn unit_struct_child() {
    let event = BaseEventV1 {
        event_type: EmptyPayloadV1::gts_schema_id().clone(),
        id: Uuid::nil(),
        tenant_id: Uuid::nil(),
        sequence_id: 1,
        payload: EmptyPayloadV1,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["sequence_id"], 1);
    assert_eq!(json["payload"], serde_json::json!({}));

    let json_str = serde_json::to_string(&event).unwrap();
    let deserialized: BaseEventV1<EmptyPayloadV1> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.sequence_id, 1);
}

// =============================================================================
// Tests: additionalProperties on each level
// =============================================================================

#[test]
fn schema_additional_properties() {
    let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
    assert_eq!(base_schema["additionalProperties"], false);

    let audit_schema = AuditPayloadV1::<()>::gts_schema_with_refs();
    assert_eq!(audit_schema["additionalProperties"], false);

    let leaf_schema = PlaceOrderDataV1::gts_schema_with_refs();
    assert_eq!(leaf_schema["additionalProperties"], false);
}
