//! Test: Tuple structs are not supported

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    file_path = "schemas/data.v1~.schema.json",
    schema_id = "gts.x.app.entities.data.v1~",
    description = "Data entity",
    properties = "0"
)]
pub struct Data(String);

fn main() {}
