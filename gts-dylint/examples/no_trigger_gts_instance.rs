// Should NOT trigger: gts_instance! with gts_id! for id field
#![allow(unused_imports)]
use gts::gts::GtsInstanceId;
use gts_macros::{gts_id, gts_instance, struct_to_gts_schema};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.events.type.v1~"),
    description = "Test base type",
    properties = "id",
)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TestBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

fn main() {
    let _ = gts_instance!(TestBaseV1::<()> {
        id: gts_id!("x.core.events.type.v1~demo.app.events.test.v1"),
        payload: (),
    });
}
