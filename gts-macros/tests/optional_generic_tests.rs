#![allow(clippy::unwrap_used, clippy::expect_used)]

//! An `Option<P>` generic field on a base struct: the extension slot is
//! optional and nullable in the base schema, so a body-less instance is an
//! instance of the base type. A derived type that fills the slot with a payload
//! requires it; one that declares no payload (`properties = ""`) keeps it
//! optional. Instances round-trip with the payload present, `null`, or absent.

use gts::gts::GtsTypeId;
use gts::{GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
use serde_json::{Value, json};
use uuid::Uuid;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.notify.event.v1~"),
    description = "Base event whose payload slot may be absent",
    properties = "event_type,id,data"
)]
#[derive(Debug)]
pub struct NotifyEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsTypeId,
    pub id: Uuid,
    pub data: Option<P>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NotifyEventV1,
    type_id = gts_id!("x.core.notify.event.v1~x.core.notify.order_placed.v1~"),
    description = "Order placed",
    properties = "order_id"
)]
#[derive(Debug)]
pub struct OrderPlacedV1 {
    pub order_id: Uuid,
}

fn base_schema() -> Value {
    NotifyEventV1::<()>::gts_schema_with_refs()
}

fn derived_schema() -> Value {
    OrderPlacedV1::gts_schema_with_refs()
}

/// The derived schema with its `$ref` to the base inlined, ready for a plain
/// JSON Schema validator.
fn resolved_derived_schema() -> Value {
    let mut store = GtsStore::new();
    store
        .register_schema(NotifyEventV1::<()>::gts_type_id().as_ref(), &base_schema())
        .unwrap();
    store
        .register_schema(OrderPlacedV1::gts_type_id().as_ref(), &derived_schema())
        .unwrap();
    store.resolve_schema_refs(&derived_schema()).unwrap()
}

fn instance(data: Option<Value>) -> Value {
    let mut event = json!({
        "type": OrderPlacedV1::gts_type_id().as_ref(),
        "id": Uuid::nil(),
    });
    if let Some(data) = data {
        event["data"] = data;
    }
    event
}

#[test]
fn optional_slot_is_the_generic_field() {
    assert_eq!(
        <NotifyEventV1<()> as GtsSchema>::GENERIC_FIELD,
        Some("data")
    );
}

#[test]
fn base_schema_admits_an_absent_or_null_payload() {
    let schema = base_schema();
    assert_eq!(
        schema["properties"]["data"],
        json!({ "type": ["object", "null"] })
    );
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"id") && required.contains(&"type"));
    assert!(!required.contains(&"data"), "required: {required:?}");
}

#[test]
fn derived_schema_requires_and_narrows_the_slot() {
    let schema = derived_schema();
    let overlay = &schema["allOf"][1];
    assert_eq!(overlay["required"], json!(["data"]), "{schema:#}");
    assert_eq!(overlay["properties"]["data"]["type"], "object");
    assert!(
        overlay["properties"]["data"]["properties"]["order_id"].is_object(),
        "{schema:#}"
    );
}

#[test]
fn registry_accepts_the_derived_type() {
    let mut ops = gts::GtsOps::new(None, None, 0);
    for schema in [base_schema(), derived_schema()] {
        let result = ops
            .add_schemas(std::slice::from_ref(&schema))
            .results
            .remove(0);
        assert!(result.ok, "registration failed: {}", result.error);
    }
}

#[test]
fn derived_schema_validates_payload_shapes() {
    let validator = jsonschema::validator_for(&resolved_derived_schema()).unwrap();

    assert!(validator.is_valid(&instance(Some(json!({ "order_id": Uuid::nil() })))));
    assert!(
        !validator.is_valid(&instance(None)),
        "the derived type requires its payload"
    );
    assert!(
        !validator.is_valid(&instance(Some(Value::Null))),
        "the derived type narrows the slot to an object"
    );
    assert!(!validator.is_valid(&instance(Some(json!({ "order_id": 5 })))));
}

#[test]
fn body_less_instances_belong_to_the_base_type() {
    let body_less = NotifyEventV1::<()> {
        event_type: NotifyEventV1::<()>::gts_type_id().clone(),
        id: Uuid::nil(),
        data: None,
    };
    let json = serde_json::to_value(&body_less).unwrap();
    let base = jsonschema::validator_for(&base_schema()).unwrap();
    assert!(base.is_valid(&json), "base schema rejects {json}");

    let payload_less_order = NotifyEventV1::<OrderPlacedV1> {
        event_type: OrderPlacedV1::gts_type_id().clone(),
        id: Uuid::nil(),
        data: None,
    };
    let json = serde_json::to_value(&payload_less_order).unwrap();
    let derived = jsonschema::validator_for(&resolved_derived_schema()).unwrap();
    assert!(
        !derived.is_valid(&json),
        "an order-placed event without its payload is not an order-placed event: {json}"
    );
}

#[test]
fn base_schema_validates_body_less_instances() {
    let validator = jsonschema::validator_for(&base_schema()).unwrap();
    assert!(validator.is_valid(&instance(None)));
    assert!(validator.is_valid(&instance(Some(Value::Null))));
}

#[test]
fn serde_round_trips_present_null_and_absent_payloads() {
    let order_id = Uuid::new_v4();
    let event = NotifyEventV1 {
        event_type: OrderPlacedV1::gts_type_id().clone(),
        id: Uuid::nil(),
        data: Some(OrderPlacedV1 { order_id }),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["data"]["order_id"], json!(order_id));
    let back: NotifyEventV1<OrderPlacedV1> = serde_json::from_value(json).unwrap();
    assert_eq!(back.data.unwrap().order_id, order_id);

    let body_less = NotifyEventV1::<OrderPlacedV1> {
        event_type: OrderPlacedV1::gts_type_id().clone(),
        id: Uuid::nil(),
        data: None,
    };
    let body_less_json = serde_json::to_value(&body_less).unwrap();
    assert!(
        body_less_json.get("data").is_none(),
        "`None` is omitted, not serialized as null: {body_less_json}"
    );

    let from_null: NotifyEventV1<OrderPlacedV1> =
        serde_json::from_value(instance(Some(Value::Null))).unwrap();
    assert!(from_null.data.is_none());
    let from_absent: NotifyEventV1<OrderPlacedV1> = serde_json::from_value(instance(None)).unwrap();
    assert!(from_absent.data.is_none());
}

/* ============================================================
A body-less derived type: fills the slot with no payload
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NotifyEventV1,
    type_id = gts_id!("x.core.notify.event.v1~x.core.notify.order_cancelled.v1~"),
    description = "Order cancelled; carries no payload",
    properties = ""
)]
#[derive(Debug)]
pub struct OrderCancelledV1;

fn resolved_cancelled_schema() -> Value {
    let cancelled = OrderCancelledV1::gts_schema_with_refs();
    let mut store = GtsStore::new();
    store
        .register_schema(NotifyEventV1::<()>::gts_type_id().as_ref(), &base_schema())
        .unwrap();
    store
        .register_schema(OrderCancelledV1::gts_type_id().as_ref(), &cancelled)
        .unwrap();
    store.resolve_schema_refs(&cancelled).unwrap()
}

fn cancelled_instance(data: Option<Value>) -> Value {
    let mut event = json!({
        "type": OrderCancelledV1::gts_type_id().as_ref(),
        "id": Uuid::nil(),
    });
    if let Some(data) = data {
        event["data"] = data;
    }
    event
}

#[test]
fn body_less_type_keeps_the_slot_optional_and_nullable() {
    let schema = OrderCancelledV1::gts_schema_with_refs();
    let overlay = &schema["allOf"][1];
    assert!(overlay.get("required").is_none(), "{schema:#}");
    assert_eq!(
        overlay["properties"]["data"]["type"],
        json!(["object", "null"]),
        "{schema:#}"
    );
}

#[test]
fn registry_accepts_the_body_less_type() {
    let mut ops = gts::GtsOps::new(None, None, 0);
    for schema in [base_schema(), OrderCancelledV1::gts_schema_with_refs()] {
        let result = ops
            .add_schemas(std::slice::from_ref(&schema))
            .results
            .remove(0);
        assert!(result.ok, "registration failed: {}", result.error);
    }
}

#[test]
fn body_less_type_admits_absent_null_or_empty_data_only() {
    let validator = jsonschema::validator_for(&resolved_cancelled_schema()).unwrap();
    assert!(validator.is_valid(&cancelled_instance(None)));
    assert!(validator.is_valid(&cancelled_instance(Some(Value::Null))));
    assert!(validator.is_valid(&cancelled_instance(Some(json!({})))));
    assert!(
        !validator.is_valid(&cancelled_instance(Some(
            json!({ "order_id": Uuid::nil() })
        ))),
        "a body-less type declares no payload fields"
    );
}

#[test]
fn body_less_type_serializes_to_valid_instances() {
    let validator = jsonschema::validator_for(&resolved_cancelled_schema()).unwrap();
    for data in [None, Some(OrderCancelledV1)] {
        let event = NotifyEventV1::<OrderCancelledV1> {
            event_type: OrderCancelledV1::gts_type_id().clone(),
            id: Uuid::nil(),
            data,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert!(validator.is_valid(&json), "body-less schema rejects {json}");
        let back: NotifyEventV1<OrderCancelledV1> = serde_json::from_value(json).unwrap();
        assert_eq!(back.data.is_some(), event.data.is_some());
    }
}
