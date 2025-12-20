//! Test: Missing required attribute schema_id

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    file_path = "schemas/user.v1~.schema.json",
    description = "User entity",
    properties = "id"
)]
pub struct User {
    pub id: String,
}

fn main() {}
