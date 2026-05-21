//! Test: Struct has minor version but schema_id doesn't
//! BaseEventV3_0 should not work with v3~ schema

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.events.type.v3~",
    description = "Base event type",
    properties = "id"
)]
pub struct BaseEventV3_0 {
    pub id: String,
}

fn main() {}
