//! Schema types expose no output-file metadata.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    base = true,
    type_id = "gts.x.app.entities.user.v1~",
    description = "User entity",
    properties = "id"
)]
pub struct UserV1 {
    pub id: gts::GtsInstanceId,
}

fn main() {
    let _ = UserV1::GTS_SCHEMA_FILE_PATH;
}
