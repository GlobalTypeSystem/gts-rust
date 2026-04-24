//! Test: #[gts(...)] without dir_path

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(schema_id = "gts.x.app.entities.user.v1~", description = "User")]
pub struct UserV1 {
    pub id: String,
}

fn main() {}
