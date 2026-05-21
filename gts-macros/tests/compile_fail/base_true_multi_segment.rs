//! Test: base = true with multi-segment schema_id should fail
//! A base type must have exactly 1 segment (no parent)

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "This should fail",
    properties = "id"
)]
pub struct InvalidBaseV1 {
    pub id: String,
}

fn main() {}
