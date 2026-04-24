//! Test: Same struct has both #[gts(type_field)] and #[gts(instance_id)]

use gts::GtsSchemaId;
use gts::GtsInstanceId;
use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.hybrid.v1~",
    description = "Hybrid"
)]
pub struct HybridV1 {
    #[gts(type_field)]
    pub event_type: GtsSchemaId,
    #[gts(instance_id)]
    pub id: GtsInstanceId,
}

fn main() {}
