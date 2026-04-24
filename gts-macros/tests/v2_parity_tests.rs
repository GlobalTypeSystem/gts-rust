#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::bool_assert_comparison,
    dead_code
)]

//! Phase 5: Parity tests — verify that the new macro produces equivalent behavior
//! to the old `struct_to_gts_schema` macro, plus test new capabilities.

use gts::{GtsInstanceId, GtsSchema, GtsSchemaId};
use gts_macros::{GtsSchema, struct_to_gts_schema};
use schemars::JsonSchema;
use uuid::Uuid;

// =============================================================================
// OLD macro structs (baseline)
// =============================================================================

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Event topic definition",
    properties = "id,name,retention"
)]
#[derive(Debug, Clone)]
pub struct OldTopicV1 {
    pub id: GtsInstanceId,
    pub name: String,
    pub retention: String,
    pub internal: Option<String>, // not in properties list
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "event_type,id,tenant_id,payload"
)]
#[derive(Debug)]
pub struct OldBaseEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OldBaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    properties = "user_agent,data"
)]
#[derive(Debug)]
pub struct OldAuditPayloadV1<D> {
    pub user_agent: String,
    pub data: D,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OldAuditPayloadV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    description = "Order placement event",
    properties = "order_id"
)]
#[derive(Debug)]
pub struct OldPlaceOrderDataV1 {
    pub order_id: Uuid,
}

// =============================================================================
// NEW macro structs (equivalent)
// =============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Event topic definition"
)]
pub struct NewTopicV1 {
    #[gts(instance_id)]
    pub id: GtsInstanceId,
    pub name: String,
    pub retention: String,
    #[gts(skip)]
    pub internal: Option<String>,
}

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type"
)]
pub struct NewBaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub payload: P,
}

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    extends = NewBaseEventV1
)]
pub struct NewAuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub data: D,
}

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    description = "Order placement event",
    extends = NewAuditPayloadV1
)]
pub struct NewPlaceOrderDataV1 {
    pub order_id: Uuid,
}

// =============================================================================
// 5.1 Schema parity tests
// =============================================================================

/// Compare two schemas ignoring known differences (description in new, x-gts-ref on identity fields).
fn schemas_match(old: &serde_json::Value, new: &serde_json::Value, context: &str) {
    // $id must match
    assert_eq!(old["$id"], new["$id"], "{context}: $id mismatch");
    // $schema must match
    assert_eq!(
        old["$schema"], new["$schema"],
        "{context}: $schema mismatch"
    );
    // type must match
    assert_eq!(old["type"], new["type"], "{context}: type mismatch");
    // additionalProperties must match
    assert_eq!(
        old["additionalProperties"], new["additionalProperties"],
        "{context}: additionalProperties mismatch"
    );
    // New schema should have description (improvement)
    assert!(
        new.get("description").is_some(),
        "{context}: new schema should have description"
    );
}

#[test]
fn topic_schema_parity() {
    let old = OldTopicV1::gts_schema_with_refs();
    let new = NewTopicV1::gts_schema_with_refs();

    schemas_match(&old, &new, "TopicV1");

    // Both should have the same structure
    let old_props = old["properties"].as_object().unwrap();
    let new_props = new["properties"].as_object().unwrap();

    // Both should have id, name, retention
    assert!(old_props.contains_key("id"));
    assert!(new_props.contains_key("id"));
    assert!(old_props.contains_key("name"));
    assert!(new_props.contains_key("name"));
    assert!(old_props.contains_key("retention"));
    assert!(new_props.contains_key("retention"));
}

#[test]
fn base_event_schema_parity() {
    let old = OldBaseEventV1::<()>::gts_schema_with_refs();
    let new = NewBaseEventV1::<()>::gts_schema_with_refs();

    schemas_match(&old, &new, "BaseEventV1");

    // Both should have same $id
    assert_eq!(old["$id"], "gts://gts.x.core.events.type.v1~");
    assert_eq!(new["$id"], "gts://gts.x.core.events.type.v1~");
}

#[test]
fn child_event_schema_parity() {
    let old = OldAuditPayloadV1::<()>::gts_schema_with_refs();
    let new = NewAuditPayloadV1::<()>::gts_schema_with_refs();

    schemas_match(&old, &new, "AuditPayloadV1");

    // Both should have allOf with $ref to base
    assert!(old.get("allOf").is_some(), "old should have allOf");
    assert!(new.get("allOf").is_some(), "new should have allOf");
    assert_eq!(old["allOf"][0]["$ref"], new["allOf"][0]["$ref"]);
}

#[test]
fn three_level_schema_parity() {
    let old = OldPlaceOrderDataV1::gts_schema_with_refs();
    let new = NewPlaceOrderDataV1::gts_schema_with_refs();

    schemas_match(&old, &new, "PlaceOrderDataV1");

    assert_eq!(old["allOf"][0]["$ref"], new["allOf"][0]["$ref"]);
}

#[test]
fn instance_json_parity() {
    let old_topic = OldTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.test.v1"),
        name: "test".to_string(),
        retention: "P30D".to_string(),
        internal: None,
    };
    let new_topic = NewTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.test.v1"),
        name: "test".to_string(),
        retention: "P30D".to_string(),
        internal: None,
    };

    let old_json = serde_json::to_value(&old_topic).unwrap();
    let new_json = serde_json::to_value(&new_topic).unwrap();

    assert_eq!(old_json["id"], new_json["id"]);
    assert_eq!(old_json["name"], new_json["name"]);
    assert_eq!(old_json["retention"], new_json["retention"]);
}

#[test]
fn deserialization_parity() {
    let json = serde_json::json!({
        "id": "gts.x.core.events.topic.v1~x.test.v1",
        "name": "test",
        "retention": "P7D"
    });

    let old: OldTopicV1 = serde_json::from_value(json.clone()).unwrap();
    let new: NewTopicV1 = serde_json::from_value(json).unwrap();

    assert_eq!(old.name, new.name);
    assert_eq!(old.retention, new.retention);
}

// =============================================================================
// 5.2 New capability tests
// =============================================================================

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
    pub quantity: u32,
}

#[test]
fn data_entity_with_type_field() {
    let schema = OrderV1_0::gts_schema_with_refs();
    assert_eq!(schema["$id"], "gts://gts.x.commerce.orders.order.v1.0~");
    assert_eq!(schema["type"], "object");
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("type")); // identity field via serde rename
    assert!(props.contains_key("id"));
    assert!(props.contains_key("quantity"));
}

#[test]
fn data_entity_roundtrip() {
    let order = OrderV1_0 {
        gts_type: OrderV1_0::gts_schema_id().clone(),
        id: Uuid::nil(),
        quantity: 5,
    };
    let json_str = serde_json::to_string(&order).unwrap();
    let deserialized: OrderV1_0 = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.quantity, 5);
    assert_eq!(
        deserialized.gts_type.as_ref(),
        "gts.x.commerce.orders.order.v1.0~"
    );
}

#[test]
fn gts_skip_field() {
    // #[gts(skip)] excludes from GTS_SCHEMA_PROPERTIES but not from serde
    let props = NewTopicV1::GTS_SCHEMA_PROPERTIES;
    assert!(
        !props.contains("internal"),
        "gts(skip) should exclude from properties"
    );
    assert!(props.contains("id"));
    assert!(props.contains("name"));
    assert!(props.contains("retention"));

    // But the field is still serializable
    let topic = NewTopicV1 {
        id: GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.test.v1"),
        name: "test".to_string(),
        retention: "P30D".to_string(),
        internal: Some("cached".to_string()),
    };
    let json = serde_json::to_value(&topic).unwrap();
    assert_eq!(json["internal"], "cached");
}

#[test]
fn description_in_runtime_schema() {
    let schema = NewTopicV1::gts_schema_with_refs();
    assert_eq!(schema["description"], "Event topic definition");

    let schema = NewBaseEventV1::<()>::gts_schema_with_refs();
    assert_eq!(schema["description"], "Base event type");

    let schema = NewAuditPayloadV1::<()>::gts_schema_with_refs();
    assert_eq!(schema["description"], "Audit event");
}

#[test]
fn x_gts_ref_self_reference() {
    // #[gts(type_field)] produces "x-gts-ref": "/$id"
    let schema = NewBaseEventV1::<()>::gts_schema_with_refs();
    let type_prop = &schema["properties"]["type"];
    assert_eq!(type_prop["x-gts-ref"], "/$id");

    // #[gts(instance_id)] produces "x-gts-ref": "/$id"
    let schema = NewTopicV1::gts_schema_with_refs();
    let id_prop = &schema["properties"]["id"];
    assert_eq!(id_prop["x-gts-ref"], "/$id");
}

/// Struct with a non-identity `GtsSchemaId` field alongside a `type_field` identity.
/// The `subject_type` field is not annotated; the test verifies it retains the generic
/// `x-gts-ref: "gts.*"` while the identity field gets `/$id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.test.ref.holder.v1~",
    description = "Cross-reference holder"
)]
pub struct CrossRefV1 {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub gts_type: GtsSchemaId,
    pub id: Uuid,
    pub subject_type: GtsSchemaId,
}

#[test]
fn x_gts_ref_cross_reference() {
    // Non-annotated GtsSchemaId retains "x-gts-ref": "gts.*"
    let schema = CrossRefV1::gts_schema_with_refs();
    let subject_prop = &schema["properties"]["subject_type"];
    assert_eq!(subject_prop["x-gts-ref"], "gts.*");
}

#[test]
fn schema_id_parity() {
    assert_eq!(
        OldTopicV1::gts_schema_id().as_ref(),
        NewTopicV1::gts_schema_id().as_ref()
    );
    assert_eq!(
        OldBaseEventV1::<()>::gts_schema_id().as_ref(),
        NewBaseEventV1::<()>::gts_schema_id().as_ref()
    );
}

#[test]
fn base_schema_id_parity() {
    assert_eq!(
        OldTopicV1::gts_base_schema_id().map(AsRef::as_ref),
        NewTopicV1::gts_base_schema_id().map(AsRef::as_ref)
    );
    assert_eq!(
        OldAuditPayloadV1::<()>::gts_base_schema_id().map(AsRef::as_ref),
        NewAuditPayloadV1::<()>::gts_base_schema_id().map(AsRef::as_ref)
    );
}

#[test]
fn instance_id_parity() {
    let old_id = OldTopicV1::gts_make_instance_id("x.test.v1");
    let new_id = NewTopicV1::gts_make_instance_id("x.test.v1");
    assert_eq!(old_id.as_ref(), new_id.as_ref());
}

#[test]
fn generic_field_parity() {
    assert_eq!(
        <OldBaseEventV1<()> as GtsSchema>::GENERIC_FIELD,
        <NewBaseEventV1<()> as GtsSchema>::GENERIC_FIELD
    );
    assert_eq!(
        <OldAuditPayloadV1<()> as GtsSchema>::GENERIC_FIELD,
        <NewAuditPayloadV1<()> as GtsSchema>::GENERIC_FIELD
    );
}

/// Dump schemas for visual inspection. Run with:
///
/// ```sh
/// cargo test -p gts-macros --test v2_parity_tests dump_schemas -- --nocapture
/// ```
#[test]
fn dump_schemas() {
    let structs: Vec<(&str, serde_json::Value)> = vec![
        ("NewTopicV1", NewTopicV1::gts_schema_with_refs()),
        (
            "NewBaseEventV1<()>",
            NewBaseEventV1::<()>::gts_schema_with_refs(),
        ),
        (
            "NewAuditPayloadV1<()>",
            NewAuditPayloadV1::<()>::gts_schema_with_refs(),
        ),
        (
            "NewPlaceOrderDataV1",
            NewPlaceOrderDataV1::gts_schema_with_refs(),
        ),
    ];

    for (name, schema) in &structs {
        println!(
            "\n=== {} ===\n{}",
            name,
            serde_json::to_string_pretty(schema).unwrap()
        );
    }
}
