//! Test: #[gts(...)] without schema_id

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(dir_path = "schemas", description = "Test")]
pub struct UserV1 {
    pub id: String,
}

fn main() {}
