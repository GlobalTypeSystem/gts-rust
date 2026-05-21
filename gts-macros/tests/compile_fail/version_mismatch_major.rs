//! Test: Struct version suffix doesn't match schema_id major version
//! BaseEventV2 should not work with v1~ schema

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "id"
)]
pub struct BaseEventV2 {
    pub id: String,
}

fn main() {}
