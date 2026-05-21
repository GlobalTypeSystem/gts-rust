//! Test: Missing required attribute base

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    type_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id"
)]
pub struct User {
    pub id: String,
}

fn main() {}
