//! A base type has no ancestors to constrain.

use gts::{GtsInstanceId, GtsTypeId};
use gts_macros::struct_to_gts_schema;

#[derive(schemars::JsonSchema)]
pub struct SubjectIdRules {
    pub subject_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,subject_type",
    overrides(EventV1 = SubjectIdRules),
)]
pub struct EventV1 {
    pub id: GtsInstanceId,
    pub subject_id: String,
    pub subject_type: GtsTypeId,
}

fn main() {}
