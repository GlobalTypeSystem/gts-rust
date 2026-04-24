//! Test: Struct V1 with schema v2~

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.user.v2~",
    description = "User"
)]
pub struct UserV1 {
    pub id: String,
}

fn main() {}
