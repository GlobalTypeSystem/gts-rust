//! Test: GTS schema ID missing 'gts.' prefix

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "x.core.events.type.v1~",
    description = "Missing gts. prefix",
    properties = "id"
)]
pub struct InvalidPrefixV1 {
    pub id: gts::GtsInstanceId,
}

fn main() {}
