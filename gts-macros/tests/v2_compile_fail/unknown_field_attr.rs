//! Test: Unknown #[gts(...)] field attribute

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.user.v1~",
    description = "User"
)]
pub struct UserV1 {
    #[gts(nonexistent)]
    pub id: String,
}

fn main() {}
