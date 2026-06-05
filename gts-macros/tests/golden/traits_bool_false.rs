// Golden case: `traits_schema = false` emits the boolean `false` subschema,
// which prohibits any traits on this host and its descendants.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.golden.closed.v1~",
    description = "Traits-prohibited host",
    properties = "id",
    traits_schema = false
)]
#[derive(Debug, JsonSchema)]
pub struct ClosedHostV1 {
    pub id: GtsInstanceId,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        ClosedHostV1::TYPE_ID.to_owned(),
        ClosedHostV1::gts_schema_with_refs(),
    )]
}
