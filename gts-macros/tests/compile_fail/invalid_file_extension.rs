//! Test: Missing property in struct should cause compile error

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id,nonexistent"
)]
pub struct User {
    pub id: String,
}

fn main() {}
