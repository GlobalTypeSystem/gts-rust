// Golden case: `traits_schema = true` emits the boolean `true` subschema, which
// admits any trait values. The host also carries arbitrary `traits` to show they
// pass validation against the unconstrained `true` shape.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.golden.open.v1~",
    description = "Open traits host",
    properties = "id",
    traits_schema = true,
    traits = serde_json::json!({
        "anything": "goes",
        "count": 7
    }),
)]
#[derive(Debug, JsonSchema)]
pub struct OpenHostV1 {
    pub id: GtsInstanceId,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        OpenHostV1::TYPE_ID.to_owned(),
        OpenHostV1::gts_schema_with_refs(),
    )]
}
