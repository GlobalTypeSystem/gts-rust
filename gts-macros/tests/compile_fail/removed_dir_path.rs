//! Output directories belong to the generator, not the schema declaration.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id"
)]
pub struct User {
    pub id: gts::GtsInstanceId,
}

fn main() {}
