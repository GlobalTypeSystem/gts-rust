//! Test: root struct without #[gts(type_field)] or #[gts(instance_id)]

use gts_macros::GtsSchema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.event.v1~",
    description = "Event without identity"
)]
pub struct EventV1 {
    pub subject: String,
    pub description: String,
}

fn main() {}
