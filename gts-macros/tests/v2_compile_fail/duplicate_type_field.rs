//! Test: Two fields with #[gts(type_field)]

use gts::GtsSchemaId;
use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.event.v1~",
    description = "Event"
)]
pub struct EventV1 {
    #[gts(type_field)]
    pub event_type: GtsSchemaId,
    #[gts(type_field)]
    pub alt_type: GtsSchemaId,
}

fn main() {}
