//! Test: Unit structs cannot have properties specified

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.app.entities.empty.v1~",
    description = "Empty entity",
    properties = "name"  // Error: unit struct can't have properties
)]
pub struct EmptyV1;

fn main() {}
