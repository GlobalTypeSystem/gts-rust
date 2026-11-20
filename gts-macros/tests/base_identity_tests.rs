#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Base identity validation and GTS schema naming: regressions from the #160 review.

use gts::{GtsConfig, GtsEntity, GtsSchema, GtsTypeId};
use gts_macros::{gts_instance, struct_to_gts_schema};
use serde_json::json;

fn selected_id_field(json: &serde_json::Value) -> Option<String> {
    GtsEntity::new(
        None,
        None,
        json,
        Some(&GtsConfig::default()),
        None,
        false,
        String::new(),
        None,
        None,
    )
    .selected_entity_field
}

fn property_names(schema: &serde_json::Value) -> Vec<&str> {
    let mut names: Vec<_> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    names.sort_unstable();
    names
}

// The #72 workaround: a skipped, defaulted `gts_type` beside domain fields with identity names.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.identity.legacy_type.v1~"),
    description = "Legacy skipped type with a domain `type`",
    properties = "kind"
)]
pub struct LegacyDomainTypeV1 {
    #[serde(skip_serializing, default = "legacy_domain_type")]
    gts_type: GtsTypeId,
    #[serde(rename = "type")]
    pub kind: String,
}

fn legacy_domain_type() -> GtsTypeId {
    LegacyDomainTypeV1::gts_type_id().clone()
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.identity.legacy_gts_id.v1~"),
    description = "Legacy skipped type with a domain `gts_id`",
    properties = "gts_id"
)]
pub struct LegacyDomainGtsIdV1 {
    #[serde(skip_serializing, default = "legacy_domain_gts_id")]
    gts_type: GtsTypeId,
    pub gts_id: String,
}

fn legacy_domain_gts_id() -> GtsTypeId {
    LegacyDomainGtsIdV1::gts_type_id().clone()
}

#[test]
fn legacy_skipped_type_keeps_domain_identity_names() {
    let json = json!({"type": "local"});
    let value: LegacyDomainTypeV1 = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(&value.gts_type, LegacyDomainTypeV1::gts_type_id());
    assert_eq!(value.gts_instance_json(), json);

    let json = json!({"gts_id": "local"});
    let value: LegacyDomainGtsIdV1 = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(&value.gts_type, LegacyDomainGtsIdV1::gts_type_id());
    assert_eq!(value.gts_instance_json(), json);
}

// Runtime reads IDs only from strings, so a numeric `$id` cannot shadow `id`.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.identity.numeric_alias.v1~"),
    description = "GTS ID beside a numeric `$id`",
    properties = "id,local,count"
)]
pub struct NumericAliasV1 {
    pub id: gts::GtsInstanceId,
    #[serde(rename = "$id")]
    pub local: Option<::core::primitive::u32>,
    #[serde(rename = "gtsIid")]
    pub count: u32,
}

#[test]
fn numeric_id_alias_does_not_shadow_gts_instance_id() {
    let value = NumericAliasV1 {
        id: NumericAliasV1::gts_make_instance_id("x.test.identity.one.v1"),
        local: Some(7),
        count: 8,
    };
    let json = value.gts_instance_json();
    assert_eq!(json["$id"], 7);
    assert_eq!(selected_id_field(&json).as_deref(), Some("id"));
}

type TagList = Vec<String>;

#[struct_to_gts_schema(
    dir_path = "schemas", base = true, type_id = gts_id!("x.test.identity.collections.v1~"),
    description = "Collection aliases beside the instance ID", properties = "id,tags,labels,pair,bytes"
)]
pub struct CollectionsV1 {
    pub id: gts::GtsInstanceId,
    #[serde(rename = "gtsId")]
    pub tags: TagList,
    #[serde(rename = "gtsIid")]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    #[serde(rename = "gtsOid")]
    pub pair: (String, String),
    #[serde(rename = "$id")]
    pub bytes: [u8; 2],
}

#[test]
fn collection_aliases_do_not_hide_the_instance_id() {
    let value = CollectionsV1 {
        id: CollectionsV1::gts_make_instance_id("x.test.identity.one.v1"),
        tags: vec!["tag".into()],
        labels: Some(std::collections::BTreeMap::new()),
        pair: ("a".into(), "b".into()),
        bytes: [1, 2],
    };
    let json = value.gts_instance_json();
    assert_eq!(json["gtsId"], json!(["tag"]));
    assert_eq!(selected_id_field(&json).as_deref(), Some("id"));
}

// Fields removed by `cfg` or renamed by a disabled `cfg_attr` do not compete.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.identity.disabled_cfg.v1~"),
    description = "Disabled fields beside a GTS ID",
    properties = "id,other"
)]
pub struct DisabledCfgV1 {
    #[cfg(any())]
    pub gts_id: gts::GtsInstanceId,
    pub id: gts::GtsInstanceId,
    #[cfg_attr(any(), serde(rename = "gtsId"))]
    pub other: gts::GtsInstanceId,
}

#[test]
fn disabled_cfg_fields_do_not_conflict() {
    let id = DisabledCfgV1::gts_make_instance_id("x.test.identity.one.v1");
    let value = DisabledCfgV1 {
        id: id.clone(),
        other: id,
    };
    let json = value.gts_instance_json();
    assert_eq!(json.as_object().unwrap().len(), 2);
    assert!(json.get("other").is_some());
    assert_eq!(selected_id_field(&json).as_deref(), Some("id"));
}

#[test]
fn skipped_field_rename_keeps_real_property() {
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = gts_id!("x.test.identity.skip_rename.v1~"),
        description = "Skipped field whose input name matches a real field",
        properties = "id,value"
    )]
    struct SkipRenameV1 {
        id: gts::GtsInstanceId,
        #[serde(skip, rename(serialize = "unused", deserialize = "value"))]
        hidden: u32,
        value: u32,
    }

    let value = SkipRenameV1 {
        id: SkipRenameV1::gts_make_instance_id("x.test.identity.one.v1"),
        hidden: 1,
        value: 2,
    };
    assert_eq!(value.hidden, 1);
    let json = value.gts_instance_json();
    let schema = gts::gts_schema_for!(SkipRenameV1);
    assert_eq!(property_names(&schema), ["id", "value"]);
    assert_eq!(schema["required"], json!(["id", "value"]));
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&json)
        .unwrap();
}

#[test]
fn container_schemars_rename_all_overrides_serde_names() {
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = gts_id!("x.test.identity.schemars_rename_all.v1~"),
        description = "Explicit schemars naming over directional serde rules",
        properties = "item_count,total"
    )]
    #[schemars(rename_all = "snake_case")]
    #[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
    struct SchemarsRenameAllV1 {
        item_count: u32,
        #[serde(rename(serialize = "Total", deserialize = "total_in"))]
        total: u32,
    }

    let schema = SchemarsRenameAllV1::gts_schema_with_refs();
    // The container override wins over serde rules; an explicit field rename still applies.
    assert_eq!(property_names(&schema), ["Total", "item_count"]);
}

// `gts_instance!` picks the ID by Rust field name, independent of JSON aliases.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.identity.typed_literal.v1~"),
    description = "Rust field named like a JSON alias but serialized under another name",
    properties = "id,gts_iid"
)]
pub struct TypedLiteralV1 {
    pub id: gts::GtsInstanceId,
    #[serde(rename = "counter")]
    pub gts_iid: u32,
}

#[test]
fn typed_literal_uses_rust_id_field_names() {
    let value: TypedLiteralV1 = gts_instance!(TypedLiteralV1 {
        id: "gts.x.test.identity.typed_literal.v1~x.test.identity.one.v1",
        gts_iid: 7,
    });
    assert_eq!(value.gts_iid, 7);
    assert_eq!(value.gts_instance_json()["counter"], 7);
}

#[test]
fn schemars_naming_applies_per_direction() {
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = gts_id!("x.test.identity.one_side.v1~"),
        description = "One-sided field renames under a schemars container rule",
        properties = "id,item_count,other_count"
    )]
    #[derive(schemars::JsonSchema)]
    #[serde(rename_all(serialize = "camelCase", deserialize = "SCREAMING_SNAKE_CASE"))]
    #[schemars(rename_all = "snake_case")]
    struct OneSideV1 {
        id: gts::GtsInstanceId,
        #[serde(rename(deserialize = "readCount"))]
        item_count: u32,
        #[serde(rename(serialize = "writeOther"))]
        other_count: u32,
    }

    let output = schemars::generate::SchemaSettings::draft07()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<OneSideV1>();
    let schema = OneSideV1::gts_schema_with_refs();
    assert_eq!(property_names(&schema), ["id", "item_count", "writeOther"]);
    assert_eq!(property_names(output.as_value()), property_names(&schema));
}
