// Should trigger: hardcoded "gts." prefix inside struct_to_gts_schema type_id
use gts::gts::GtsTypeId;
use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.events.type.v1~",
    description = "Test base type",
    properties = "event_type,id",
)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TestBaseV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsTypeId,
    pub id: String,
    pub payload: P,
}

fn main() {
    let _ = <TestBaseV1<()> as GtsSchema>::TYPE_ID;
}
