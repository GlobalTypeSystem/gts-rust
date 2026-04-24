//! Test: Malformed schema_id string

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "not-a-valid-id~",
    description = "Bad ID"
)]
pub struct BadV1 {
    pub id: String,
}

fn main() {}
