// Golden case: `struct_to_gts_schema` preserves an explicitly open Schemars
// content model on the GTS base struct itself instead of applying its usual
// closed-root default.

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.rootopen.v1~"),
    description = "GTS base type with an explicitly open root",
    properties = "schema_type,label",
)]
#[derive(Debug)]
#[schemars(extend("additionalProperties" = true))]
pub struct RootOpenHostV1 {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub label: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        RootOpenHostV1::TYPE_ID.to_owned(),
        RootOpenHostV1::gts_schema_with_refs(),
    )]
}
