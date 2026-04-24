//! Test: Struct with 2+ type params

use gts_macros::GtsSchema;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.multi.v1~",
    description = "Too many generics"
)]
pub struct MultiV1<A, B> {
    pub a: A,
    pub b: B,
}

fn main() {}
