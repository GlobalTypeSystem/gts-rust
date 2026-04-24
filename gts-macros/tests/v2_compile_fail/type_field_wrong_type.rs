//! Test: #[gts(type_field)] on a String field (must be GtsSchemaId)

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.event.v1~",
    description = "Event"
)]
pub struct EventV1 {
    #[gts(type_field)]
    pub event_type: String,
    pub id: String,
}

fn main() {}
