//! Test: No extends but multi-segment schema_id

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Should fail"
)]
pub struct AuditV1 {
    pub data: String,
}

fn main() {}
