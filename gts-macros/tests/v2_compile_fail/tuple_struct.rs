//! Test: #[derive(GtsSchema)] on tuple struct

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.data.v1~",
    description = "Data entity"
)]
pub struct DataV1(String);

fn main() {}
