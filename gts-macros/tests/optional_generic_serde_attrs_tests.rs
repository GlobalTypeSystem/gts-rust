#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `Option<P>` generic fields that already carry the serde options the macro
//! would add (`default`, `skip_serializing_if`). The macro adds only the
//! missing ones, so these structs compile, and they behave like slots without
//! the annotations: absent for `None`, `None` for an absent member or `null`.

use gts::GtsSchema;
use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;
use serde_json::{Value, json};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.annotated.event.v1~"),
    description = "Base event whose optional slot is already annotated",
    properties = "event_type,data"
)]
#[derive(Debug)]
pub struct AnnotatedEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsTypeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<P>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AnnotatedEventV1,
    type_id = gts_id!("x.core.annotated.event.v1~x.core.annotated.record.v1~"),
    description = "Mid-chain record whose optional slot is already annotated",
    properties = "actor,details"
)]
#[derive(Debug)]
pub struct AnnotatedRecordV1<D> {
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<D>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AnnotatedRecordV1,
    type_id = gts_id!(
        "x.core.annotated.event.v1~x.core.annotated.record.v1~x.core.annotated.login.v1~"
    ),
    description = "Login record",
    properties = "ip"
)]
#[derive(Debug)]
pub struct AnnotatedLoginV1 {
    pub ip: String,
}

type LoginEvent = AnnotatedEventV1<AnnotatedRecordV1<AnnotatedLoginV1>>;

fn login_event(details: Option<AnnotatedLoginV1>) -> LoginEvent {
    AnnotatedEventV1 {
        event_type: AnnotatedLoginV1::gts_type_id().clone(),
        data: Some(AnnotatedRecordV1 {
            actor: "alice".to_owned(),
            details,
        }),
    }
}

#[test]
fn annotated_slots_render_optional_and_nullable() {
    const {
        assert!(<AnnotatedEventV1<()> as GtsSchema>::GENERIC_FIELD_OPTIONAL);
        assert!(<AnnotatedRecordV1<()> as GtsSchema>::GENERIC_FIELD_OPTIONAL);
    }

    let base = AnnotatedEventV1::<()>::gts_schema_with_refs();
    assert_eq!(
        base["properties"]["data"],
        json!({ "type": ["object", "null"] }),
        "{base:#}"
    );
    assert_eq!(base["required"], json!(["type"]), "{base:#}");

    let middle = AnnotatedRecordV1::<()>::gts_schema_with_refs();
    let data = &middle["allOf"][1]["properties"]["data"];
    assert_eq!(
        data["properties"]["details"],
        json!({ "type": ["object", "null"] }),
        "{middle:#}"
    );
    assert_eq!(data["required"], json!(["actor"]), "{middle:#}");
}

#[test]
fn annotated_slots_round_trip_present_null_and_absent() {
    let json = serde_json::to_value(login_event(Some(AnnotatedLoginV1 {
        ip: "10.0.0.1".to_owned(),
    })))
    .unwrap();
    assert_eq!(json["data"]["details"]["ip"], "10.0.0.1");
    let back: LoginEvent = serde_json::from_value(json).unwrap();
    assert_eq!(back.data.unwrap().details.unwrap().ip, "10.0.0.1");

    let json = serde_json::to_value(login_event(None)).unwrap();
    assert!(json["data"].get("details").is_none(), "{json}");
    let back: LoginEvent = serde_json::from_value(json).unwrap();
    assert!(back.data.unwrap().details.is_none());

    let empty = AnnotatedEventV1::<AnnotatedRecordV1<AnnotatedLoginV1>> {
        event_type: AnnotatedEventV1::<()>::gts_type_id().clone(),
        data: None,
    };
    let json = serde_json::to_value(empty).unwrap();
    assert!(json.get("data").is_none(), "{json}");

    for data in [None, Some(Value::Null)] {
        let mut event = json!({ "type": AnnotatedEventV1::<()>::gts_type_id().as_ref() });
        if let Some(data) = data {
            event["data"] = data;
        }
        let back: LoginEvent = serde_json::from_value(event).unwrap();
        assert!(back.data.is_none());
    }
}
