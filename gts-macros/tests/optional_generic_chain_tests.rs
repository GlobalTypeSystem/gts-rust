#![allow(clippy::unwrap_used, clippy::expect_used)]

//! An `Option<P>` generic field on a derived struct in the middle of a chain:
//! base -> middle (optional slot) -> leaf. The middle's slot is optional and
//! nullable in its own schema, so a body-less record is an instance of the
//! middle type. A leaf with a payload fills the slot and requires it; a leaf
//! that declares no payload keeps it optional. Instances round-trip with the
//! middle's payload present, `null`, or absent.

use gts::gts::GtsTypeId;
use gts::{GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
use serde_json::{Value, json};
use uuid::Uuid;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.audit.event.v1~"),
    description = "Base audit event",
    properties = "event_type,id,payload"
)]
#[derive(Debug)]
pub struct AuditEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsTypeId,
    pub id: Uuid,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AuditEventV1,
    type_id = gts_id!("x.core.audit.event.v1~x.core.audit.record.v1~"),
    description = "Audit record whose details may be absent",
    properties = "actor,details"
)]
#[derive(Debug)]
pub struct AuditRecordV1<D> {
    pub actor: String,
    pub details: Option<D>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AuditRecordV1,
    type_id = gts_id!("x.core.audit.event.v1~x.core.audit.record.v1~x.core.audit.login.v1~"),
    description = "Login audit record",
    properties = "ip"
)]
#[derive(Debug)]
pub struct LoginDetailsV1 {
    pub ip: String,
}

type LoginEvent = AuditEventV1<AuditRecordV1<LoginDetailsV1>>;

fn base_schema() -> Value {
    AuditEventV1::<()>::gts_schema_with_refs()
}

fn middle_schema() -> Value {
    AuditRecordV1::<()>::gts_schema_with_refs()
}

fn leaf_schema() -> Value {
    LoginDetailsV1::gts_schema_with_refs()
}

/// `schema` with every `$ref` along the chain inlined.
fn resolved(schema: &Value) -> Value {
    let mut store = GtsStore::new();
    store
        .register_schema(AuditEventV1::<()>::gts_type_id().as_ref(), &base_schema())
        .unwrap();
    store
        .register_schema(
            AuditRecordV1::<()>::gts_type_id().as_ref(),
            &middle_schema(),
        )
        .unwrap();
    store
        .register_schema(LoginDetailsV1::gts_type_id().as_ref(), &leaf_schema())
        .unwrap();
    store.resolve_schema_refs(schema).unwrap()
}

fn login_event(details: Option<LoginDetailsV1>) -> LoginEvent {
    AuditEventV1 {
        event_type: LoginDetailsV1::gts_type_id().clone(),
        id: Uuid::nil(),
        payload: AuditRecordV1 {
            actor: "alice".to_owned(),
            details,
        },
    }
}

fn instance(details: Option<Value>) -> Value {
    let mut event = json!({
        "type": LoginDetailsV1::gts_type_id().as_ref(),
        "id": Uuid::nil(),
        "payload": { "actor": "alice" },
    });
    if let Some(details) = details {
        event["payload"]["details"] = details;
    }
    event
}

#[test]
fn optional_slot_is_the_middle_generic_field() {
    assert_eq!(
        <AuditRecordV1<()> as GtsSchema>::GENERIC_FIELD,
        Some("details")
    );
}

#[test]
fn middle_schema_admits_absent_or_null_details() {
    let schema = middle_schema();
    let payload = &schema["allOf"][1]["properties"]["payload"];
    assert_eq!(
        payload["properties"]["details"],
        json!({ "type": ["object", "null"] }),
        "{schema:#}"
    );
    let required: Vec<&str> = payload["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    assert!(required.contains(&"actor"), "{schema:#}");
    assert!(!required.contains(&"details"), "{schema:#}");
}

#[test]
fn leaf_schema_requires_and_narrows_the_middle_slot() {
    let schema = leaf_schema();
    let payload = &schema["allOf"][1]["properties"]["payload"];
    assert_eq!(payload["required"], json!(["details"]), "{schema:#}");
    let details = &payload["properties"]["details"];
    assert_eq!(details["type"], "object", "{schema:#}");
    assert!(details["properties"]["ip"].is_object(), "{schema:#}");
}

#[test]
fn registry_accepts_the_chain() {
    let mut ops = gts::GtsOps::new(None, None, 0);
    for schema in [base_schema(), middle_schema(), leaf_schema()] {
        let result = ops
            .add_schemas(std::slice::from_ref(&schema))
            .results
            .remove(0);
        assert!(result.ok, "registration failed: {}", result.error);
    }
}

#[test]
fn serde_round_trips_present_null_and_absent_details() {
    let json = serde_json::to_value(login_event(Some(LoginDetailsV1 {
        ip: "10.0.0.1".to_owned(),
    })))
    .unwrap();
    assert_eq!(json["payload"]["details"]["ip"], "10.0.0.1");
    let back: LoginEvent = serde_json::from_value(json).unwrap();
    assert_eq!(back.payload.details.unwrap().ip, "10.0.0.1");

    let json = serde_json::to_value(login_event(None)).unwrap();
    assert!(
        json["payload"].get("details").is_none(),
        "`None` is omitted, not serialized as null: {json}"
    );
    let back: LoginEvent = serde_json::from_value(json).unwrap();
    assert!(back.payload.details.is_none());

    let from_null: LoginEvent = serde_json::from_value(instance(Some(Value::Null))).unwrap();
    assert!(from_null.payload.details.is_none());
    let from_absent: LoginEvent = serde_json::from_value(instance(None)).unwrap();
    assert!(from_absent.payload.details.is_none());
}

#[test]
fn leaf_type_requires_its_details() {
    let validator = jsonschema::validator_for(&resolved(&leaf_schema())).unwrap();
    let json = serde_json::to_value(login_event(Some(LoginDetailsV1 {
        ip: "10.0.0.1".to_owned(),
    })))
    .unwrap();
    assert!(validator.is_valid(&json), "leaf schema rejects {json}");

    let json = serde_json::to_value(login_event(None)).unwrap();
    assert!(
        !validator.is_valid(&json),
        "a login record without its details is not a login record: {json}"
    );
    assert!(
        !validator.is_valid(&instance(Some(Value::Null))),
        "the leaf narrows the middle slot to an object"
    );
    assert!(!validator.is_valid(&instance(Some(json!({ "ip": 5 })))));
}

#[test]
fn body_less_records_belong_to_the_middle_type() {
    let record = AuditEventV1 {
        event_type: AuditRecordV1::<()>::gts_type_id().clone(),
        id: Uuid::nil(),
        payload: AuditRecordV1::<()> {
            actor: "alice".to_owned(),
            details: None,
        },
    };
    let json = serde_json::to_value(record).unwrap();
    let validator = jsonschema::validator_for(&resolved(&middle_schema())).unwrap();
    assert!(validator.is_valid(&json), "middle schema rejects {json}");
    assert!(validator.is_valid(&instance(Some(Value::Null))));
}

/* ============================================================
A body-less leaf: fills the middle slot with no payload
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AuditRecordV1,
    type_id = gts_id!("x.core.audit.event.v1~x.core.audit.record.v1~x.core.audit.logout.v1~"),
    description = "Logout audit record; carries no details",
    properties = ""
)]
#[derive(Debug)]
pub struct LogoutV1;

fn logout_instance(details: Option<Value>) -> Value {
    let mut event = json!({
        "type": LogoutV1::gts_type_id().as_ref(),
        "id": Uuid::nil(),
        "payload": { "actor": "alice" },
    });
    if let Some(details) = details {
        event["payload"]["details"] = details;
    }
    event
}

fn resolved_logout_schema() -> Value {
    let logout = LogoutV1::gts_schema_with_refs();
    let mut store = GtsStore::new();
    store
        .register_schema(AuditEventV1::<()>::gts_type_id().as_ref(), &base_schema())
        .unwrap();
    store
        .register_schema(
            AuditRecordV1::<()>::gts_type_id().as_ref(),
            &middle_schema(),
        )
        .unwrap();
    store
        .register_schema(LogoutV1::gts_type_id().as_ref(), &logout)
        .unwrap();
    store.resolve_schema_refs(&logout).unwrap()
}

#[test]
fn body_less_leaf_keeps_the_middle_slot_optional_and_nullable() {
    let schema = LogoutV1::gts_schema_with_refs();
    let payload = &schema["allOf"][1]["properties"]["payload"];
    assert!(payload.get("required").is_none(), "{schema:#}");
    assert_eq!(
        payload["properties"]["details"]["type"],
        json!(["object", "null"]),
        "{schema:#}"
    );
}

#[test]
fn registry_accepts_the_body_less_leaf() {
    let mut ops = gts::GtsOps::new(None, None, 0);
    for schema in [
        base_schema(),
        middle_schema(),
        LogoutV1::gts_schema_with_refs(),
    ] {
        let result = ops
            .add_schemas(std::slice::from_ref(&schema))
            .results
            .remove(0);
        assert!(result.ok, "registration failed: {}", result.error);
    }
}

#[test]
fn body_less_leaf_admits_absent_null_or_empty_details_only() {
    let validator = jsonschema::validator_for(&resolved_logout_schema()).unwrap();
    assert!(validator.is_valid(&logout_instance(None)));
    assert!(validator.is_valid(&logout_instance(Some(Value::Null))));
    assert!(validator.is_valid(&logout_instance(Some(json!({})))));
    assert!(
        !validator.is_valid(&logout_instance(Some(json!({ "ip": "10.0.0.1" })))),
        "a body-less leaf declares no detail fields"
    );
}

#[test]
fn body_less_leaf_serializes_to_valid_instances() {
    let validator = jsonschema::validator_for(&resolved_logout_schema()).unwrap();
    for details in [None, Some(LogoutV1)] {
        let event = AuditEventV1 {
            event_type: LogoutV1::gts_type_id().clone(),
            id: Uuid::nil(),
            payload: AuditRecordV1 {
                actor: "alice".to_owned(),
                details,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert!(
            validator.is_valid(&json),
            "body-less leaf schema rejects {json}"
        );
        let back: AuditEventV1<AuditRecordV1<LogoutV1>> = serde_json::from_value(json).unwrap();
        assert_eq!(
            back.payload.details.is_some(),
            event.payload.details.is_some()
        );
    }
}
