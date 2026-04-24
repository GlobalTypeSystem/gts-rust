//! Test: #[gts(...)] without description

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(dir_path = "schemas", schema_id = "gts.x.app.entities.user.v1~")]
pub struct UserV1 {
    pub id: String,
}

fn main() {}
