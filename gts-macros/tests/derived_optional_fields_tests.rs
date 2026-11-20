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

fn default_kind() -> String {
    "plain".to_owned()
}

fn retries() -> u32 {
    5
}

/// Field names and attributes that interact with the generated visitor: a
/// rename followed by another option, a field named like its default function,
/// and a field named like the visitor's map.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.derived_opt.record.v1~x.test.derived_opt.edge.v1~"),
    description = "Fields that interact with the generated visitor",
    properties = "kind_name,retries,map"
)]
#[derive(Debug, PartialEq)]
pub struct EdgeV1 {
    #[serde(rename = "kind", default = "default_kind")]
    pub kind_name: String,
    #[serde(default = "retries")]
    pub retries: u32,
    pub map: Option<String>,
}

/// Fields whose renames swap names (`second` is written as `c`, `first` as
/// `second`'s Rust name `b`), declared before the field they take the name
/// from, and a default path through `Self`.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.derived_opt.record.v1~x.test.derived_opt.swap.v1~"),
    description = "Swapped renames and a Self default",
    properties = "b,a,size"
)]
#[derive(Debug, PartialEq)]
pub struct SwapV1 {
    #[serde(rename = "c")]
    pub b: String,
    #[serde(rename = "b")]
    pub a: String,
    #[serde(default = "Self::default_size")]
    pub size: u32,
}

impl SwapV1 {
    fn default_size() -> u32 {
        9
    }
}

/// A generic derived struct in the middle of a chain, with its own optional
/// field and a `Self` default next to the generic slot.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.derived_opt.record.v1~x.test.derived_opt.wrapper.v1~"),
    description = "Generic middle type",
    properties = "label,depth,inner"
)]
#[derive(Debug)]
pub struct WrapperV1<Q> {
    pub label: Option<String>,
    #[serde(default = "Self::default_depth")]
    pub depth: u32,
    pub inner: Q,
}

impl<Q: gts::GtsSchema> WrapperV1<Q> {
    fn default_depth() -> u32 {
        4
    }
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = WrapperV1,
    type_id = gts_id!(
        "x.test.derived_opt.record.v1~x.test.derived_opt.wrapper.v1~x.test.derived_opt.leaf.v1~"
    ),
    description = "Leaf under the generic middle type",
    properties = "size,comment"
)]
#[derive(Debug)]
pub struct LeafV1 {
    pub size: u32,
    pub comment: Option<String>,
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

fn read_edge(data: Value) -> Result<EdgeV1, serde_json::Error> {
    let id = GtsInstanceId::new(EdgeV1::gts_type_id().as_ref(), "x.test.derived_opt.edge.v1");
    let mut record = json!({ "id": id });
    record["data"] = data;
    serde_json::from_value::<RecordV1<EdgeV1>>(record).map(|record| record.data)
}

#[test]
fn rename_is_read_when_followed_by_other_options() {
    let schema = EdgeV1::gts_schema_with_refs();
    let data = &schema["allOf"][1]["properties"]["data"];
    assert!(data["properties"]["kind"].is_object(), "{schema:#}");

    let edge = read_edge(json!({ "kind": "strict" })).unwrap();
    assert_eq!(edge.kind_name, "strict");
    let edge = read_edge(json!({})).unwrap();
    assert_eq!(edge.kind_name, "plain");

    let id = GtsInstanceId::new(EdgeV1::gts_type_id().as_ref(), "x.test.derived_opt.edge.v1");
    let json = serde_json::to_value(RecordV1 { id, data: edge }).unwrap();
    assert_eq!(json["data"]["kind"], "plain", "{json}");
}

#[test]
fn defaults_and_names_do_not_collide_with_the_visitor() {
    let edge = read_edge(json!({})).unwrap();
    assert_eq!(
        edge,
        EdgeV1 {
            kind_name: "plain".to_owned(),
            retries: 5,
            map: None,
        }
    );
    let edge = read_edge(json!({ "retries": 1, "map": "m" })).unwrap();
    assert_eq!(edge.retries, 1);
    assert_eq!(edge.map.as_deref(), Some("m"));
}

#[test]
fn wire_name_and_alias_together_are_a_duplicate() {
    let error = read(json!({ "name": "alice", "wireName": "a", "wire_name": "b" })).unwrap_err();
    assert!(
        error.to_string().contains("duplicate field `wireName`"),
        "{error}"
    );
}

#[test]
fn generic_middle_types_default_absent_fields() {
    let id = GtsInstanceId::new(LeafV1::gts_type_id().as_ref(), "x.test.derived_opt.leaf.v1");
    let record: RecordV1<WrapperV1<LeafV1>> = serde_json::from_value(json!({
        "id": id,
        "data": { "inner": { "size": 2 } },
    }))
    .unwrap();
    assert_eq!(record.data.label, None);
    assert_eq!(record.data.depth, 4);
    assert_eq!(record.data.inner.size, 2);
    assert_eq!(record.data.inner.comment, None);
}

fn swap_record(data: SwapV1) -> RecordV1<SwapV1> {
    let id = GtsInstanceId::new(SwapV1::gts_type_id().as_ref(), "x.test.derived_opt.swap.v1");
    RecordV1 { id, data }
}

#[test]
fn swapped_renames_read_back_in_declaration_order() {
    let record = swap_record(SwapV1 {
        b: "first".to_owned(),
        a: "second".to_owned(),
        size: 1,
    });
    let json = serde_json::to_value(&record).unwrap();
    assert_eq!(json["data"]["c"], "first", "{json}");
    assert_eq!(json["data"]["b"], "second", "{json}");
    let back: RecordV1<SwapV1> = serde_json::from_value(json).unwrap();
    assert_eq!(back.data, record.data);
}

#[test]
fn self_default_paths_name_the_struct() {
    let mut json = serde_json::to_value(swap_record(SwapV1 {
        b: "first".to_owned(),
        a: "second".to_owned(),
        size: 1,
    }))
    .unwrap();
    json["data"].as_object_mut().unwrap().remove("size");
    let back: RecordV1<SwapV1> = serde_json::from_value(json).unwrap();
    assert_eq!(back.data.size, 9);
}
