//! Test: Property listed in `properties` doesn't exist in struct

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    file_path = "schemas/user.v1~.schema.json",
    schema_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id,nonexistent_field"
)]
pub struct User {
    pub id: String,
}

fn main() {}
