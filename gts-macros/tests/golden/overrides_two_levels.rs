// Golden case: two levels. The derived type replaces the base's root `id`
// (a string of 1..=1024 chars) with a 36-char UUID.

use gts::GtsSchema;
use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr2.subject.v1~"),
    description = "Subject with a bounded id",
    properties = "subject_type,id,payload"
)]
#[derive(Debug)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    #[schemars(length(min = 1, max = 1024))]
    pub id: String,
    pub payload: P,
}

/// `/id` is a UUID.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct UuidId {
    #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
    pub id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = gts_id!("x.test.ovr2.subject.v1~x.test.ovr2.uuid_subject.v1~"),
    description = "Subject whose id is a UUID",
    properties = "label",
    overrides(SubjectV1 = UuidId)
)]
#[derive(Debug)]
pub struct UuidSubjectV1 {
    pub label: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (SubjectV1::<()>::TYPE_ID.to_owned(), SubjectV1::<()>::gts_schema_with_refs()),
        (UuidSubjectV1::TYPE_ID.to_owned(), UuidSubjectV1::gts_schema_with_refs()),
    ]
}
