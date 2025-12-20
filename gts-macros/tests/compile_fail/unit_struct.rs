//! Test: Unit structs are not supported

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    file_path = "schemas/empty.v1~.schema.json",
    schema_id = "gts.x.app.entities.empty.v1~",
    description = "Empty entity",
    properties = ""
)]
pub struct Empty;

fn main() {}
