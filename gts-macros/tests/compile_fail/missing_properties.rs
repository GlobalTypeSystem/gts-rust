//! Test: Missing required attribute properties

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.user.v1~",
    description = "User entity"
)]
pub struct User {
    pub id: String,
}

fn main() {}
