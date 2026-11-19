#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Fields of a derived struct (`base = Parent`) read the way serde derive reads
//! the fields of a base struct: an absent `Option` is `None`, an absent field
//! with `#[serde(default)]` or `#[serde(default = "path")]` takes that default,
//! and a renamed field is read under its wire name. The schema leaves the same
//! fields out of `required`, so an instance the schema accepts deserializes.

use gts::gts::GtsInstanceId;
use gts::{GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
use serde_json::{Value, json};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.derived_opt.record.v1~"),
    description = "Base record",
    properties = "id,data"
)]
#[derive(Debug)]
pub struct RecordV1<P> {
    pub id: GtsInstanceId,
    pub data: P,
}

fn default_level() -> u32 {
    3
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.derived_opt.record.v1~x.test.derived_opt.profile.v1~"),
    description = "Profile with optional and renamed fields",
    properties = "name,note,tags,level,wire_name,wire_note"
)]
#[derive(Debug, PartialEq)]
pub struct ProfileV1 {
    pub name: String,
    pub note: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_level")]
    pub level: u32,
    #[serde(rename = "wireName")]
    pub wire_name: String,
    #[serde(rename = "wireNote")]
    pub wire_note: Option<String>,
}

fn record_id() -> GtsInstanceId {
    GtsInstanceId::new(
        ProfileV1::gts_type_id().as_ref(),
        "x.test.derived_opt.alice.v1",
    )
}

fn record(data: Value) -> Value {
    let mut record = json!({ "id": record_id() });
    record["data"] = data;
    record
}

fn read(data: Value) -> Result<ProfileV1, serde_json::Error> {
    serde_json::from_value::<RecordV1<ProfileV1>>(record(data)).map(|record| record.data)
}

fn resolved_derived_schema() -> Value {
    let base = RecordV1::<()>::gts_schema_with_refs();
    let derived = ProfileV1::gts_schema_with_refs();
    let mut store = GtsStore::new();
    store
        .register_schema(RecordV1::<()>::gts_type_id().as_ref(), &base)
        .unwrap();
    store
        .register_schema(ProfileV1::gts_type_id().as_ref(), &derived)
        .unwrap();
    store.resolve_schema_refs(&derived).unwrap()
}

#[test]
fn absent_optional_fields_take_their_defaults() {
    let profile = read(json!({ "name": "alice", "wireName": "a" })).unwrap();
    assert_eq!(
        profile,
        ProfileV1 {
            name: "alice".to_owned(),
            note: None,
            tags: Vec::new(),
            level: 3,
            wire_name: "a".to_owned(),
            wire_note: None,
        }
    );
}

#[test]
fn null_reads_as_none() {
    let profile = read(json!({
        "name": "alice",
        "note": null,
        "wireName": "a",
        "wireNote": null,
    }))
    .unwrap();
    assert_eq!(profile.note, None);
    assert_eq!(profile.wire_note, None);
}

#[test]
fn present_fields_are_read() {
    let profile = read(json!({
        "name": "alice",
        "note": "hi",
        "tags": ["x"],
        "level": 7,
        "wireName": "a",
        "wireNote": "b",
    }))
    .unwrap();
    assert_eq!(profile.note.as_deref(), Some("hi"));
    assert_eq!(profile.tags, ["x"]);
    assert_eq!(profile.level, 7);
    assert_eq!(profile.wire_name, "a");
    assert_eq!(profile.wire_note.as_deref(), Some("b"));
}

#[test]
fn absent_required_fields_are_missing() {
    let error = read(json!({ "wireName": "a" })).unwrap_err();
    assert!(
        error.to_string().contains("missing field `name`"),
        "{error}"
    );
    let error = read(json!({ "name": "alice" })).unwrap_err();
    assert!(
        error.to_string().contains("missing field `wireName`"),
        "{error}"
    );
}

#[test]
fn renamed_fields_also_accept_the_rust_name() {
    let profile = read(json!({
        "name": "alice",
        "wire_name": "a",
        "wire_note": "b",
    }))
    .unwrap();
    assert_eq!(profile.wire_name, "a");
    assert_eq!(profile.wire_note.as_deref(), Some("b"));
}

#[test]
fn serialized_records_read_back() {
    let record = RecordV1 {
        id: record_id(),
        data: ProfileV1 {
            name: "alice".to_owned(),
            note: Some("hi".to_owned()),
            tags: vec!["x".to_owned()],
            level: 7,
            wire_name: "a".to_owned(),
            wire_note: None,
        },
    };
    let json = serde_json::to_value(&record).unwrap();
    assert_eq!(json["data"]["wireName"], "a", "{json}");
    let back: RecordV1<ProfileV1> = serde_json::from_value(json).unwrap();
    assert_eq!(back.data, record.data);
}

#[test]
fn schema_and_deserializer_agree_on_optional_fields() {
    let schema = ProfileV1::gts_schema_with_refs();
    let data = &schema["allOf"][1]["properties"]["data"];
    let mut required: Vec<&str> = data["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["name", "wireName"], "{schema:#}");

    let minimal = record(json!({ "name": "alice", "wireName": "a" }));
    let validator = jsonschema::validator_for(&resolved_derived_schema()).unwrap();
    assert!(validator.is_valid(&minimal), "schema rejects {minimal}");
    serde_json::from_value::<RecordV1<ProfileV1>>(minimal).unwrap();
}
