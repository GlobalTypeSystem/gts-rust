//! Test: Enums are not supported

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    base = true,
    type_id = "gts.x.app.entities.status.v1~",
    description = "Status enum",
    properties = "Active"
)]
pub enum Status {
    Active,
    Inactive,
}

fn main() {}
