//! Test: #[derive(GtsSchema)] on enum

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.status.v1~",
    description = "Status enum"
)]
pub enum StatusV1 {
    Active,
    Inactive,
}

fn main() {}
