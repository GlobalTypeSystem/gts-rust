//! Test: Enums are not supported

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    file_path = "schemas/status.v1~.schema.json",
    schema_id = "gts.x.app.entities.status.v1~",
    description = "Status enum",
    properties = "Active"
)]
pub enum Status {
    Active,
    Inactive,
}

fn main() {}
