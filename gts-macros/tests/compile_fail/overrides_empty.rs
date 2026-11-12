//! An empty `overrides()` is rejected.

use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.subject.v1~",
    description = "Base",
    properties = "subject_type,id,payload",
)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    pub id: String,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.app.ovr.subject.v1~x.app.ovr.regional.v1~",
    description = "Middle",
    properties = "region,details",
)]
pub struct RegionalSubjectV1<D> {
    pub region: String,
    pub details: D,
}

#[derive(schemars::JsonSchema)]
pub struct IdRules {
    pub id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.app.ovr.subject.v1~x.app.ovr.regional.v1~x.app.ovr.leaf.v1~",
    description = "Derived",
    properties = "",
    overrides(),
)]
pub struct LeafV1;

fn main() {}
