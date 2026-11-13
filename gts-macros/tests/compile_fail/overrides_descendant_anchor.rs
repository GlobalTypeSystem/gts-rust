//! A descendant of the overriding type is not a valid ancestor anchor.

use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.subject.v1~",
    description = "Base",
    properties = "subject_type,id,payload"
)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    pub id: String,
    pub payload: P,
}

#[derive(schemars::JsonSchema)]
pub struct LabelRules {
    pub label: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.app.ovr.subject.v1~x.app.ovr.regional.v1~",
    description = "Incorrectly overrides its descendant",
    properties = "region,details",
    overrides(LeafV1 = LabelRules),
)]
pub struct RegionalSubjectV1<D> {
    pub region: String,
    pub details: D,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.app.ovr.subject.v1~x.app.ovr.regional.v1~x.app.ovr.leaf.v1~",
    description = "Actual descendant of RegionalSubjectV1",
    properties = "label,extra",
)]
pub struct LeafV1<E> {
    pub label: String,
    pub extra: E,
}

fn main() {}
