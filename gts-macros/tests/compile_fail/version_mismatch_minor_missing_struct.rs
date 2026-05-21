//! Test: Struct has no minor version but schema_id has minor version
//! BaseEventV2 should not work with v2.2~ schema

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.events.type.v2.2~",
    description = "Base event type",
    properties = "id"
)]
pub struct BaseEventV2 {
    pub id: String,
}

fn main() {}
