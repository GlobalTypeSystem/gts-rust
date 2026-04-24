//! Test: #[gts(nonexistent)] fails fast on typos

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.user.v1~",
    description = "User",
    nonexistent = "value"
)]
pub struct UserV1 {
    pub id: String,
}

fn main() {}
