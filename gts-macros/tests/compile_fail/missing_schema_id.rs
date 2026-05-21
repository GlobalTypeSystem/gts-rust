//! Test: Missing required attribute type_id (formerly schema_id)

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    description = "User entity",
    properties = "id"
)]
pub struct User {
    pub id: String,
}

fn main() {}
