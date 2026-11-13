//! A base type has no ancestors to constrain.

use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;

#[derive(schemars::JsonSchema)]
pub struct IdRules {
    pub id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.subject.v1~",
    description = "Base",
    properties = "subject_type,id",
    overrides(SubjectV1 = IdRules),
)]
pub struct SubjectV1 {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    pub id: String,
}

fn main() {}
