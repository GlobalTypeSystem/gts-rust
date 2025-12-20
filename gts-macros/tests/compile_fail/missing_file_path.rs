//! Test: Missing required attribute file_path

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    schema_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id"
)]
pub struct User {
    pub id: String,
}

fn main() {}
